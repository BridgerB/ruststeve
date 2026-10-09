// Backfill attempt rows from data/gym.db for trials that predate the cycle-4 event log, so
// compare.ts can test a new build against an old build's recorded trials (cycle 4, Part 7).
//
//   node scripts/ml/backfill-gym.ts <slug> <build_label> <id_from> <id_to> [gym.db] [out.jsonl] [--skip-running] [--x <min>,<max>]
//
// Rows are tagged `backfill: true` and written to their own file (default
// data/attempts/backfill-<slug>-<build>.jsonl: one writer, never the shared log). A row left `running`
// is a killed trial and counts as `failed` unless --skip-running. `aborted` rows stay `aborted`.
// --x keeps only rows whose start x lies in [min, max]: gym.db has no bot column, and concurrent
// arms are told apart by their region (x = region × 3000 …).
// Gym pass → ok, timeout → timeout, died → death, anything else → failed (the learn.rs mapping).
import { execFileSync } from "node:child_process";
import { appendFileSync, mkdirSync } from "node:fs";

const argv = process.argv.slice(2);
const flagVal = (f: string) => (argv.includes(f) ? argv[argv.indexOf(f) + 1] : undefined);
const skipRunning = argv.includes("--skip-running");
const xr = flagVal("--x")?.split(",").map(Number);
const pos = argv.filter((a, i) => a !== "--skip-running" && a !== "--x" && argv[i - 1] !== "--x");
const [slug, build, from, to, db = "data/gym.db", outArg] = pos;
if (!slug || !build || !from || !to) {
  console.error("usage: node scripts/ml/backfill-gym.ts <slug> <build_label> <id_from> <id_to> [gym.db] [out.jsonl] [--skip-running] [--x <min>,<max>]");
  process.exit(2);
}
mkdirSync("data/attempts", { recursive: true });
const out = outArg ?? `data/attempts/backfill-${slug}-${build.replace(/[^A-Za-z0-9+_-]/g, "_")}.jsonl`;
const skillOf = (s: string) => (["portal", "pool", "to_nether", "reach_lava"].includes(s) ? "portal" : s.startsWith("water_") ? "water" : s);
const sql = `SELECT id, ts, pass, duration_ms, COALESCE(x,0), COALESCE(y,0), COALESCE(z,0), COALESCE(outcome,''), replace(COALESCE(message,''), char(10), ' ')
             FROM gym_runs WHERE slug='${slug}' AND id BETWEEN ${Number(from)} AND ${Number(to)} ORDER BY id`;
const raw = execFileSync("sqlite3", ["-separator", "\t", db, sql], { encoding: "utf8" });
let n = 0;
for (const line of raw.split("\n").filter((l) => l.trim())) {
  const [id, ts, pass, dur, x, y, z, outcome, message] = line.split("\t");
  if (skipRunning && outcome === "running") continue;
  if (xr && (Number(x) < xr[0] || Number(x) > xr[1])) continue;
  const o = outcome === "running" ? "failed" : outcome === "aborted" ? "aborted" : pass === "1" ? "ok" : outcome === "timeout" ? "timeout" : outcome === "died" ? "death" : "failed";
  const deaths = Number((message.match(/deaths=(\d+)/) ?? [])[1] ?? 0);
  appendFileSync(out, JSON.stringify({
    run_id: `gym-backfill-${slug}`, bot_impl: "rs", build, world_seed: null, skill: skillOf(slug), step_id: "build_nether_portal",
    source: "gym", bot: "gym", gym_slug: slug, start_ms: Number(ts), duration_s: Math.round(Number(dur) / 100) / 10,
    outcome: o, reason: outcome === "running" ? "killed before finishing" : message.slice(0, 160), deaths, death_cause: deaths ? "see server log" : null,
    pos: [Number(x), Number(y), Number(z)], params: {}, context: null, backfill: true, gym_id: Number(id),
  }) + "\n");
  n++;
}
console.log(`backfilled ${n} '${slug}' rows (ids ${from}–${to}) as build ${build} into ${out}`);
