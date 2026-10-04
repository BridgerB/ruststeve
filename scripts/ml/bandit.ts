// Thompson-sampling bandit over argued constants (cycle 4, Part 5.3). Reference implementation;
// src/learn.rs ports the same Marsaglia–Tsang sampler and both are checked against
// mean(Beta(3, 7)) ≈ 0.3.
//
//   node scripts/ml/bandit.ts test                 sampler self-test
//   node scripts/ml/bandit.ts init [params.json]   write the cycle-4 parameter file (keeps existing posteriors)
//   node scripts/ml/bandit.ts refit [attempts.jsonl] [params.json]
//                                                  rebuild every arm's posterior from the event log
//   node scripts/ml/bandit.ts show [params.json]   posterior mean and count per arm
import { existsSync, readFileSync, writeFileSync } from "node:fs";

const stdNormal = (): number => {
  const u1 = Math.max(Number.MIN_VALUE, Math.random());
  const u2 = Math.random();
  return Math.sqrt(-2 * Math.log(u1)) * Math.cos(2 * Math.PI * u2);
};

export const gamma = (shape: number): number => {
  if (shape < 1) return gamma(shape + 1) * Math.random() ** (1 / shape);
  const d = shape - 1 / 3;
  const c = 1 / Math.sqrt(9 * d);
  for (;;) {
    const x = stdNormal();
    const v = (1 + c * x) ** 3;
    if (v <= 0) continue;
    const u = Math.max(Number.MIN_VALUE, Math.random());
    if (Math.log(u) < 0.5 * x * x + d - d * v + d * Math.log(v)) return d * v;
  }
};

export const beta = (a: number, b: number): number => {
  const x = gamma(a);
  const y = gamma(b);
  return x / (x + y);
};

type Arms = Record<string, [number, number]>;
type Param = { skills: string[]; enabled: boolean; arms: Arms; note?: string };
type Params = { params: Record<string, Param>; budgets: Record<string, number>; stall_s: Record<string, number> };

// Cycle 4's first arms (Part 5.3), all disabled until the skill's structure is right (Phase 2).
const initial = (): Params => ({
  params: {
    buckets: { skills: ["kit_craft", "portal"], enabled: false, arms: { "3": [1, 1], "4": [1, 1], "5": [1, 1] }, note: "lava buckets filled per refill" },
    cluster_min: { skills: ["portal"], enabled: false, arms: { "60": [1, 1], "100": [1, 1], "150": [1, 1] } },
    iron_band: { skills: ["iron"], enabled: false, arms: { "8,24": [1, 1], "0,16": [1, 1], "-8,8": [1, 1] } },
    retreat_hp: { skills: ["portal"], enabled: false, arms: { "10": [1, 1], "14": [1, 1] } },
    station_tiebreak: { skills: ["portal"], enabled: false, arms: { distance: [1, 1], targets: [1, 1] } },
  },
  budgets: {},
  stall_s: { default: 180, build_nether_portal: 300, find_fortress: 300 },
});

const readJsonl = (path: string): Record<string, unknown>[] =>
  existsSync(path)
    ? readFileSync(path, "utf8").split("\n").filter((l) => l.trim()).flatMap((l) => {
        try {
          return [JSON.parse(l)];
        } catch {
          return [];
        }
      })
    : [];

const [cmd = "show", a1, a2] = process.argv.slice(2);

// CLI only when run directly (compare.ts imports the sampler).
if (!import.meta.main) {
  // imported
} else if (cmd === "test") {
  const n = 10000;
  let s = 0;
  for (let i = 0; i < n; i++) s += beta(3, 7);
  const mean = s / n;
  const ok = Math.abs(mean - 0.3) < 0.02;
  console.log(`mean of ${n} Beta(3,7) draws = ${mean.toFixed(4)} (want 0.3 ± 0.02) → ${ok ? "PASS" : "FAIL"}`);
  process.exit(ok ? 0 : 1);
} else if (cmd === "init") {
  const path = a1 ?? "data/params.json";
  const fresh = initial();
  if (existsSync(path)) {
    const old = JSON.parse(readFileSync(path, "utf8")) as Params;
    for (const [k, v] of Object.entries(old.params ?? {})) if (fresh.params[k]) fresh.params[k] = { ...fresh.params[k], ...v };
    fresh.budgets = { ...fresh.budgets, ...(old.budgets ?? {}) };
    fresh.stall_s = { ...fresh.stall_s, ...(old.stall_s ?? {}) };
  }
  writeFileSync(path, JSON.stringify(fresh, null, 2));
  console.log(`wrote ${path}`);
} else if (cmd === "refit") {
  const rows = readJsonl(a1 ?? "data/attempts.jsonl");
  const path = a2 ?? "data/params.json";
  const p = JSON.parse(readFileSync(path, "utf8")) as Params;
  for (const spec of Object.values(p.params)) for (const k of Object.keys(spec.arms)) spec.arms[k] = [1, 1];
  for (const r of rows) {
    const chosen = (r.params ?? {}) as Record<string, string>;
    for (const [name, arm] of Object.entries(chosen)) {
      const ab = p.params[name]?.arms[String(arm)];
      if (!ab) continue;
      if (r.outcome === "ok") ab[0] += 1;
      else ab[1] += 1;
    }
  }
  writeFileSync(path, JSON.stringify(p, null, 2));
  console.log(`refit ${path} from ${rows.length} rows`);
} else {
  const p = JSON.parse(readFileSync(a1 ?? "data/params.json", "utf8")) as Params;
  for (const [name, spec] of Object.entries(p.params)) {
    console.log(`${name} (${spec.enabled ? "enabled" : "disabled"}; skills ${spec.skills.join(",")})`);
    for (const [arm, [a, b]] of Object.entries(spec.arms)) {
      const n = a + b - 2;
      console.log(`  ${arm.padEnd(10)} mean ${(a / (a + b)).toFixed(3)}  trials ${n}  (ok ${a - 1}, fail ${b - 1})`);
    }
  }
}
