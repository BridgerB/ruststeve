// Bayesian build comparison with a sequential stop rule (cycle 4, Part 5.2).
//
//   node scripts/ml/compare.ts <skill|step_id> <buildA> <buildB> [attempts.jsonl] [--source gym|race]
//
// Beta(successes + 1, failures + 1) per build; P(B > A) from 10,000 paired posterior draws.
// Decision: "B wins" at P > 0.95, "B loses" at P < 0.05, else "continue" until 30 trials each
// ("cap reached, undecided"). Paired: for seeds both builds ran, how many differ and which way.
import { existsSync, readFileSync } from "node:fs";
import { beta } from "./bandit.ts";

type Row = { deaths?: number; build?: string; skill?: string; step_id?: string; outcome?: string; source?: string; world_seed?: number | null; pos?: number[] };

const args = process.argv.slice(2);
// --clean: success = ok with 0 deaths (the portal gate counts clean passes).
const clean = args.includes("--clean");
const srcIdx0 = args.filter((a) => a !== "--clean");
args.length = 0;
args.push(...srcIdx0);
const srcIdx = args.indexOf("--source");
const source = srcIdx >= 0 ? args[srcIdx + 1] : undefined;
const pos = srcIdx < 0 ? args : args.filter((_, i) => i !== srcIdx && i !== srcIdx + 1);
const [target, buildA, buildB, file = "data/attempts.jsonl"] = pos;
if (!target || !buildA || !buildB) {
  console.error("usage: node scripts/ml/compare.ts <skill|step_id> <buildA> <buildB> [attempts.jsonl] [--source gym|race]");
  process.exit(2);
}

const rows: Row[] = existsSync(file)
  ? readFileSync(file, "utf8").split("\n").filter((l) => l.trim()).flatMap((l) => {
      try {
        return [JSON.parse(l)];
      } catch {
        return [];
      }
    })
  : [];
// Setup skips (e.g. `no land within ±480 — trial skipped`) measure nothing about the bot: excluded.
const measured = (r: Row & { reason?: string }) => r.outcome !== "skipped" && !(r.reason ?? "").includes("trial skipped");
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
const cap = 30;
const decision = p > 0.95 ? "B wins (stop)" : p < 0.05 ? "B loses (stop)" : nA >= cap && nB >= cap ? "cap reached, undecided (stop)" : "continue";

const z = 1.96;
const wilson = (kk: number, n: number) => {
  if (n === 0) return "n/a";
  const ph = kk / n;
  const centre = (ph + (z * z) / (2 * n)) / (1 + (z * z) / n);
  const half = (z * Math.sqrt((ph * (1 - ph)) / n + (z * z) / (4 * n * n))) / (1 + (z * z) / n);
  return `${Math.round(ph * 100)}% [${Math.round(Math.max(0, centre - half) * 100)}%, ${Math.round(Math.min(1, centre + half) * 100)}%]`;
};
console.log(`${target}${source ? ` (${source})` : ""}${clean ? " — clean passes (0 deaths)" : ""}`);
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
