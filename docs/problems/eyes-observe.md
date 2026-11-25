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

### The Nether portal today (probe 45688112814, the `pearls_12` world's region files)

**Lit.** Portal cells x 22 to 23, y 34 to 36, z -20; obsidian frame x 21 to 24, y 33 to 37, z -20. The plane runs along x, so the faces are z -19 (south) and z -21 (north).

- **South face (z -19)**, feet level y 34: (21, 34, -19) lava, **(22, 34, -19) cobblestone** (the bot's own), (23, 34, -19) lava, (24 and 25, 34, -19) lava. At y 35: (21, 35, -19) lava, **(22 and 23, 35, -19) air**, (24 and 25, 35, -19) lava. Lava falls from above at x 20 to 21 and 24 to 26 (lava at (20, 36 to 40, -19), (24 to 26, 35 to 40, -19)). The row behind, z -18, is solid netherrack at x 19 to 26, y 33 to 35.
- **The one dry standing cell** is on the cobblestone: feet (22, 35, -19), the portal cell (22, 35, -20) one step north. Lava touches it on the west (21, 35, -19) and at (23, 34, -19) and (24, 35, -19). A tunnel at y 35 to 36 through the netherrack of z -18 reaches it without walking the open ledge.
- **North face (z -21)**: x 22 to 24 at y 34 to 35 is air with no floor (y 32 to 33 air under x 23 to 24) straight over the lava sea (lava at y 30 to 31); lava at (19 to 20, 34, -21), (20, 34, -22), (25 to 26, 34, -21 to -23). Never approached from the north.

### The overworld portal and the old stairs (same probe)

- **Lit**: cells x 125 to 126, y -52 to -50, z -108; obsidian frame x 124 to 127, y -53 to -49, z -108 (faces z -107 and z -109); plains above. One more obsidian at (111, -45, -107).
- **The old stairs**: dug air (not `cave_air`), one cell wide and three high, at x 56 from (56, 53, -109) up to (56, 60, -116), rising a block a step northward. Below y 53 they open into the cave band: thousands of air cells from y -57 to 33 across x 40 to 135, z -150 to -90.

- **The overworld portal's faces** (probe 45689842509, the lane-7 world): the north face (z -109) is solid deepslate; **the south face (z -107) is the way in and out.**
- **The stairs reach the surface**: the same one-wide, three-high dug air at x 56 continues from (56, 53, -109) to (56, 69, -126), one up per block north; open air over the land from y 70. The old stairs are a dry way from y 53 to the surface; between the portal (y -52) and y 53 is the cave band.

### The land from the portal to the stronghold (probe 45689842509, a fresh world of the seed, one chunk forceloaded every 32 blocks)

The line from the portal (113, -105) to the stronghold point (1648, -16), the top block, its y and biome every 32 blocks:

| along | where | what |
|---|---|---|
| 0 to 320 | (113, -105) to (432, -86) | plains, y 65 to 81 |
| 352 to 416 | (464, -85) to (528, -81) | **river** biome (the column tops are grass and leaves at y 62 to 70: the line meets its banks, the water at y 62) |
| 448 to 736 | (560, -79) to (848, -62) | plains then forest, y 62 to 85 |
| 768 to 960 | (880, -61) to (1071, -49) | old growth birch forest; **water at the top at (976, -55) and (1039, -51)**, y 62 (ponds or a stream) |
| 992 to 1216 | (1103, -48) to (1327, -35) | forest; water at (1199, -42), y 62 |
| 1248 to 1440 | (1359, -33) to (1551, -22) | plains, y 63 to 80 |
| 1472 to 1504 | (1583, -20) to (1614, -18) | **river**: water at the top, y 62, two points 32 apart |
| 1536 | (1646, -16) | plains, y 65 |

**No ocean on the line** (the nearest ocean is 232 west of the portal, the nearest deep ocean 1,605 off). Two rivers: one about 450 to 530 along, crossed or skirted (the line's points there are bank land), and one about 30 to 60 wide just before the stronghold at x 1583 to 1614. Land height 62 to 85 all the way: no mountains.

### The stronghold's portal room (same probe)

- **The frame ring: x 1626 to 1630, z -36 to -32, y 19** (12 frames). Its centre (1628.5, -33.5): 21 blocks from the `locate` point (1648, -16) and 46 below the surface (y 65).
- **The room** (stone bricks): walls at z -39 and z -29 and at x 1623 and x 1638, so 9 inside across z, 14 along x; **floor y 16, feet y 17**; iron bars in the long walls at y 19 to 20.
- **The door is in the east wall: (1638, 17 to 18, -34)**, iron bars beside it at z -35 and -33; a corridor runs east from it.
- **The raised platform**: stone bricks at y 17 over x 1626 to 1634, z -36 to -32; **the lava pool under the ring at y 17, x 1627 to 1629, z -35 to -33** (the ring's inside, open above at y 18). Steps up at x 1631 to 1633, y 18 to the **spawner at (1632, 19, -34)**, on the door side of the ring.
- **Lava at the floor's level at the door's end**: (1635 to 1637, 17, -38) and (1635 to 1640, 17, -30), (1636 to 1638, 17, -29 to -28). A bot entering by the door walks between them (z -37 to -31 at x 1635 to 1637 is air).
- **Bricks within 112 of the point**: 9,467 stone bricks, 5,592 mossy, 3,705 cracked, **918 infested stone bricks** (no other infested kind); the stronghold's bricks lie at y 2 to 30, most at y 14 to 24.
- **Other spawners**: 14 more within 112 of the point, all deep (y -46 to -11) but one at (1741, 36, -25): dungeons and mineshafts, not the stronghold's. The room's is the only one at the frames' y.
- **The `portal_room` box follows**: within 4.5 of the ring's centre on both axes (the inside on the short axis, whichever way a room faces), feet y 17 to 21 (frame y - 2 to + 2), an air cell. Q5's first box (8) reached through the west wall into a corridor of air at x 1621 to 1622, y 19 to 20.

## 19b: what was built

- **Stages** (`crates/world/src/stage.rs`, e96708e), between `pearls_12` and `end` (`Stage::ALL` is 20):
  - `overworld`: the overworld dimension, `ender_eye` + `ender_pearl` >= 12 (`EYES_FULL`).
  - `stronghold`: the overworld, an `end_portal_frame` on record within 128 horizontally (`FRAME_NEAR`, the view distance; `frames_near`).
  - `portal_room`: the feet in an air cell within 4.5 (`ROOM_HALF`) of the frame ring's centre along each axis, at the frame y - 2 to + 2 (`in_portal_room`; the ring's centre is the middle of the box of the frames at the nearest frame's y within 16 of it, `frame_ring`). From the jar's piece (Q5: 11 by 16, frames at piece y 3, floor y 0) and the race world's room above; first 8, narrowed when the room was read.
  - `stronghold` and `portal_room` scan memory, so they are looked at once a second with `fortress` and `spawner`.
- **Memory** (`crates/memory`): watches `end_portal_frame`, `end_portal`, stone bricks and their mossy, cracked and chiseled kinds, and the seven infested kinds (`memory::INFESTED`); frames, the End portal, stone bricks and the infested kinds are in the kind index.
- **`add_entity`'s velocity** (7c9069f): `S2c::AddEntity { velocity }` (an LpVec3), kept in the world as `Entity::spawn_velocity` (apart from `velocity`, which the fireball readers read as sent since).
- **Facts** (1a56f31, accreted): `EyeThrown {id, from, yaw}` (an eye appearing within 4 of the bot; its spawn velocity's heading), `EyeFlight {id, first, last, heading, samples, ticks}` (told when the eye is removed), `EyeEnded {id, dropped, at}` (dropped when an `ender_eye` item appears within 2 of where it ended within 10 ticks, else shattered), `EyePicked {id, amount}`, `FrameSeen {cell}`, `SilverfishWoken {id, d}` (within 16); `Dimension` gains `from`.
- **Progress**: an eye thrown or picked up, a frame seen or a dimension entered is `Gain::Eyes`; `config/race.toml [progress] items` gains `["ender_eye", 12]`.
- **Senses row**: ` overworld eyes 12 frames 9` (the dimension outside the Nether, eyes held, frames on record within 128).
- **Tooling** (15003db, d0cd4db): `results memory <memory.rsw> [kind]...`, `results region-find <region dir> <block|*>... [--box ...]`, `results region-strip <region dir> x0,z0 x1,z1 step`.
