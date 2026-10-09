// Bot-hours past budget per race (the cycle-5 report's measure, checked in for cycle 6).
//   node scripts/ml/race-budget.ts <label>=<race.db> [<label>=<race.db> …]
// For every step span in v_step_spans, the seconds beyond the step's budget (gather_wood 360, mine_iron 1200,
// mine_coal 600, build_nether_portal 1500, find_fortress 900, every other step 600), summed per step and per bot.
import { DatabaseSync } from "node:sqlite";

const B: Record<string, number> = { gather_wood: 360, mine_iron: 1200, mine_coal: 600, build_nether_portal: 1500, find_fortress: 900 };
for (const arg of process.argv.slice(2)) {
  const [name, f] = arg.includes("=") ? arg.split("=") : [arg, arg];
  const db = new DatabaseSync(f, { readOnly: true });
  const spans = db.prepare("SELECT bot, step_id, secs FROM v_step_spans").all() as { bot: string; step_id: string; secs: number }[];
  const per = new Map<string, number>();
  const perBot = new Map<string, number>();
  for (const s of spans) {
    const over = Math.max(0, s.secs - (B[s.step_id] ?? 600));
    if (!over) continue;
    per.set(s.step_id, (per.get(s.step_id) ?? 0) + over);
    const k = `${s.bot}:${s.step_id}`;
    perBot.set(k, (perBot.get(k) ?? 0) + over);
  }
  const tot = [...per.values()].reduce((a, b) => a + b, 0);
  console.log(`${name}: total ${(tot / 3600).toFixed(2)} h | ${[...per].sort((a, b) => b[1] - a[1]).map(([k, v]) => `${k} ${(v / 3600).toFixed(2)}`).join(", ")}`);
  console.log(`   by bot: ${[...perBot].sort((a, b) => b[1] - a[1]).slice(0, 8).map(([k, v]) => `${k} ${(v / 3600).toFixed(2)}`).join(", ")}`);
}
