# Eyes: measure and observe (issue 19a and 19b, agent Q1)

The orchestrator's index is [`eyes.md`](eyes.md). This file holds what Q1 measured (19a.1 to 19a.3; 19a.3's locate and 19a.4's jar rules were measured by Q5, probe 45685968956, see [`eyes-model.md`](eyes-model.md)) and what it built so the eyes and the stronghold can be seen before they are built (19b). One world throughout: seed `typecraft`, the race server's.

## 19a.1: the four `pearls_12` snapshots (probe 45686594347, `results player-dat`)

Race 45621953332, all four in the main warped forest. Common to all: 7 blaze rods, 12 ender pearls, a bucket, 2 water buckets, flint and steel (63 uses left), a furnace, 1 stick, 3 warped planks, a fresh stone pickaxe (131) and 2 fresh stone swords, iron helmet, chestplate and leggings, 7 raw iron, 50 soul soil, 8 gravel, 4 leather, 6 gold nuggets, 2 quartz. **No food item, no crafting table, no cobblestone.** Saturation 0 in every one.

| lane | where, server tick | health | food (exhaustion) | iron sword left | shield left | wooden pickaxes (uses left) | pickaxe uses in all | netherrack | stems, wart blocks |
|---|---|---|---|---|---|---|---|---|---|
| 1 (the old frontier) | (494.7, 44.0, 19.7), 11,360 | 10.0 | 10 (0.33) | 37 | 30 | 25, 59, 59, 59 | 333 | 76 | 7, 4 |
| 5 | (506.1, 45.0, 38.5), 25,500 | 19.8 | 10 (0.50) | 44 | 30 | 8, 59, 59 | 257 | 69 | 10, 16 |
| 6 | (518.9, 46.0, 13.5), 47,300 | 19.8 | 10 (2.89) | 24 | 30 | 30, 59 | 220 | 33 | 10, 9 |
| 7 | (496.5, 44.0, 17.9), 21,620 | 19.8 | 11 (3.56) | 47 | 18 | 25, 21, 59, 59 | 295 | 67 | 6, 12 |

Armour wear: 18 to 20 of 165/240/225 in every lane.

**Recommended and taken (orchestrator, 91b5146): lane 7.** 19.8 health, one more food point, the most sword uses (47) and the most pickaxe uses of the three healthy bots (295 against lane 5's 257 and lane 6's 220), 67 netherrack. Its weakness is the shield (18 uses left against 30).

## 19a.2: what the bot remembers (probe 45686594347, `results memory`)

The memory is the same in what matters in all four snapshots: about 165,000 sightings, 148,000 of them nether bricks.

- **The Nether portal is in memory under `minecraft:the_nether`, lit as last seen**: `nether_portal` at x 22 to 23, y 34 to 36, z -20 (6 cells), and 30 obsidian. Memory saw 385 Nether chunk columns, chunk x -3 to 43, z -15 to 22: the whole way from the portal to the forest.
- **The overworld portal is in memory under `minecraft:overworld`**: `nether_portal` at x 125 to 126, y -52 to -50, z -108 (6 cells, seen at server tick 24,760). Not (113, -44, -105) as the plan had it. 194 obsidian; 332 overworld columns, chunk x -10 to 19, z -20 to 7.
- No other `nether_portal` sighting in either dimension.
- **The old stairs cannot be in memory**: memory watches logs, tables, furnaces, nether bricks, spawners, obsidian and portals (and since 19b the stronghold's blocks); stone and air are never recorded. The stairs are read from the world's region files below.

So the way home is memory: a far lookup over memory's portal sightings, not geography.

## 19a.3: the land to the stronghold and the portals today (probes 45686839641, 45688112814, region files)

The stronghold (Q5's probe, confirmed here): `locate structure minecraft:stronghold` gives (1648, ~, -16) from the console (spawn, 1,648 off), from the portal (1,537) and from the old stairs (1,597).

`locate biome` from the portal (113, 64, -105): ocean (-111, -41) 232 west, lukewarm ocean (-335, -233) 465, river (49, -265) 172, deep ocean 1,605 off, cold ocean 2,375; nothing on the bearing east within those.

(The strip and the portals' state follow from probe 45688112814.)

## 19b: what was built

- **Stages** (`crates/world/src/stage.rs`, e96708e), between `pearls_12` and `end` (`Stage::ALL` is 20):
  - `overworld`: the overworld dimension, `ender_eye` + `ender_pearl` >= 12 (`EYES_FULL`).
  - `stronghold`: the overworld, an `end_portal_frame` on record within 128 horizontally (`FRAME_NEAR`, the view distance; `frames_near`).
  - `portal_room`: the feet in an air cell within 8 (`ROOM_HALF`) of the frame ring's centre along each axis, at the frame y - 2 to + 2 (`in_portal_room`; the ring's centre is the middle of the box of the frames at the nearest frame's y within 16 of it, `frame_ring`). The box is Q5's from the jar's portal room piece (11 by 16, frames at piece y 3, floor y 0).
  - `stronghold` and `portal_room` scan memory, so they are looked at once a second with `fortress` and `spawner`.
- **Memory** (`crates/memory`): watches `end_portal_frame`, `end_portal`, stone bricks and their mossy, cracked and chiseled kinds, and the seven infested kinds (`memory::INFESTED`); frames, the End portal, stone bricks and the infested kinds are in the kind index.
- **`add_entity`'s velocity** (7c9069f): `S2c::AddEntity { velocity }` (an LpVec3), kept in the world as `Entity::spawn_velocity` (apart from `velocity`, which the fireball readers read as sent since).
- **Facts** (1a56f31, accreted): `EyeThrown {id, from, yaw}` (an eye appearing within 4 of the bot; its spawn velocity's heading), `EyeFlight {id, first, last, heading, samples, ticks}` (told when the eye is removed), `EyeEnded {id, dropped, at}` (dropped when an `ender_eye` item appears within 2 of where it ended within 10 ticks, else shattered), `EyePicked {id, amount}`, `FrameSeen {cell}`, `SilverfishWoken {id, d}` (within 16); `Dimension` gains `from`.
- **Progress**: an eye thrown or picked up, a frame seen or a dimension entered is `Gain::Eyes`; `config/race.toml [progress] items` gains `["ender_eye", 12]`.
- **Senses row**: ` overworld eyes 12 frames 9` (the dimension outside the Nether, eyes held, frames on record within 128).
- **Tooling** (15003db, d0cd4db): `results memory <memory.rsw> [kind]...`, `results region-find <region dir> <block|*>... [--box ...]`, `results region-strip <region dir> x0,z0 x1,z1 step`.
