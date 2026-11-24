# Issue 17: from 7 blaze rods to a ticking enderman (one bot first)

**Status: open.** The index for issue 17 (#22). Each part has its own wall file: [`endermen-observe.md`](endermen-observe.md) (the stage, ticking, biomes, notes), [`heading.md`](heading.md) (the long heading), [`endermen-leave.md`](endermen-leave.md) (after the rods, near endermen, the stare), [`endermen-model.md`](endermen-model.md) (the sim).

## Needs
From the frontier `rods_7` snapshot, one real bot comes within 32 blocks of an enderman that ticks (its chunk within the server's simulation distance of the player's chunk), still holding its 7 rods; the race snapshots it as stage `endermen`, the snapshot is published and becomes the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`.

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: endermen`, and the race's release carries its `snapshot-endermen-…` asset.

## Risks
- Pickaxe uses: 144 in all (one pickaxe at 13 uses, one fresh), no table and no wood to make more; a reserve of 40 is kept for issue 18's burrow.
- Food: one raw porkchop; a long trip does not heal.
- Ghasts and skeletons in a soul sand valley; ghasts and magma cubes in basalt deltas; hoglins and piglins in crimson forests (no gold armour, so piglins are hostile).
- A stare: an enderman is provoked by a look at its eye point within a narrow cone (below), and then hits for 7 and keeps its anger.
- One world: every frontier race replays the same world, so a choice tuned here is tuned to one place.

## What was measured before any code (17a, 2026-11-24)

Probes: 45472883583 (the snapshots' player files, the biome files, `locate biome` and a strip map), 45472888528 (an enderman census over 617 runs), 45472254333 (javap of `EnderMan`, its goals, `NaturalSpawner`, `DistanceManager`, `ChunkMap`, `ServerLevel`). Tooling: `results player-dat`, `results endermen` (c5791eb).

**The starting bots** (stone pickaxe 131 uses; iron helmet 165, chestplate 240, leggings 225):

| Snapshot | Health | Food / saturation | Burning | Pickaxe uses | Shield | Food held |
|---|---|---|---|---|---|---|
| race-45358767468 lane 0 (the frontier until 17's first commit) | 17.1 | 17 / 0 | yes, 97 ticks | 13 + 131 | none | 1 raw porkchop |
| race-45363542882 lane 1 | 19.0 | 19 / 0 | no | 13 + 131 | none | none |
| race-45363542882 lane 2 | 16.2 | 20 / 0.8 | no | 13 + 131 | 48 of 336 left (about 8 fireballs) | 1 raw porkchop |

**How the three left the spawner** (their own ledgers): each walked about 45 blocks south by the route to about (129, 61, 205) on nether bricks, then ended `rods met`. None died; no enderman was seen after the rods. The frontier bot was burning at rod 7, took a fireball during the leave and ended at 9.2 health.

**This world's biomes** (development knowledge only: no bot code reads it; `locate biome` at y 64):
- From the spawner (129, 64, 159), which stands in nether wastes: a soul sand valley 32 blocks west (and from 32 to 176 north); basalt deltas 45 blocks north-east; a crimson forest 143 south-south-west; the nearest **warped forest 454 blocks east-north-east** at (481, 64, -129), a strip only 48 blocks wide on the +x axis, behind about 300 blocks of basalt deltas.
- Strip map to 608 blocks: only +x crosses a warped forest; south and north cross crimson forests; south-east is nether wastes all the way.

**The game's rules** (26.1.2 jar):
- Ticking: an entity ticks when its chunk is within Chebyshev chunk distance 3 (the simulation distance) of the player's; outside, it is frozen and sends nothing. At distance 3 an enderman within 32 blocks always ticks.
- Spawning: at least 24 blocks from the player, only in ticking chunks (7 × 7 = 49). The monster cap is 70 for a lone player (the cap's chunk count is a fixed 17 × 17, not the simulation distance), so the cap does not bind; spawn attempts in 49 chunks and the biomes' energy budgets do.
- Monster spawns by biome: warped forest, endermen only (groups of 4, budget 0.12); soul sand valley, skeleton 20 (groups of 5), ghast 50, enderman 1 (groups of 4): 1 in 71; nether wastes, enderman 1 in 168; crimson forest and basalt deltas, none.
- Despawn: beyond 128 blocks at once; past 32, 1 in 800 a tick once idle 600 ticks; frozen mobs never despawn.
- The stare: only the enderman's eye point (feet + 2.55) counts; provoked when the look's dot with the direction to the eye exceeds 1 − 0.025/distance (about 6.4° at 4 blocks, 3.2° at 16, 1.6° at 64), with line of sight past every block collision shape, within its 64-block follow range, and no carved pumpkin on the head. Anger comes 4 to 6 server ticks after a held stare (metadata 18 goes true at once); then within 4 it teleports away, past 16 it teleports toward the player after 30 ticks; anger lasts; a hit is 7.

**Our own recorded evidence** (617 runs, one senses row a second):

| Where | Seconds | An enderman within 32 | Runs ever within 32 |
|---|---|---|---|
| Nether (portal, fortress and spawner only) | 68,888 | 268 s (0.39%) | 10 |
| Overworld cave (y < 50) | 931,177 | 16,287 s (1.75%) | 81 |
| Overworld surface | 137,189 | 205 s (0.15%) | 7 |

No row carried the day clock yet, so there is no night split. One frontier-world bot (race 45358767468 lane 1) had an enderman at 31 blocks while still at the spawner, before its seventh rod.

## The decision (2026-11-24)

**The nearest place that spawns endermen first, the warped forest only if seen.** The warped forest is 454 blocks off behind 300 blocks of basalt deltas, with 144 pickaxe uses in hand; a soul sand valley, which spawns endermen (1 in 71, in fours), lies 32 blocks from the spawner, and its soul sand and soul soil are blocks the bot sees exposed (it already carries 50 soul soil dug on its way in). So:
1. After the leave walk and recovery, a soul sand valley known from seen soul sand or soul soil, or the feet biome `soul_sand_valley`, is the first target: go there and wait in a covered spot (skeletons and ghasts spawn there too) for endermen to spawn and come within 32.
2. A warped forest known from seen blocks (nylium, stems, wart) beats everything.
3. Only with neither known, the held heading (17d), which turns away from basalt and crimson signs.
4. The overworld is not used: its caves score 1.75% of seconds with an enderman within 32, which a soul sand valley 32 blocks away should match without a 210-block trip back and a cave cast.

This uses only what a player could know: the biome at the feet and blocks seen. The geography above sized the choice; it is not in the code.

**The frontier moves sideways** to race-45363542882 lane 2 (the same stage, real, the same world): not burning, food 20 and saturation, and a shield with 48 uses, against the old frontier's burning bot at food 17 with no shield.

## Index of walls
(none yet)
