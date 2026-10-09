// Bayesian build comparison with a sequential stop rule (cycle 4, Part 5.2; cycle 5, decision 2).
//
//   node scripts/ml/compare.ts <skill|step_id> <buildA> <buildB> [attempts file or dir] [--source gym|race] [--clean]
//   node scripts/ml/compare.ts --paired <gym.db> <slug> <buildA> <buildB> [--exclude A11]   (cycle 6, paired.ts)
//
// Beta(successes + 1, failures + 1) per build; P(B > A) from 10,000 posterior draws. Success is any
// pass (reached the goal, deaths allowed: cycle 5's primary metric); --clean counts only passes with
// 0 deaths. Decision (cycle 5): no stop before 12 trials per arm; then "B wins" at P > 0.95,
// "B loses" at P < 0.05; cap 18 per arm ("cap reached, undecided"). Rows are read through rows.ts
// (data/attempts/*.jsonl + the read-only data/attempts.jsonl).
import { beta } from "./bandit.ts";
import { paired } from "./paired.ts";
import { readRows, unparseable } from "./rows.ts";

type Row = { deaths?: number; build?: string; skill?: string; step_id?: string; outcome?: string; source?: string; world_seed?: number | null; pos?: number[] };

const args = process.argv.slice(2);
if (args[0] === "--paired") {
  paired(args.slice(1));
  process.exit(0);
}
const clean = args.includes("--clean");
const srcIdx0 = args.filter((a) => a !== "--clean");
args.length = 0;
args.push(...srcIdx0);
const srcIdx = args.indexOf("--source");
const source = srcIdx >= 0 ? args[srcIdx + 1] : undefined;
const pos = srcIdx < 0 ? args : args.filter((_, i) => i !== srcIdx && i !== srcIdx + 1);
const [target, buildA, buildB, file] = pos;
if (!target || !buildA || !buildB) {
  console.error("usage: node scripts/ml/compare.ts <skill|step_id> <buildA> <buildB> [attempts file or dir] [--source gym|race] [--clean]");
  process.exit(2);
}

const rows: Row[] = readRows<Row>(file);
// Setup skips and trials I stopped myself measure nothing about the bot: excluded.
const measured = (r: Row & { reason?: string }) => r.outcome !== "skipped" && r.outcome !== "aborted" && !(r.reason ?? "").includes("trial skipped");
const pick = (b: string) => rows.filter((r) => measured(r) && r.build === b && (r.skill === target || r.step_id === target) && (!source || r.source === source));
const A = pick(buildA);
const B = pick(buildB);
const good = (r: Row) => r.outcome === "ok" && (!clean || (r.deaths ?? 0) === 0);
const k = (rs: Row[]) => rs.filter(good).length;
const [kA, nA, kB, nB] = [k(A), A.length, k(B), B.length];

const draws = 10000;
let wins = 0;
for (let i = 0; i < draws; i++) if (beta(kB + 1, nB - kB + 1) > beta(kA + 1, nA - kA + 1)) wins++;
const p = wins / draws;
const [minN, cap] = [12, 18];
const enough = nA >= minN && nB >= minN;
const decision = enough && p > 0.95 ? "B wins (stop)" : enough && p < 0.05 ? "B loses (stop)" : nA >= cap && nB >= cap ? "cap reached, undecided (stop)" : `continue (needs ≥ ${minN} per arm, cap ${cap})`;

const z = 1.96;
const wilson = (kk: number, n: number) => {
  if (n === 0) return "n/a";
  const ph = kk / n;
  const centre = (ph + (z * z) / (2 * n)) / (1 + (z * z) / n);
  const half = (z * Math.sqrt((ph * (1 - ph)) / n + (z * z) / (4 * n * n))) / (1 + (z * z) / n);
  return `${Math.round(ph * 100)}% [${Math.round(Math.max(0, centre - half) * 100)}%, ${Math.round(Math.min(1, centre + half) * 100)}%]`;
};
console.log(`${target}${source ? ` (${source})` : ""}${clean ? " — clean passes (0 deaths)" : " — any pass (deaths allowed)"}${unparseable ? ` [${unparseable} unparseable lines skipped]` : ""}`);
console.log(`  A ${buildA}: ${kA}/${nA}  ${wilson(kA, nA)}  posterior mean ${((kA + 1) / (nA + 2)).toFixed(3)}`);
console.log(`  B ${buildB}: ${kB}/${nB}  ${wilson(kB, nB)}  posterior mean ${((kB + 1) / (nB + 2)).toFixed(3)}`);
console.log(`  P(B > A) = ${p.toFixed(3)}  →  ${decision}`);

// Paired by seed (+ start position): the same seed/spawn/kit is a comparable trial.
const key = (r: Row) => `${r.world_seed ?? "?"}@${(r.pos ?? []).join(",")}`;
const byKey = (rs: Row[]) => new Map(rs.map((r) => [key(r), good(r)]));
const mA = byKey(A);
const mB = byKey(B);
let both = 0, bOnly = 0, aOnly = 0, same = 0;
for (const [kk, okA] of mA) {
  if (!mB.has(kk)) continue;
  both++;
  const okB = mB.get(kk)!;
  if (okA === okB) same++;
  else if (okB) bOnly++;
  else aOnly++;
}
console.log(`  paired seeds: ${both} (same ${same}, B-only pass ${bOnly}, A-only pass ${aOnly})`);
