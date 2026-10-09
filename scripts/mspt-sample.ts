// Decision 10 (cycle 6): sample a server's tick times while a gym batch runs, before trusting it.
//   node scripts/mspt-sample.ts <rcon-port> <minutes> > out.tsv
// Every 10 s: `tick query` over the repo's rcon binary. Prints one TSV row per sample and a summary:
// the worst P99 and the max of the sampled averages. Vanilla has no max-tick readout, so P99 of the
// last 100 ticks is the closest honest stand-in for "max tick"; both are reported.
import { execFileSync } from "node:child_process";

const [port = "25577", minutes = "10"] = process.argv.slice(2);
const end = Date.now() + Number(minutes) * 60_000;
const rows: { avg: number; p50: number; p95: number; p99: number }[] = [];
const num = (s: string, re: RegExp) => Number(s.match(re)?.[1] ?? Number.NaN);

console.log("ts\tavg_ms\tp50_ms\tp95_ms\tp99_ms\tplayers");
while (Date.now() < end) {
  const out = execFileSync("target/release/rcon", ["tick query", "list"], { env: { ...process.env, RCON_PORT: port }, encoding: "utf8" });
  const r = { avg: num(out, /Average time per tick: ([\d.]+)ms/), p50: num(out, /P50: ([\d.]+)ms/), p95: num(out, /P95: ([\d.]+)ms/), p99: num(out, /P99: ([\d.]+)ms/) };
  const players = num(out, /There are (\d+)/);
  rows.push(r);
  console.log([new Date().toISOString(), r.avg, r.p50, r.p95, r.p99, players].join("\t"));
  await new Promise((res) => setTimeout(res, 10_000));
}
const max = (k: keyof (typeof rows)[number]) => Math.max(...rows.map((r) => r[k]).filter(Number.isFinite));
console.log(`# samples=${rows.length} max_avg=${max("avg")}ms max_p95=${max("p95")}ms max_p99=${max("p99")}ms`);
