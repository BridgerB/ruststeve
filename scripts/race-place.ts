// Race placement on land (cycle 5, Phase 1 (i)): the gym's RandomSurface rules, by RCON only, because
// the harness places bots before they move. Vanilla never spawns a player in water; race i5 and i6 each
// started two of five bots on the ocean (~9.2 bot-hours of gather_wood per race, 0 progress).
//
//   node scripts/race-place.ts <x> <z> [rcon binary]   → prints "x z" of the chosen surface column
//
// Per candidate (a spiral around the lane start, staying near the lane):
//   1. forceload the column and wait until `execute if loaded` passes. An unloaded column has no
//      heightmap, and its empty test replies read as "not water": that is how the old harness placed
//      bots on the sea.
//   2. reject water or lava underfoot (the block under the motion_blocking_no_leaves surface);
//   3. reject a shore: any water in the 7×7 ring at the floor and feet levels (the gym's rule).
// The first candidate passing all three wins. If none does, the first one dry underfoot is used
// (a pond nearby beats the ocean). Rejected candidates are un-forceloaded; the chosen column stays
// forced (the caller records it for cleanup).
import { execFileSync } from "node:child_process";

const [xs, zs, rconBin = "target/release/rcon"] = process.argv.slice(2);
const [x0, z0] = [Number(xs), Number(zs)];
if (!Number.isFinite(x0) || !Number.isFinite(z0)) {
  console.error("usage: node scripts/race-place.ts <x> <z> [rcon binary]");
  process.exit(2);
}
const rcon = (...cmds: string[]): string => {
  try {
    return execFileSync(rconBin, cmds, { encoding: "utf8", timeout: 60_000 });
  } catch {
    return "";
  }
};
const passed = (out: string) => (out.match(/Test passed/g) ?? []).length;
const at = (x: number, z: number, test: string) => `execute positioned ${x} 0 ${z} positioned over motion_blocking_no_leaves if block ${test}`;
const sleep = (ms: number) => Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);

// Near the lane first (lanes are 90 apart in z), then along x only, so a lane stays a lane.
const OFFSETS = [[0, 0], [48, 0], [0, 32], [-48, 0], [0, -32], [96, 0], [-96, 0], [160, 0], [-160, 0], [320, 0], [-320, 0], [480, 0], [-480, 0]];
let fallback: [number, number] | null = null;
for (const [ox, oz] of OFFSETS) {
  const [x, z] = [x0 + ox, z0 + oz];
  rcon(`forceload add ${x} ${z}`);
  let loaded = false;
  for (let i = 0; i < 30 && !loaded; i++) {
    loaded = passed(rcon(`execute if loaded ${x} 0 ${z}`)) > 0;
    if (!loaded) sleep(1000);
  }
  if (!loaded) {
    console.error(`[race-place] (${x},${z}) never loaded — skipped`);
    rcon(`forceload remove ${x} ${z}`);
    continue;
  }
  const wet = passed(rcon(at(x, z, "~ ~-1 ~ minecraft:water"), at(x, z, "~ ~-1 ~ minecraft:lava"))) > 0;
  const ring: string[] = [];
  for (let dx = -3; dx <= 3; dx++) for (let dz = -3; dz <= 3; dz++) for (const dy of [-1, 0]) ring.push(at(x, z, `~${dx} ~${dy} ~${dz} minecraft:water`));
  const shore = passed(rcon(...ring)) > 0;
  if (!wet && !shore) {
    console.error(`[race-place] (${x},${z}) land, no water within 3`);
    console.log(`${x} ${z}`);
    process.exit(0);
  }
  console.error(`[race-place] (${x},${z}) rejected: ${wet ? "water/lava underfoot" : "shore (water within 3)"}`);
  if (!wet && !fallback) {
    fallback = [x, z];
  } else {
    rcon(`forceload remove ${x} ${z}`);
  }
}
if (fallback) {
  console.error(`[race-place] no shore-free land — using the first dry column (${fallback[0]},${fallback[1]})`);
  console.log(`${fallback[0]} ${fallback[1]}`);
  process.exit(0);
}
console.error(`[race-place] water everywhere around (${x0},${z0}) — no placement`);
process.exit(1);
