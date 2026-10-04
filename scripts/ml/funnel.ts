// The funnel (cycle 4, Part 10.1): every skill, its attempts, pass rate with the Wilson 95%
// interval, median time of passes, deaths, source and whether the postcondition was ground truth.
//
//   node scripts/ml/funnel.ts [attempts file or dir] [--run <run_id prefix>] [--build <build>]
// Rows come through rows.ts: data/attempts/*.jsonl (one writer each) + the read-only data/attempts.jsonl.
import { readRows, unparseable } from "./rows.ts";

type Row = { skill?: string; step_id?: string; outcome?: string; source?: string; duration_s?: number; deaths?: number; run_id?: string; build?: string };

const args = process.argv.slice(2);
const flag = (f: string) => (args.includes(f) ? args[args.indexOf(f) + 1] : undefined);
const file = args.find((a, i) => !a.startsWith("--") && !(i > 0 && args[i - 1].startsWith("--")));
const run = flag("--run");
const build = flag("--build");

const SKILLS = ["water", "wood_stone_tools", "iron", "kit_craft", "portal", "fortress", "blaze_rods", "pearls", "stronghold", "end_entry", "crystals", "dragon"];
// Postconditions checked from ground truth by the harness: gym pass checks read server state
// (RCON / block reads); step-loop completion reads the bot's synced inventory/state.
const GROUND = (source: string) => (source === "gym" ? "gym check" : "bot state");

// Water gyms run gather_wood, so their rows carry skill wood_stone_tools; their purpose is water
// survival, so they count under "water" (the cycle-4 funnel mixed them into wood_stone_tools).
const rows: Row[] = readRows<Row & { gym_slug?: string }>(file).map((r) => (r.gym_slug?.startsWith("water_") ? { ...r, skill: "water" } : r));
// Setup skips and trials I stopped myself ("aborted") measure nothing about the bot: excluded.
const sel = rows.filter((r) => r.outcome !== "skipped" && r.outcome !== "aborted" && !((r as { reason?: string }).reason ?? "").includes("trial skipped") && (!run || (r.run_id ?? "").startsWith(run)) && (!build || r.build === build) && SKILLS.includes(r.skill ?? ""));

const z = 1.96;
const wilson = (k: number, n: number) => {
  if (n === 0) return "—";
  const p = k / n;
  const c = (p + (z * z) / (2 * n)) / (1 + (z * z) / n);
  const h = (z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))) / (1 + (z * z) / n);
  return `${Math.round(p * 100)}% [${Math.round(Math.max(0, c - h) * 100)}%, ${Math.round(Math.min(1, c + h) * 100)}%]`;
};
const median = (xs: number[]) => {
  if (!xs.length) return "—";
  const s = [...xs].sort((a, b) => a - b);
  return (s[Math.floor((s.length - 1) / 2)] / 60).toFixed(1);
};

console.log(`funnel from ${file ?? "data/attempts/ + data/attempts.jsonl"} (${sel.length} rows${run ? `, run ${run}` : ""}${build ? `, build ${build}` : ""}${unparseable ? `, ${unparseable} unparseable lines skipped` : ""})`);
console.log(["skill".padEnd(18), "src".padEnd(5), "n".padStart(4), "ok".padStart(4), "pass (Wilson 95%)".padEnd(18), "med_ok_min".padStart(10), "deaths".padStart(6), "timeouts".padStart(8), "postcondition"].join("  "));
for (const skill of SKILLS) {
  const bySrc = ["race", "gym"].map((src) => [src, sel.filter((r) => r.skill === skill && r.source === src)] as const);
  let printed = false;
  for (const [src, rs] of bySrc) {
    if (!rs.length) continue;
    printed = true;
    const ok = rs.filter((r) => r.outcome === "ok");
    console.log([
      skill.padEnd(18), src.padEnd(5), String(rs.length).padStart(4), String(ok.length).padStart(4), wilson(ok.length, rs.length).padEnd(18),
      median(ok.map((r) => r.duration_s ?? 0)).padStart(10), String(rs.reduce((a, r) => a + (r.deaths ?? 0), 0)).padStart(6),
      String(rs.filter((r) => r.outcome === "timeout").length).padStart(8), GROUND(src),
    ].join("  "));
  }
  if (!printed) console.log(`${skill.padEnd(18)}  —      0     0  —`);
}
