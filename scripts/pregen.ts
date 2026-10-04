// Pre-generate an area of a dimension before bots go there (cycle 4, 2026-10-03).
// Server B crashed twice on the fresh world (60 s tick, server thread in ServerChunkCache.getChunk):
// 7 bots teleported into never-generated terrain at once, each with its full view distance to
// generate on a 2-CPU box. This forceloads the area in small batches, waits until each batch is
// loaded (async, no players), then releases it, so later teleports find generated chunks.
//
//   node scripts/pregen.ts <x0> <z0> <x1> <z1> [dimension=minecraft:overworld] [batchChunks=16]
// Runs on the box (uses target/release/rcon → Server B, RCON 25576).
import { execFileSync } from "node:child_process";

const [x0, z0, x1, z1] = process.argv.slice(2, 6).map(Number);
const dim = process.argv[6] ?? "minecraft:overworld";
const batch = Number(process.argv[7] ?? 16);
if ([x0, z0, x1, z1].some((v) => Number.isNaN(v))) {
  console.error("usage: node scripts/pregen.ts <x0> <z0> <x1> <z1> [dimension] [batchChunks]");
  process.exit(2);
}
const rcon = (...cmds: string[]) =>
  execFileSync("target/release/rcon", cmds, { encoding: "utf8", env: { ...process.env, RCON_PORT: "25576" } });
const inDim = (cmd: string) => (dim === "minecraft:overworld" ? cmd : `execute in ${dim} run ${cmd}`);

const cx0 = Math.floor(Math.min(x0, x1) / 16), cx1 = Math.floor(Math.max(x0, x1) / 16);
const cz0 = Math.floor(Math.min(z0, z1) / 16), cz1 = Math.floor(Math.max(z0, z1) / 16);
const chunks: [number, number][] = [];
for (let cx = cx0; cx <= cx1; cx++) for (let cz = cz0; cz <= cz1; cz++) chunks.push([cx, cz]);
console.log(`pregen ${dim}: ${chunks.length} chunks in batches of ${batch}`);

const t0 = Date.now();
for (let i = 0; i < chunks.length; i += batch) {
  const b = chunks.slice(i, i + batch);
  rcon(...b.map(([cx, cz]) => inDim(`forceload add ${cx * 16} ${cz * 16}`)));
  // Wait for every chunk of the batch to report loaded (async generation), ≤ 120 s.
  const tb = Date.now();
  for (;;) {
    const out = rcon(...b.map(([cx, cz]) => inDim(`execute if loaded ${cx * 16 + 8} 0 ${cz * 16 + 8}`)));
    const passed = (out.match(/passed/g) ?? []).length;
    if (passed >= b.length) break;
    if (Date.now() - tb > 120000) {
      console.log(`  batch ${i / batch}: only ${passed}/${b.length} loaded after 120 s — moving on`);
      break;
    }
    execFileSync("sleep", ["1"]);
  }
  rcon(...b.map(([cx, cz]) => inDim(`forceload remove ${cx * 16} ${cz * 16}`)));
  if ((i / batch) % 10 === 0) console.log(`  ${Math.min(i + batch, chunks.length)}/${chunks.length} chunks, ${((Date.now() - t0) / 1000).toFixed(0)} s`);
}
console.log(`done: ${chunks.length} chunks in ${((Date.now() - t0) / 1000).toFixed(0)} s`);
console.log(rcon("tick query").split("\n").slice(0, 2).join(" "));
