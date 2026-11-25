# Issue 18c: from the endermen pocket to a warped forest (agent P2)

The orchestrator's index is `docs/problems/pearls.md`; this file is agent P2's (the hand-over, the travel, the heading's rules past the endermen stage).

## Needs
A real bot resumed from the `endermen` frontier (race-45481633896 lane 3: sealed in a 12-block pocket at about (8, 85, 81) in a soul sand valley, health 12.5, food 17, no food held so it cannot heal, no shield, 144 pickaxe uses, about 588 filler, 7 rods) leaves its pocket and reaches a warped forest alive with its 7 rods: the feet's biome `warped_forest`, stage `forest`.

## Done (world terms)
A case file from a race resumed at the frontier shows `stage reached: forest`, and the race's release carries its `snapshot-forest-…` asset.

## Risks
- No healing: every hit on the way is kept (12.5 is about three enderman hits, two piglin brute hits).
- This world's only warped forest within 600 of the spawner lies about 454 blocks east-north-east behind about 300 blocks of basalt deltas (17a, development knowledge: never in code). The heading has never raced, and today it turns away from basalt signs.
- Ghasts in the valley and the deltas, magma cubes in the deltas, piglins (hostile: no gold worn), wither skeletons near the fortress.
- One world: every frontier race is the same seed, forest and route.

## What is built (branch `issue-18-p2-travel`)
- `EnderState.phase: EnderPhase { Reach, Travel, Hunt }` (serde default `Reach`).
- The hand-over: under `Aim::Race` a pocket closed (or one that cannot be sealed) at the stage notes "endermen reached: on to the forest" and moves to `Travel`; `met` is no longer set there. The stage seen by the world's ladder (`w.stage_reached >= endermen`) also counts as reached, for a resumed bot's first look.
- The way out: `endermen::exit_side` picks a side of the pocket (no lava beside the cells it opens; first a side not facing a ghast, blaze, skeleton, wither skeleton, piglin, brute, hoglin or magma cube within 48; then the longest standing run past the wall; then toward a known forest), and `exit_step` digs its head and feet cells ("pocket opened at", "out of the pocket to"), at most 4 digs.
- In `Travel`: no approach of endermen, no valley (the heading's valley signs are dropped too), no forest wait. A known forest is walked to by cell (`endermen::forest_target`: seen warped nylium in a known forest column with room for a body over it, within 12 of the feet's height first, then the nearest), ending within 1 of the cell; else the heading. A "to the forest" note every 1,200 ticks and every heading leg's note carry health, food, pickaxe uses and filler.
- The feet's biome `warped_forest` moves to `Hunt` ("warped forest reached: on to the hunt"); `Plan::pearls_task` (P3) runs there (a stub returning `None` until it merges).

## The rules chosen from 18a.2 (P1's geography, development knowledge only)
From the frontier no compass axis to 640 crosses a warped forest; the nearest, about 516 blocks east-north-east at x 470 to 540, z -127 to 49, lies north of the east axis behind basalt deltas from x 160 to 640. The rules, written as what a player could see:
- **Basalt deltas are crossed**, never a turn reason: they spawn no piglins or hoglins; a route never stands on a magma block (awkward, never a floor) and keeps 8 from a magma cube (`path::SHUN_MAGMA`). "heading crosses basalt deltas" once a heading.
- **Crimson still turns** (hoglins, piglins hostile with no gold worn).
- **In travel a fortress seen ahead turns** (12 or more nether bricks in the next 1 to 4 chunks of the axis, or a ticking blaze or wither skeleton ahead within 48): no healing, and blazes and wither skeletons live nowhere else.
- **Past the endermen stage soul signs count for no axis**, and the heading's valley target is dropped.
- Wither skeletons join the far route's shunned kinds; the radius is 24.
- No axis rule here points east from the frontier: the heading's own choice (away from the explored, then the turns) decides, and that is the point of racing both snapshots.

## Walls

### 1. The first travel races (570fa62, before the forest stage merged: no `forest` snapshot possible)
Races 45531575517 (frontier, 4 lanes) and 45531577912 (lane-2 snapshot, 2 lanes), cap 24,000.
- **The hand-over works**: every read ledger has "endermen reached: on to the forest" and "out of the pocket to …" in 2 digs (2 pickaxe uses). A resumed frontier bot built a second pocket first (its roof block re-placed 16 times, "pocket given up" at 24): 240 ticks.
- **Frontier lane 3** (`rs-l03-b00`): "heading chosen: (0, -1)" (away from the centroid of 111 seen columns at (76, 198)), turned at once to (-1, 0) by 503 crimson signs ahead; five legs west (walk and far walk, about 1 block placed a metre, no digs, 240 m), then into a second fortress (bricks x -320 to -180): "out of the fortress footprint" walked it along it, and a blaze's fireball and its fire took it from 12.5 to 5.6 at tick 3,900. It holds in a pocket with nothing to heal with. Fixed in 2daa2d8: in travel a fortress seen ahead turns the axis, and no footprint walk.
- **Lane-2 snapshot, lane 1** (`rs-l01-b00`, health 17.1 throughout): north, crimson turns west twice, two tunnel legs over their dig budget (61 uses: 144 to 55), a re-choice and a crimson turn onto (1, 0); nine legs east, walk and far walk, about 0.8 placed a metre, "heading crosses basalt deltas" at x 197; at tick 7,160 "warped forest known" (chunk columns 29 to 33). The wall: from (437, 40, 4) at the end of its own bridge, 35 blocks short, every walk to a forest cell fails NoProgress at cell 0 with no block laid and every tunnel finds no route, from tick 8,500 on.
- **Frontier lanes 0 to 2 died** (piglins twice, a hoglin once; 12.5 to 7.4 to 2.2 in two blows of 5.2) on plain walks: a plain route never shunned mobs, only a far one did. Fixed in 708a62a: the travel's plain walks (heading legs and forest walks) shun as far ones do (`walk.more.shun`).

### 2. The swap that undid itself (race 45531577912 lane 1, from tick 8,300 to the cap)
- **What happened**: 35 blocks from the warped forest, every walk to a forest cell failed NoProgress "at cell 0 with 0 blocks laid". Its senses rows alternate "cursor netherrack 64" and filler 351 / 415 with cobblestone in hand.
- **Cause**: `path::bridge_item` chose the bridge kind with the most held, read afresh each tick; lifting the netherrack stack onto the cursor for the swap took 64 off its count, cobblestone became the choice, the cursor's stack went back down, netherrack became the choice again: the bridge's hold never ended, and no block was laid.
- **Fix** (dbe0371): the bridge block in hand first, then the one on the cursor, then the most held; `bridge_blocks` counts the cursor's stack. Unit test `the_bridge_block_choice_holds_through_a_swap`.

### 3. Steering by known geography (Bridger, 2026-11-24)
- Ruling: "I don't care about ANY direction ... I just want to win": the travel may steer by the world's known geography. `config/goals.toml` `[survival] travel_way = [[440, 60], [488, -111]]` (probe 45529738504's locate; east along the deltas, where lane 1 saw the forest from x 341, then into it). In travel the heading's legs run to each point's column in turn ("heading to the travel point", "travel point reached"); a forest known from blocks seen replaces the point and the forest walk by cell takes over. Strict no X-ray still holds for every block acted on.

### 4. Skeletons, fixed-cell legs, deltas tunnels (708a62a races 45535712274 and 45535726218, cut short)
- Frontier lanes 1 and 3: soul sand valley skeletons, three arrows each, 12.5 to under 6. Fixed in 1c17850: skeletons shunned at 16 (`path::SHUN_SKELETON`).
- Lane-2 snapshot lane 1: legs to a fixed cell (the point's column centre at the feet's height) found no way, "far walk Partial, no tunnel route", and the heading turned out in 80 ticks. Fixed in 1c17850: a travel point aims the axis (`heading::aim`), legs run as ever (`Goal::Ahead`), a failed leg turns (a detour) and the next leg done aims again; a forest leg ends on the plane at any height (`Goal::Plane`).
- Frontier lane 0 tunnelled 84 pickaxe uses in three over-budget legs at y 86 in the deltas. Fixed in bd1f9b4: in travel a heading tunnel needs `TRAVEL_SPARE` 60 uses over the reserve and the leg's own.
- A granite junk drop of 25 failed a leg "25 placed in 0 m": a leg's placement now leaves out the junk kinds (`leg_filler`).

### 5. The forest reached (1c17850, race 45537545167 lane 0, the lane-2 snapshot)
- `rs-l00-b00`: "endermen reached: on to the forest" at tick 400 (a sealed pocket of 13), out to (0, 1) in 2 digs, "heading to the travel point (440, 60) on (1, 0)", three legs east by far walk (0.56 to 1.0 placed a metre, no digs, no health lost), "heading crosses basalt deltas" at (226, 41, 194), "warped forest known" from tick 2,000 (chunk columns 29 to 33, 9 to 12), the forest cell walk ("to the warped forest at (464, 68, 160)", then "(498, 42, 153)" with "2 nylium cells refused"), `stage forest` at tick 4,020, "warped forest reached: on to the hunt" at (497, 42, 153): health 18.3, food 17, 7 rods, 142 pickaxe uses, 263 filler. Snapshot `snapshot-forest-45537545167-race-45537545167-lane-0.tar.zst`. One world: this seed, this forest, the route east along z 150 to 200.
