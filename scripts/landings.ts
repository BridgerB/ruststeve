// Paired landing sets (cycle 6, decision 4): 12 pre-generated, pre-checked surface landings that every
// gym arm runs in the same order.
//   RCON_PORT=25577 node scripts/landings.ts <set> <x0> <z0> [count=12] [spacing=400] > data/landings-<set>.json
// Candidates lie on a grid from (x0, z0), `spacing` apart (so trials never share terrain). A landing is kept when:
//   - its column is land: the block under the motion_blocking_no_leaves surface is not water or lava;
//   - no water at the surface of any column within 8 blocks (17 × 17 columns);
//   - no ruined portal (the source of natural surface obsidian) within 200 blocks (`locate structure`);
// and its surface y is recorded. Chunks are forceloaded only while checked, then released.
import { execFileSync } from "node:child_process";

const [set = "A", x0s = "60000", z0s = "3000", counts = "12", spacings = "400"] = process.argv.slice(2);
const [x0, z0, count, spacing] = [Number(x0s), Number(z0s), Number(counts), Number(spacings)];
const rcon = (...cmds: string[]) => execFileSync("target/release/rcon", cmds, { encoding: "utf8", maxBuffer: 1 << 24 }).trimEnd().split("\n");
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const surfaceY = (x: number, z: number): number | null => {
  const tag = `lm${x}_${z}`.replace(/-/g, "m");
  const out = rcon(
    `execute positioned ${x} 0 ${z} positioned over motion_blocking_no_leaves run summon minecraft:marker ~ ~ ~ {Tags:["${tag}"]}`,
    `data get entity @e[type=minecraft:marker,tag=${tag},limit=1] Pos[1]`,
    `kill @e[type=minecraft:marker,tag=${tag}]`,
  ).join(" ");
  const m = out.match(/has the following entity data: (-?[\d.]+)d/);
  return m ? Math.floor(Number(m[1])) : null;
};

const landings: { id: string; x: number; y: number; z: number; checks: string }[] = [];
const side = Math.ceil(Math.sqrt(count * 3));
for (let i = 0; i < side * side && landings.length < count; i++) {
  const x = x0 + (i % side) * spacing;
  const z = z0 + Math.floor(i / side) * spacing;
  rcon(`forceload add ${x - 8} ${z - 8} ${x + 8} ${z + 8}`);
  let loaded = false;
  for (let t = 0; t < 60 && !loaded; t++) {
    loaded = rcon(`execute if loaded ${x - 8} 0 ${z - 8}`, `execute if loaded ${x + 8} 0 ${z + 8}`).every((l) => l.includes("passed"));
    if (!loaded) await sleep(1000);
  }
  const reject = (why: string) => {
    console.error(`(${x},${z}) rejected: ${why}`);
    rcon(`forceload remove ${x - 8} ${z - 8} ${x + 8} ${z + 8}`);
  };
  if (!loaded) {
    reject("chunks did not load in 60 s");
    continue;
  }
  const under = rcon(
    `execute positioned ${x} 0 ${z} positioned over motion_blocking_no_leaves if block ~ ~-1 ~ minecraft:water`,
    `execute positioned ${x} 0 ${z} positioned over motion_blocking_no_leaves if block ~ ~-1 ~ minecraft:lava`,
  );
  if (under.some((l) => l.includes("passed"))) {
    reject("liquid underfoot");
    continue;
  }
  const cols: string[] = [];
  for (let dx = -8; dx <= 8; dx++) for (let dz = -8; dz <= 8; dz++) cols.push(`execute positioned ${x + dx} 0 ${z + dz} positioned over motion_blocking_no_leaves if block ~ ~-1 ~ minecraft:water`);
  const wet = rcon(...cols).filter((l) => l.includes("passed")).length;
  if (wet > 0) {
    reject(`${wet} water columns within 8`);
    continue;
  }
  const loc = rcon(`execute positioned ${x} 64 ${z} run locate structure minecraft:ruined_portal`).join(" ");
  const away = Number(loc.match(/\((\d+) blocks away\)/)?.[1] ?? Number.POSITIVE_INFINITY);
  if (away <= 200) {
    reject(`ruined portal ${away} blocks away`);
    continue;
  }
  const y = surfaceY(x, z);
  rcon(`forceload remove ${x - 8} ${z - 8} ${x + 8} ${z + 8}`);
  if (y === null) {
    console.error(`(${x},${z}) rejected: no surface y`);
    continue;
  }
  const id = `${set}${String(landings.length + 1).padStart(2, "0")}`;
  landings.push({ id, x, y, z, checks: `land; 0 water columns within 8; nearest ruined portal ${Number.isFinite(away) ? away : ">search"} blocks` });
  console.error(`${id} (${x},${y},${z}) ok, ruined portal ${away}`);
}
if (landings.length < count) {
  console.error(`only ${landings.length}/${count} landings passed`);
  process.exit(1);
}
console.log(JSON.stringify(landings, null, 1));
