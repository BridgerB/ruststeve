# The Nether walk, by tunnel (issue 16)

## Needs
A bot that stands in the Nether with its kit and sees nether bricks it cannot walk to has to reach them. 13 armed, fed bots stood about 110 blocks from bricks at (61, 30 to 34, 95) and none arrived (blaze.md walls 22 and 23 and the stop after them). Every bot that ever reached a fortress in this project tunnelled through netherrack from a start at y 64; none walked. The route attack on the same wall is `docs/problems/nether-walk.md`; this file is the tunnel.

## Risks
- Lava. The portal of the clean snapshot's world comes out at the lava sea's level (y 31 to 35) with lava falling beside it; a cut that opens a lava face kills in 80 ticks.
- No X-ray. A tunnel is a plan through rock the bot has not seen. The search may read only open cells, blocks with an open face, and fluid whose column's top is exposed; everything else is "rock". What rock hides is met one step ahead by `stairs::refusal`, the existing exception.
- Pickaxes: four stone pickaxes are 524 cuts; a level step is 2, a stair 3.
- The sim is kinder than the game (`docs/problems/nether-testbed.md`): gravel does not fall there, no mobs.

## Done, in world terms
A bot resumed from `race-45194253932/snapshot-nether-45194253932-race-45194253932-lane-5.tar.zst` stands within 6 blocks of nether bricks (stage `fortress`), alive, its ledger showing the tunnel's notes, and its `snapshot-fortress` is published.

## The picture (probe 45334838508: `survey` on race 45194253932 lane 5 at tick 25,700)
`survey` (crates/survey) prints level slabs and upright sections of the world a recorded bot had loaded. Read for this world:
- **The portal** is at (22 to 23, 34 to 36, -20) in a pit open to a great cavern above (air from y 35 to y 106 over (22, -19)). Lava falls beside it: a column at (18 to 20, 33 to 44, -21 to -17) and a stream down a slope at (25 to 32, 34 to 44, -25 to -18). By tick 25,700 the pit's floor at (21 to 25, 34, -19) is lava.
- **South of the portal is a hill of solid netherrack**: from z -18 on, x 0 to 60, y 32 to 56. Its top (y 57) is the floor of the cavern above. Magma lies in it at y 33 to 37 (the veins at (25 to 28, 33 to 36, -13 to 15) and others), gravel in pockets, a few lava cells shut in the rock (at y 44: (3, 13), (54, 15), (55, -6), (1, 73)).
- **Under it all is lava**: y 22 to 27 under the hill, and the lava sea at y 31 east and south of it.
- **A cavern cuts the straight line.** At y 44 the hill's east face runs from (63, 0) south-west to (15, 58): east of it is open air over the lava sea, 23 columns wide on the line from the portal to the bricks (columns 60 to 82 of 115). The face is lined with soul sand and soul soil 2 to 12 blocks thick. South of z 67 the rock comes east again to x 59.
- **The bricks** are a pillar from y 28 to y 67 at (61 to 63, 95 to 104), with one column of air between it and the rock at (59, 87 to 97) for y 40 to 56. At y 31 and under it stands in lava.
- **So there is a way wholly in rock at y 40 to 56**: south from the portal along x 14 to 22 to about z 66, then east along z 70 to 95 to x 59. No straight heading has one: every line from the portal to the bricks crosses the cavern.

## How earlier bots moved
- **Lane 5 of 45194253932, lanes 3 and 7 of 45204423376** (ledgers): "stairs down toward the fortress at (61, 34, 95)" two or three steps east to (30, 31, -18), then `StepFailed after 0 stairs` with nothing refused, hundreds of times (215 + 210 + 210 task starts in lane 3). Lane 5 died there in lava at tick 25,760; lanes 3 and 7 stood there for 20,000 ticks.
- **Lane 14 of 45164371770** (the 108 blocks): its only pickaxe broke at tick 21,900, 700 ticks in, and it cut the rest by hand at about 1,000 ticks for six stairs. Its tasks alternate "stairs down toward the fortress at (61, 25, 95)" and "stairs up toward the fortress at (61, 22, 95)": the nearest brick's height changes as bricks lower down come into view, the stairs go six down for a target one lower and six up for a target two higher, and the bot saws between y 22 and y 28, under the lava sea, until `Boxed ... lava beside (25, 23, 56)`.

## Walls

### 1. Stairs down from the lava sea's level, on one heading (the cause)
- **What happened**: above.
- **Cause**, in `fortress_task` and `dig_toward`: (1) with no walk the only plan was a cut straight at the nearest brick: stairs down six when it is a block lower, into the sea's level; (2) the heading is the larger of the two offsets and a refusal turns it a quarter, so nothing holds a way to the bricks; (3) nothing looks at what is open: the cavern on the line is met as "no floor" one cell at a time.
- **Fix: the travel tunnel** (`crates/plan/src/tunnel.rs`). A route of feet cells is searched from the feet to within 5 of the brick, each move a level step, a stair up or a stair down (a weighted A*, at most 60,000 nodes). Costs do the steering: a step under y 40 (`SAFE_Y`) costs 6 more, so a long route climbs out of the sea's level first and comes down only at its end, to the target's height; a step in the open costs 4 more than a cut. A portal's blocks, obsidian and bedrock are never opened. `Stairs::along` cuts the route six steps a stretch. When a step is refused (lava beside, magma under, gravel over, no floor) the stretch ends, the planner notes the reason, blocks that cell and searches again from where it stands. The target brick is held while the tunnel lives.
- **Progress invariant**: 16 stretches begun with the feet no nearer the target on the plane give the tunnel up and refuse the target a while.
- **Ledger notes**: "tunnel route N toward the fortress at …: K steps" (each search, with its nodes and whether it ends within reach), "tunnel stairs up / level / stairs down toward the fortress at …" (each stretch, with the step and the distance), "tunnel step to … refused" (each blocked cell with the staircase's reason), "tunnel off its route", "tunnel … given up", "no tunnel route".
- **Unit tests** (`crates/bot/tests/tunnel.rs`): a straight level route through rock; from y 34 a long route climbs to y 40 first and comes down at the end; a chasm is gone round; seen lava is kept a block away and a pocket shut in rock is left to the refusal, then gone round once blocked; a portal is never stepped into; and closed loop, the planner tunnels to bricks in a pocket under the ground round a wall of hidden lava.
- **On the recorded world** (probe 45337091364, `survey --route` on lane 5's recording): at tick 25,160, from the feet at (20, 35, -19) to the brick at (61, 34, 95), the search answers in 10 ms and 3,569 nodes with 163 steps: five stairs up south into the hill to y 40, level along x 22 to z 46, up to y 55 over the cavern's edge at z 47 to 60, level south to z 95, east along z 95 and down to (59, 39, 95). Eight of its steps are ones that world's hidden blocks would refuse (lava beside (22, 40, 42), and seven with soul sand for a floor at (21, 50 to 54, 56 to 62)): each is one blocked cell and one more search in the run. At tick 24,990, with the far chunks not yet loaded, it answers "ends short" after all 60,000 nodes in 158 ms: a partial route 93 steps long, taken because it ends nearer. At arrival (tick 24,990) no lava is within 8 blocks of the portal at y 34 to 37: the lava that killed lane 5 came later.

### 2. Nothing to step onto (the Nether test bed's lake under a hill)
- **What happened** (`docs/problems/nether-testbed.md` wall (c)): on a cavern's flat floor a stair up has no block to step onto, so no route climbs into the rock over the lake.
- **Fix**: a laid stair. With netherrack, cobblestone or cobbled deepslate held, a stair up whose step is missing lays it first: the cell under the step, clicked onto the side of the block the feet stand on, when that is open too, then the step on its top (`stairs::lays`, `StairStage::Lay`); the search prices each laid block at 2. Never into or beside a fluid.
- **Also**: a route ends within reach of any brick within 12 of its target, not only the brick first seen, which is often a pillar's foot in the lava (`tunnel::GOAL_SPREAD`).

### 3. "cut 6 stairs" with nothing cut (found by the test bed)
- **What happened**: level tunnels along a shore ended "cut 6 stairs" in 40 to 60 ticks, hundreds of times, in the model and in the races' ledgers: six level steps over cells already open, back to where they began.
- **Fix**: a staircase that dug nothing and ends within a block of where it began fails `Boxed` with "nothing cut and nowhere gone".

### 4. No bricks in view and no walk out
- **Fix**: `Plan::explore_tunnel`: a search tunnel on a held compass heading, 48 blocks a leg at the travel height, the heading turned a quarter only when a leg has no way. Notes "search tunnel leg toward …" and "search tunnel turned to …". Not yet seen in a race: every bot of this snapshot has bricks in view.

## Raced
### Race 45337739022 (3079a6b, the Nether snapshot of race 45194253932 lane 5, 3 x 1, cap 12,000)
- **3 of 3 at stage `fortress`**, at ticks 8,540, 8,000 and 9,400, each at 20 health, by the tunnel alone (no "walk toward the fortress" in any ledger). Lane 0 went on to stage `spawner` at tick 11,940 at (128, 61, 166), 20 health. Lane 1 died in lava at tick 9,480 on the fortress, inside a `defend`, after the tunnel's work was done; lane 2 was alive in `defend Blaze` at (114, 62, 186) at the cap.
- **Published**: `snapshot-fortress-45337739022-race-45337739022-lane-0.tar.zst`, `-lane-1`, `-lane-2` and `snapshot-spawner-45337739022-race-45337739022-lane-0.tar.zst` on release `race-45337739022`.
- **The notes, counted** (lanes 0 / 1 / 2): "tunnel route N" 25 / 23 / 24; stretches begun ("start … stairs: tunnel …") 59 / 58 / 59; "tunnel step to … refused" 22 / 22 / 22; "tunnel off its route" 2 in lane 0 (after a `defend` cut a stretch); "given up" and "no tunnel route" 0.
- **The way taken** (lane 0's ledger): route 1 at tick 220, 157 steps, 4,511 nodes; stairs up south from the portal to y 40 and level along x 22; "lava beside (22, 40, 42)" refused at tick 2,040 and gone round; from tick 2,940 to 6,860 the soul sand face at (21 to 35, 50 to 63, 53 to 62), 19 refusals "no floor" (soul sand under the step) and 3 "the step ended short there 2 times", the route climbing from y 51 to y 64; then 60 level steps at y 64, south to z 95 and east to the bricks, in 1,700 ticks. The stage was met at (50.9, 64, 95.5), in rock, 11 steps before the route's end. The lava that killed the earlier bots at the portal never met this one: it was in the hill by tick 400.
- **The magma refusal** (blaze.md wall 23, 4b214ac): "magma under" 0 times in all three ledgers. The route leaves the magma band (y 33 to 37) in its first five stairs, so the refusal had nothing to refuse. It stays, unproven: it costs nothing, and the search treats seen magma the same way.
- **Costs**: about 2 pickaxes of 4 (lane 0 held 2 spares and one at 60 uses at the fortress), 266 netherrack in the pack.

### What the race showed to fix
- **The soul sand face cost about 2,500 ticks a bot**: each refusal blocked one cell and the next route stepped to the cell beside it. Fix: rock within 3 steps of a blocked cell is suspect, 8 more a step (`SUSPECT_NEAR`, `SUSPECT_COST`); and a stair's floor is whatever `path::classify` calls a floor, so when the route takes soul sand as ground (nether-walk.md) the stairs do too.
- **Stairs failures with no reason** ("last refusal: ;", 644 in one ledger of race 45204423376): a failed step now says the cells, the walk's failure and the floor's block; a failed dig its failure; `Boxed` every heading's refusal.
- **The spawner leg**: `dig_toward` in the Nether hands a target more than 8 blocks off to the travel tunnel (`Plan::tunnel_toward`, the route ending only at the target itself), in place of six stairs down or up at a time.
- **Laid blocks that never showed** (the same race, read after): 53 stairs failures "a block laid at … did not show" over the three lanes (the cause under the "ended short 2 times" refusals), each where a new route turned back up over the cell the feet had just left, as at (23, 56, 61) at tick 3,800 of lane 0. Cause (read in `stairs.rs`, not yet seen fixed in a race): a step is done the moment the feet cross into its cell, 0.3 of the body still in the cell behind, and a server refuses a block that would overlap the player. The model's test laid only ahead. Fix: before a block is laid into a cell the body's box is in, the bot walks to the middle of its own cell.

### 5. A step whose walk has no start (not yet seen in a race)
- **Risk read from the code**: a tunnel step is walked by `Walk`, and `path::plan` has no start for feet on soul sand or beside lava (blaze.md wall 23's brief): a bot that arrives from its portal on soul sand would fail every first step.
- **Fix**: in a travel tunnel a step whose walk fails is steered straight at the cell just cut (one block away, already passed by `refusal`), jumping for a stair up. The task's end note counts them: "cut N stairs (L blocks laid, S steps steered straight)".
