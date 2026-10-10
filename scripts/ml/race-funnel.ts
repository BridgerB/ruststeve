// Race funnel from race.db milestones (works for any binary: old-tree races have no event rows).
//
//   node scripts/ml/race-funnel.ts <race.db> [<race.db> …]
//
// Per db (one race each): per bot the furthest milestone reached, deaths by cause (death events' detail),
// time to the first `Build Nether Portal` milestone (the step being REACHED, not done), and the gap between
// consecutive race.db ticks (written per step, so this is step cadence, not tick-loop health).
// Then the funnel: for each milestone, how many bots reached it.
import { DatabaseSync } from "node:sqlite";

const ORDER = [
  "Gather Wood", "Craft Planks", "Craft Crafting Table", "Craft Sticks", "Craft Wooden Pickaxe", "Mine Cobblestone",
  "Craft Stone Pickaxe", "Craft Stone Sword", "Craft Furnace", "Mine Coal", "Mine Iron Ore", "Smelt Iron",
  "Craft Iron Pickaxes (2)", "Craft Buckets", "Fill Water Buckets", "Get Flint and Steel", "Gather Build Blocks",
  "Build Nether Portal", "Enter Nether",
  // Post-portal steps (cycle 6): a milestone names the step REACHED, so "Find Nether Fortress" means in the Nether.
  // Without these, i11 rust-race-004 (in the Nether at 78 min) reported furthest = Smelt Iron.
  "Find Nether Fortress", "Kill a Blaze", "Gather 7 Blaze Rods",
];

const report = (path: string) => {
  const db = new DatabaseSync(path, { readOnly: true });
  const start = db.prepare("SELECT MIN(ts_ms) t, MAX(ts_ms) e FROM ticks").get() as { t: number; e: number };
  const bots = (db.prepare("SELECT DISTINCT bot FROM ticks ORDER BY bot").all() as { bot: string }[]).map((r) => r.bot);
  const reached = new Map<string, number>();
  let inNether = 0;
  console.log(`\n== ${path}  (${((start.e - start.t) / 60000).toFixed(0)} min of ticks, ${bots.length} bots)`);
  for (const bot of bots) {
    const ms = db.prepare("SELECT event, MIN(ts_ms) t FROM events WHERE bot=? AND category='milestone' GROUP BY event").all(bot) as { event: string; t: number }[];
    const furthest = ms.reduce((best, m) => Math.max(best, ORDER.indexOf(m.event)), -1);
    for (let i = 0; i <= furthest; i++) reached.set(ORDER[i], (reached.get(ORDER[i]) ?? 0) + 1);
    const portal = ms.find((m) => m.event === "Build Nether Portal");
    // Ground truth for the Nether: the first tick the bot wrote in the_nether.
    const nether = db.prepare("SELECT MIN(ts_ms) t FROM ticks WHERE bot=? AND dim LIKE '%nether%'").get(bot) as { t: number | null };
    if (nether.t) inNether++;
    const deaths = db.prepare("SELECT COALESCE(detail,'?') d, COUNT(*) n FROM events WHERE bot=? AND category='death' GROUP BY d").all(bot) as { d: string; n: number }[];
    const gaps = (db.prepare("SELECT ts_ms FROM ticks WHERE bot=? ORDER BY ts_ms").all(bot) as { ts_ms: number }[])
      .map((r, i, a) => (i ? r.ts_ms - a[i - 1].ts_ms : 0)).slice(1).sort((a, b) => a - b);
    const q = (p: number) => (gaps.length ? (gaps[Math.floor(p * (gaps.length - 1))] / 1000).toFixed(1) : "-");
    console.log(
      `${bot}: furthest=${furthest >= 0 ? ORDER[furthest] : "none"}` +
        ` portal_milestone=${portal ? `${((portal.t - start.t) / 60000).toFixed(0)} min` : "-"}` +
        ` in_nether=${nether.t ? `${((nether.t - start.t) / 60000).toFixed(0)} min` : "-"}` +
        ` deaths=${deaths.reduce((s, d) => s + d.n, 0)} [${deaths.map((d) => `${d.d}×${d.n}`).join("; ")}]` +
        ` tick_gap_s p50=${q(0.5)} p90=${q(0.9)} max=${q(1)}`,
    );
  }
  console.log(`funnel: ${ORDER.map((m) => `${m}=${reached.get(m) ?? 0}`).join(" | ")} | in the Nether (ticks)=${inNether}`);
};

for (const p of process.argv.slice(2)) report(p);
