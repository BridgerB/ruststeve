// Backfill attempt rows from data/gym.db for trials that predate the cycle-4 event log, so
// compare.ts can test a new build against an old build's recorded trials (cycle 4, Part 7).
//
//   node scripts/ml/backfill-gym.ts <slug> <build_label> <id_from> <id_to> [gym.db] [attempts.jsonl]
//
// Rows are tagged `backfill: true`. A row left `running` is a killed trial and counts as `failed`.
// Gym pass → ok, timeout → timeout, died → death, anything else → failed (the learn.rs mapping).
import { execFileSync } from "node:child_process";
import { appendFileSync } from "node:fs";

const argv = process.argv.slice(2);
// --skip-running: rows left `running` because the batch was stopped by hand are unmeasured, not failures.
const skipRunning = argv.includes("--skip-running");
const [slug, build, from, to, db = "data/gym.db", out = "data/attempts.jsonl"] = argv.filter((a) => a !== "--skip-running");
if (!slug || !build || !from || !to) {
  console.error("usage: node scripts/ml/backfill-gym.ts <slug> <build_label> <id_from> <id_to> [gym.db] [attempts.jsonl]");
  process.exit(2);
}
const skillOf = (s: string) => (["portal", "pool", "to_nether", "reach_lava"].includes(s) ? "portal" : s.startsWith("water_") ? "water" : s);
const sql = `SELECT id, ts, pass, duration_ms, COALESCE(x,0), COALESCE(y,0), COALESCE(z,0), COALESCE(outcome,''), replace(COALESCE(message,''), char(10), ' ')
             FROM gym_runs WHERE slug='${slug}' AND id BETWEEN ${Number(from)} AND ${Number(to)} ORDER BY id`;
const raw = execFileSync("sqlite3", ["-separator", "\t", db, sql], { encoding: "utf8" });
let n = 0;
for (const line of raw.split("\n").filter((l) => l.trim())) {
  const [id, ts, pass, dur, x, y, z, outcome, message] = line.split("\t");
  if (skipRunning && outcome === "running") continue;
  const o = outcome === "running" ? "failed" : pass === "1" ? "ok" : outcome === "timeout" ? "timeout" : outcome === "died" ? "death" : "failed";
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
