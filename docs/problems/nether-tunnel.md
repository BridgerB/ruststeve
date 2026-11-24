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
