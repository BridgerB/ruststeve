// Paired landing comparison (cycle 6, decision 4): two builds run the same ordered landing set in identical
// fresh worlds; compare them landing by landing.
//
//   node scripts/ml/compare.ts --paired <gym.db> <slug> <buildA> <buildB> [--exclude A11,A05]
//
// Per landing: any pass, clean pass (0 deaths), deaths, time to the first lava-site decision (lava_s) and
// time to portal (duration of a pass). For each metric: B − A per landing, the paired mean with a 95%
// bootstrap interval (10,000 resamples of landings), and for pass metrics the two-sided sign test on the
// discordant landings. Landings missing from either arm, aborted, or listed in --exclude are dropped and named.
import { DatabaseSync } from "node:sqlite";

type Trial = { landing: string; pass: number; outcome: string; duration_ms: number; message: string; lava_s: number | null };

export const paired = (argv: string[]) => {
  const exIdx = argv.indexOf("--exclude");
  const exclude = new Set(exIdx >= 0 ? (argv[exIdx + 1] ?? "").split(",").filter(Boolean) : []);
  const [db, slug, buildA, buildB] = argv.filter((_, i) => exIdx < 0 || (i !== exIdx && i !== exIdx + 1));
  if (!db || !slug || !buildA || !buildB) {
    console.error("usage: node scripts/ml/compare.ts --paired <gym.db> <slug> <buildA> <buildB> [--exclude A11]");
    process.exit(2);
  }
  const conn = new DatabaseSync(db, { readOnly: true });
  const load = (b: string) =>
    new Map(
      (conn.prepare("SELECT landing, pass, outcome, duration_ms, message, lava_s FROM gym_runs WHERE slug = ? AND build = ? AND landing IS NOT NULL ORDER BY id").all(slug, b) as Trial[])
        .filter((t) => t.outcome !== "aborted" && t.outcome !== "running")
        .map((t) => [t.landing, t]),
    );
  const A = load(buildA);
  const B = load(buildB);
  const deaths = (t: Trial) => Number(t.message.match(/deaths=(\d+)/)?.[1] ?? 0);
  const landings = [...new Set([...A.keys(), ...B.keys()])].sort();
  const dropped = landings.filter((l) => !A.has(l) || !B.has(l) || exclude.has(l));
  const keep = landings.filter((l) => !dropped.includes(l));

  console.log(`paired ${slug}: A=${buildA} B=${buildB}, ${keep.length} landings${dropped.length ? ` (dropped: ${dropped.map((l) => `${l}${exclude.has(l) ? " excluded" : " unpaired"}`).join(", ")})` : ""}`);
  console.log("landing  A:pass deaths lava_s  dur_s | B:pass deaths lava_s  dur_s");
  for (const l of keep) {
    const f = (t: Trial) => `${t.pass ? "PASS" : "fail"}  ${String(deaths(t)).padStart(5)} ${String(t.lava_s === null ? "-" : Math.round(t.lava_s)).padStart(6)} ${String(Math.round(t.duration_ms / 1000)).padStart(6)}`;
    console.log(`${l.padEnd(8)} ${f(A.get(l)!)} | ${f(B.get(l)!)}`);
  }

  const boot = (d: number[]) => {
    if (d.length === 0) return "n/a";
    const mean = (xs: number[]) => xs.reduce((s, x) => s + x, 0) / xs.length;
    const means: number[] = [];
    for (let i = 0; i < 10000; i++) means.push(mean(d.map(() => d[Math.floor(Math.random() * d.length)])));
    means.sort((a, b) => a - b);
    return `${mean(d).toFixed(2)} [${means[249].toFixed(2)}, ${means[9749].toFixed(2)}] (n=${d.length})`;
  };
  // Two-sided exact sign test on the discordant landings.
  const sign = (bWins: number, aWins: number) => {
    const n = bWins + aWins;
    if (n === 0) return "no discordant landings";
    const choose = (n: number, k: number) => { let c = 1; for (let i = 1; i <= k; i++) c = (c * (n - k + i)) / i; return c; };
    const k = Math.min(bWins, aWins);
    let p = 0;
    for (let i = 0; i <= k; i++) p += choose(n, i) / 2 ** n;
    return `B-only ${bWins}, A-only ${aWins}, sign test p = ${Math.min(1, 2 * p).toFixed(3)}`;
  };
  const metric = (name: string, v: (t: Trial) => number | null) => {
    const d: number[] = [];
    for (const l of keep) {
      const a = v(A.get(l)!), b = v(B.get(l)!);
      if (a !== null && b !== null) d.push(b - a);
    }
    console.log(`  ${name.padEnd(22)} B−A mean ${boot(d)}`);
  };
  console.log("paired differences (B − A), 95% bootstrap over landings:");
  metric("any pass", (t) => t.pass);
  metric("clean pass (0 deaths)", (t) => (t.pass && deaths(t) === 0 ? 1 : 0));
  metric("deaths", (t) => deaths(t));
  metric("time to lava seen (s)", (t) => t.lava_s);
  metric("time to portal (s)", (t) => (t.pass ? t.duration_ms / 1000 : null));
  const sg = (v: (t: Trial) => number) => {
    let bw = 0, aw = 0;
    for (const l of keep) { const a = v(A.get(l)!), b = v(B.get(l)!); if (b > a) bw++; if (a > b) aw++; }
    return sign(bw, aw);
  };
  console.log(`  any pass:   ${sg((t) => t.pass)}`);
  console.log(`  clean pass: ${sg((t) => (t.pass && deaths(t) === 0 ? 1 : 0))}`);
};
