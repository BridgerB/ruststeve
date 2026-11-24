# The Nether walk, by route (issue 16)

Thirteen armed, fed bots stood in the Nether of the clean snapshot's world and none reached the fortress whose bricks they saw about 110 blocks away (`docs/problems/blaze.md`, walls 22 and 23 and the second "Stopped"). This file is the route's side of that wall: what the ground is, why no route was found over it, and each fix with its evidence. The tunnel's side is another agent's.

## The ground, read before any fix

Read with `navmap` (crate `navprobe`, probes 45334641630 and 45334717081) on `rs-l05-b00.rsr` of race 45194253932 lane 5 at tick 25,700, lane 3 of race 45204423376 at tick 50,000 (the same blocks, 329 columns known) and lane 5 of race 45204423376 at ticks 22,200 to 22,710. All coordinates are blocks; "feet y" is the cell the feet stand in.

- **The lava sea's surface is y 31** (lava at y 31 and under, air from y 32). It is one sea: a cavern north and west of the portal (x -8 to 40, z -50 to -22), joined round the north-east (x 40 to 77, z -45 to -34) to a cavern on the east (x 68 to 100 and beyond, z -30 to 100 and beyond).
- **The portal** (obsidian x 21 to 24, y 34 to 37, z -20) opens on a ledge of netherrack at feet y 34, about nine cells by three (x 22 to 30, z -24 to -21), three blocks over the sea on its north side. South of it is a hill: netherrack with magma veins (most of y 29 under the ledge, and bands all through), gravel pockets and soul soil, solid from the sea to above y 48 over x 0 to 66, z -18 to 97. A search from the portal at tick 24,980 spends 42 expansions and its frontier is empty: the ledge is an island for `path::plan`.
- **Lava comes down onto the ledge after the bot arrives.** A lava stream runs down the hill's north face from above y 44 (x 25 to 33, z -23 to -18 at y 36 to 44) and a fall stands at (26, 35 and up, -23). At tick 22,200 the ledge is dry; at 22,560 lava lies at (26 to 28, 34, -23); at 22,700 it is at y 36 over (29 to 32, -23 to -21) and one tick-row later on the bot. By tick 25,700 the row (21 to 25, 34, -19) in front of the portal is lava. The chunks start ticking when a bot comes through, so every bot of this snapshot meets the same flood 700 to 900 ticks in: the four lava deaths at (30, 31, -18) and (20, 34, -18) are this, not only the bot's own stairs.
- **The bricks in view** are a pillar standing in the sea: (61 to 62, 28 to 34 and up, 95 to 98), widening to x 58 to 66 at z 99 to 104. West of it the hill ends in a sheer face of soul soil and soul sand at x 56 to 57 (z 85 to 97), four cells of lava (x 57 to 60) between the face and the pillar. At z 100 a shelf at feet y 34, (55, 34, 100), is two lava cells from bricks at x 58. `at_fortress` is within 6 of a brick, so the shelf, or the face's foot, is enough. 1,927 bricks are on record; the nearest are these.
- **Soul sand and soul soil** line every shore: the hill's east shore (x 62 to 78, z -50 to 100) is soul sand on slopes that drop 2 to 5 a column into the sea; a valley at z 44 to 72, x 15 to 66 has soul sand floors at feet y 32 to 40 round lava pools; the plateau at z 85 to 97 is soul soil from y 31 up. Soul sand is `Awkward` to `path::classify` (neither floor nor clear), so none of it is ground today; soul soil is a full block and is.
- **Magma** is in the hill and under the ledge (y 29 to 30), and on shores at feet y 32 to 35 in places (x 32 to 39, z -42 to -40). It stays a wall for the route.
- **No ground joins the portal to the bricks at or under y 48.** Between them is the hill (tunnel: 115 blocks of netherrack and soul soil at feet y 33 or 34, coming out at the face five blocks from the pillar) or the sea (round by the north-east and down the east cavern: about 200 cells of lava). Over y 48 was not read in this probe.

## What a route needs here

1. Soul sand as a floor, with the feet's cell right for a body standing at y + 0.875.
2. A start that is allowed beside lava: a bot with lava at its door must still be given a way off.
3. A way over lava: a bridge of held blocks, as a move the search can choose and the walk can make.
4. A search that reaches 110 to 250 blocks: 6,000 expansions do not.

## Fix 1: soul sand is ground, a bridge move, a far search (38f14da, d709221, 475e5f1)

- **Soul sand** is `Solid` to `path::classify`; feet on it rest at y + 0.875, in the soul sand's own cell, and the route's node is the cell over it (first by lifting `feet_cell` itself, which fix 5 took back); `follow` jumps only for a cell up; soul sand is dry land for a trap to clear on. Magma stays a wall.
- **The lava rule was not the cause.** `lava_near` forbids a cell with lava beside it at its own height, or under it. A shore whose floor is level with the sea (floor y 31, feet y 32) has lava beside the floor, not beside the feet, and was always usable. What had no route was the ledge (an island) and soul sand. The rule is kept; only the bridge's own feet cell may have lava under it, since its floor is laid first (`clear_over`).
- **The bridge move** (`MoveKind::Bridge`): one cell on the level whose floor is air or lava; the walk stands on the block before it and lays a held block against that block's side, then steps. It does not sneak (the protocol sends no sneak input; the server asks only that the clicked block is in reach). A bridge it cannot lay fails the walk; it never steps over a floor not laid. In the Nether only, with 16 bridge blocks held. Each block laid writes "bridge block laid at …".
- **The far search** (`plan_far`, `Walk::far`): f = g + 4h, 40,000 expansions. Probe 45336893578 on the recorded Nether (lane 5 of race 45194253932 at tick 25,140, from (19, 35, -20) to within 5 of (61, 33, 95)): weight 1 with 6,000 finds nothing; weight 2 finds 291 cells (76 bridged) in 147,900 expansions, 1.8 s; weight 3, 290 cells (91 bridged) in 35,525; weight 4, 222 cells (163 bridged) in 12,664, 0.1 s; weight 10, 218 cells in 8,906. At ticks 25,020 and 25,060, with half the chunks, a search spends all it is given and finds none, so a failed one is tried again 200 ticks on. The weight-4 route: nine cells bridged north off the ledge, over the land to the north-east, 15 bridged east, then a causeway 110 long at feet y 33 down the east cavern to an islet at (70 to 74, 32, 79 to 82) and 20 more to (63, 33, 91).
- **In `fortress_task`** (a fenced block, agreed with the orchestrator): one far search toward the nearest brick, its route held by the walk, a note with its cells, bridges and expansions; the tunnel stays the fallback.
- **What the search reads**: the blocks of loaded columns, as `path::plan` always has; it walks only through open cells and digs nothing.
- **Unit tests**: `crates/bot/tests/nether_walk.rs` (soul sand routed and walked, the feet's cell, a moat routed only with blocks held, the overworld not bridged, a chasm bridged block by block in the model, the weighed search); the test bed's `a_soul_sand_flat_has_a_route`, `a_lava_moat_has_a_route` and `a_lava_moat_is_crossed_to_the_bricks` no longer ignored.

### Raced: race 45337580375 (475e5f1, the Nether snapshot, 3 x 1, cap 12,000)

- **Lane 1**: the bridge works on a real server. Nine "bridge block laid" notes at ticks 80 to 120, (24, 33, -25) to (24, 33, -33): nine blocks in about 50 ticks, health 20, and the bot off the ledge 120 ticks after resuming. It was an `explore` walk that laid them (no bricks on record yet at tick 40), not the far walk. On the north-east land two piglins met it at tick 200: "defend Piglin 12 (Fight)", 5.1 a hit, dead at tick 300 ("death.attack.mob piglin").
- **Cause of the death**: the route crosses land where piglins stand (they were on record at 25 to 37 blocks when the route was chosen), and a bot in iron with no gold is attacked on sight. The defend row fights them and loses.

## Fix 2: a far route keeps clear of piglins; only a far walk bridges

- `Cells::far` holds where piglins, piglin brutes and hoglins stand (`SHUN_KINDS`); a far search enters no cell within 20 of one (`SHUN_RADIUS`; a piglin sees at 16), except within 6 of its start. A far walk plans again when one comes within 20 of its next 12 waypoints.
- Bridging is the far search's alone (`plan_far`): a plain `plan` (an explore, a walk to a block) lays nothing.

### Race 45337580375, all three lanes: two bots at `fortress`

- **Lane 2** (rs-l02-b00): tick 280 "far walk toward the fortress at (61, 34, 95) (a route of 201 cells, 149 to bridge with 626 blocks held, found in 8222 expansions)"; 148 blocks laid on that walk; **"stage fortress" at tick 1,600**; "explored to (61, 34, 95)" at 1,640, standing at (64, 32, 93). Health 20.0 from the start to the cap. 162 "bridge block laid" notes in the run.
- **Lane 0** (rs-l00-b00): far walk at tick 320 (187 cells, 149 to bridge, 6,280 expansions), cut at 900 by a skeleton (defend, 133 steps held back, unhurt), taken up at 1,020 (105 cells, 98 to bridge, 265 expansions), **"stage fortress" at tick 1,720**. 172 blocks laid, health 20.0.
- **Snapshots**: `race-45337580375/snapshot-fortress-45337580375-race-45337580375-lane-2.tar.zst` and `…-lane-0.tar.zst`.
- **Counted**: far walks found 3 of 3 searches; bridge blocks laid 9 + 162 + 172 = 343; deaths 1 of 3 (lane 1, the piglins, before any far walk).
- **After `fortress` both bots were stranded**: the bricks met are a lone pillar in the sea, the spawner on record at (129, 64, 159), 68 east and 31 up. `spawner_task` cut "stairs up toward the spawner" three times ("no floor at (64, 32, 93)"), walked the pillar's own bricks one by one, then explored up and down the causeway to the cap, at 20 health. Probe 45339612248 (lane 2 at tick 1,700): `plan_far` from (66, 32, 89) to within 6 of the spawner is Found, 177 cells, 121 bridged, 30 jumps, 25,091 expansions: the far walk is the way on from there (`Walk::far`, for the spawner leg's owner).

## Fix 3: off the pillar, and second to the tunnel on rock

- **The target** is the nearest brick with 200 brick columns in the nine 16-block squares round it (`walk::brick_in_the_body`), before a nearer lone pillar (60 columns here). Unit test `a_corridor_is_preferred_to_a_nearer_pillar`.
- **A step out of lava, tried and taken out.** The causeway is one block over the sea and the lift out of a liquid is half a block, so a bot knocked off it cannot climb back. Tried (3c1b500): in lava with a block held, lay a block under the bridge and a step against it, and swim to the step. In the model (probe 45344086862, a lake two deep, the body a cell from the bridge) the escape laid five blocks in 60 ticks and the bot died at tick 60 on the lake's bed: it sinks at once, rises 0.02 a tick with the jump held, and lava takes 4 a half-second (2.2 through iron on the real server, then 15 seconds alight). No block laid at the surface is reached in time. Reverted; a body in deep lava is dead, and what keeps a bot alive is not falling in: the walk never steps over a floor not laid, and a fight on a one-wide bridge is the defend row's to keep off the edge.
- **The order in `fortress_task`** (the orchestrator's, with the tunnel merged): a found plain route; then, on rock (`walk::rock_to_tunnel`: two full blocks under the feet and no lava at the door), the travel tunnel and after it the far walk; with no rock to tunnel (a causeway, a ledge with lava at its door) the far walk first.
- **The lava at the door, read again** (probe 45334717081): lane 5 of race 45204423376 did not die in its pit. At tick 22,680 it was back up its stairs at (30, 33, -21); at 22,700 it stood on the ledge at (29, 34, -22) and lava lay at y 36 over (29 to 32, -23 to -21), come down the hill's face. `lava_door` looked over the head and beside the head and feet; this lava was a cell higher and one aside, and was on the body 20 ticks later. `lava_door` now sees that ring too; the note at an escape's start says whether the lava was at the door and where, and its end ("out of the lava", or the run's end) carries its seals, steps, digs and trail.

### Raced again: race 45339584687 (e4c1d07: fixes 1 and 2, before the tunnel was merged; 3 x 1, cap 12,000)

- **Three of three at `fortress`**, at ticks 1,720, 1,700 and 1,700, each at 20 health. No death in the first 400 ticks (one of three before).
- **Far walks**: each lane's first search found its route at tick 220 (218 cells, 158 to bridge, 7,821 to 11,498 expansions). Bridge blocks laid: 170, 158 and 158.
- **The piglin rule fired in lane 0**: six "a mob to keep clear of is near the way ahead" notes at ticks 400 to 440, five plans in a row as the piglin came on (the walk's six plans spent), then a new far walk at tick 460 (190 cells, 161 to bridge) that went round and arrived. Lanes 1 and 2 met none.
- **After `fortress`**, with the spawner leg still a level walk, lane 0 paced its causeway and at tick 5,020 a ghast's fireball set it alight and put it in the lava ("death.attack.lava.player ghast", tick 5,140): the stranding of the first race, with the death that goes with it. Lanes 1 and 2 lived to the cap.

## Fix 4: the eighth of a block from soul sand is stepped, not jumped

- **What happened** (the model's kit bot on the real exit's fixture, CI 45341556367 and probe 45344086862): the tunnel came out at (61, 47, 79) at tick 2,660 with 20 health, 16 blocks from the bricks; the far walk laid 44 blocks, then every route from there failed NoPath for 8,000 ticks, none laying a block.
- **Read** (the walk's state every 10 ticks): the body stands at y 46.88, on soul sand, its next waypoint (61, 47, 80) a plain walk onto a full block an eighth higher, under a two-high roof. It never passes z 79.7: it jumps (the walk jumps when blocked), hits the lintel, lands on the soul sand still blocked by the eighth, and jumps again.
- **Cause**: mine. Soul sand as ground (fix 1) gave the walk a rise of an eighth, which the physics steps only on a tick with no jump.
- **Fix**: `follow` does not answer a block with a jump when the waypoint is a step (0.6) or less over feet on the ground. And its rule for "a cell up" is read from the feet's cell again (the first form of fix 1, "more than a step over the feet", let go of the jump for a body 0.4 to 0.5 under a waypoint, and the overworld's `the_planner_reaches_a_log_thirty_blocks_out_on_real_terrain` failed NoProgress on it, CI 45344088403).
- **The door, in the model**: `lava_at_the_door_of_a_stair_cut_is_sealed_out` passes (probe 45344086862) and is no longer ignored.


## Fix 4: the eighth of a block up from soul sand is stepped, not jumped

- **What happened** (CI 45341556367 and probe 45344086862, the model's kit bot from the real exit): the tunnel came out at (61, 47, 79) after 2,660 ticks; the far walk then laid 44 blocks and stalled: twenty-five far walks in a row "failed: NoPath", each a route of 20 cells found in 113 expansions, the bot at (61, 46 to 48, 79) for 8,000 ticks at 20 health.
- **Read** (the walk's state every 10 ticks, 4,130 to 4,470): the body stands at y 46.88, on soul sand, at z 79.7; the next waypoint is (61, 47, 80) by a plain walk, a full block's top an eighth of a block up, under a two-high roof. The body is in the air at y 47.0 to 48.1 in 27 of 32 rows and never passes z 79.7.
- **Cause**: mine (fix 1 made soul sand a floor). `follow` jumps when the body is blocked. The eighth blocks it for a tick; the jump lifts the head into the lintel over the next cell; it lands on the soul sand, still blocked by the eighth, and jumps again. A step is taken only on a ground tick with no jump.
- **Fix**: `follow` does not jump at a rise of a step or less (the physics steps it), and reads "a cell up" from the cell the feet stand in. The second half also undoes a regression: fix 1's "more than a step up" stopped holding jump for a body within 0.6 under its waypoint, and `the_planner_reaches_a_log_thirty_blocks_out_on_real_terrain` failed NoProgress (CI 45344088403).
- **Also read there**: `lava_at_the_door_of_a_stair_cut_is_sealed_out` passes (probe 45344086862) and is no longer ignored.

## Fix 5: the feet's cell is left as it was

- **What happened** (probe 45346724772, the whole suite on 113cd37, 329 of 333): with the tunnel merged, the model's kit bot from the real exit stood at (37.5, 51.875, 47.5), on soul sand, in one "tunnel stairs up" task from tick 2,900 to the cap; `the_printed_cell_is_the_planners_feet_cell` failed (the senses' cell and the planner's no longer agreed at y 63.875); and `the_planner_reaches_a_log_thirty_blocks_out_on_real_terrain` still failed NoProgress.
- **Cause**: mine. Fix 1 made `feet_cell` add 0.2 before flooring y, for every task. The stairs, the senses and every other reader of the feet's cell got a different cell on soul sand and in the top fifth of any jump.
- **Fix**: `feet_cell` floors again. Soul sand is handled where a route is made and walked: `plan` searches from the cell over soul sand, the walk's arrival reads `stand_cell`, `follow` knows feet at y + 0.875 are not a cell under their waypoint.

## Fix 6: the overworld's log walk, found by bisection

- **What happened** (probe 45349492975, the whole suite on fc9bed7, 363 of 364; probe 45350890077 on 070578d, 366 of 367): `the_planner_reaches_a_log_thirty_blocks_out_on_real_terrain` failed NoProgress, three walks to the log at (189, 75, 24) each failing about 440 ticks in; on `origin/issue-16e-observability` it passes.
- **First guess, wrong** (0eee250): `follow` read "on soul sand" from y's fraction. It now reads the block (the feet's cell is soul sand), which is right and kept, but the test still failed.
- **Bisected** (probe 45351964250: the test on 070578d with one change of this branch taken out at a time): it passes only with the waypoint rule taken out. Taking out soul sand's pace, the lava ring, soul sand as ground, the jump rule or the piglin rule each leaves it failing.
- **Cause**: fix 1 made every waypoint reached on a found route reset the walk's progress clock, for every walk; meant for the far walk's route round a lava sea.
- **Fix**: the reset is a far walk's only. Every other walk keeps clojurecraft's progress invariant as it was.

## The escape out of an aquifer (A1's stall, owned here with escape.rs)

- **What happened** (race 45343519424 lane 2, rs-l02-b00): own stairs down cut into an aquifer at y 55; from tick 14,780 to the run's end (35,980) the bot floated at (42.3, 55.2, -135.7) in "escape-water", 17 "escape stalled" notes, health 20.
- **Read** (probe 45351007612, `why --at 16000 --blocks 42,55,-136 --radius 15 --ys 51,60`): water two deep under the feet ((42, 54 and 55, -136)), the stairs' cells east of it floored with water at y 55, the aquifer round it standing at y 53 to 59. No standable cell near; the banks chosen were nine cells of the old stairs at x 28 to 30, 12 blocks west through stone. Each was "no nearer in 40", and `in_the_way` would not dig the wall toward it, which held water back ((40, 58, -136), (40, 56, -136)), so each was refused; past eight the list was cleared and the round began again. A notch is cut only with no bank at all, so it never was.
- **Cause**: a bank on record is taken to be reachable by swimming. Here none was.
- **Fix** (escape.rs): once a bank is refused, the next search takes a notch beside the body before another bank (trail "notch before another bank at …", in the escape's end note). A notch is never cut in a cell with water over it or beside it on the level: the nearest here, (42, 56, -135), has water at (41, 56, -135) and would flood; (44, 56, -137) is dry.
- **Test**: `water_far_from_any_bank_is_notched_beside_the_body` (crates/bot/tests/escape.rs), the race's shape in the model.
