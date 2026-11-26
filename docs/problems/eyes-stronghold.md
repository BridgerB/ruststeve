# Overland and the stronghold (issue 19e, 19f)

Agent Q4's wall file; the index is [`eyes.md`](eyes.md). From the overworld surface with the eyes' fix in hand to standing in the stronghold's portal room.

## Needs
- **Overland**: from where the bot comes up (near the old stairs' top at (55, 70, -133)) to the fix, about 1,537 blocks almost due east in this world (Q5's probe 45685968956, development knowledge only), on foot, by day, with a hut at night, rivers bridged, oceans gone round, no healing without food.
- **The dig**: from the surface near the fix into the portal room, never through open caves, never over the lava under the ring or the side pools by the door, never opening an infested block; the spawner broken, a nook sealed at the ring's side.

## Risks
- Night mobs and creepers on a two-day walk; a bot that cannot heal (food under 18 with nothing to eat).
- Water: aquifers beside the stairs (overworld-leg wall 7), rivers, an ocean on the bearing.
- Silverfish: the spawner wakes them within 16; an infested block broken wakes its neighbours.
- One world, one stronghold, one route: every result here is said with that.

## Done, in world terms
A race from the frontier reaches `stronghold` (a frame seen within 128), then `portal_room` (the feet on the room's floor within 7 of the ring's centre).

## The build (2026-11-25)
- **`crates/plan/src/overland.rs`**, `Plan::overland_task`: legs of 64 to 128 to a cell on seen ground on the bearing to `OverlandState::fix` (set by the caller from the eyes' `Plan.fix`); a far walk that bridges over water (`path::Cells::far_over_water`, keep 32, at most 40 a leg); an ocean (64 or more of water on the bearing) turns the bearing 60 toward the side with more land seen; a leg with no gain in 600 ticks or three walks turns 30; the fix 32 nearer every 1,200 walking ticks or a turn; a hut under `DUSK_MARGIN` or under the danger line with food (a held furnace put down and raw meat cooked inside), its wall dug on the fix's side to leave. None once a frame is seen; within 24 of the fix with none seen, the stronghold's search starts there.
- **`crates/plan/src/stronghold.rs`**, `room`/`room_of` (pure, the ring's centre from any frames seen, the feet's floor two under the frames: the `PortalRoom` template), `stand_target` (on the floor, out of the ring, two or more from lava), `Plan::stronghold_task`: dry stairs out of the room's footprint (toward the target while they end 10 or more off the centre, else away), the travel tunnel at the floor's level, a plain walk only within 24 at that height; inside, the spawner broken, the standing cell, a nook; with no frame at the fix, dry stairs down to y 30 and a square spiral of tunnel legs toward stone bricks seen.
- **Shared, fenced**: infested blocks refused by `stairs::refusal` and walls to `tunnel`'s route; `walk::More::water`; `[overland]` and `[stronghold]` in `config/goals.toml`; one call site in `nether_task` (`eyes_stronghold_task`, in the overworld with eyes held or a room known).
- **Merged** (c680b59, 5e5dfa7): Q1's memory record of `EndPortalFrame`; Q3's `Plan.fix`, `eyes_task` and `eyes_way` wired in `eyes_stronghold_task` (pickaxes, the stronghold, the eyes' throw, the overland to the fix or to the next throw's spot).
- **Unit tests** (`crates/plan/tests/overland_stronghold.rs`, hand-built worlds): the room from its frames, the standing cell clear of lava, the dry descent out of the footprint, infested blocks never opened, a river bridged, an ocean gone round, a hut at dusk, the progress turn, the arrival's hand-over to the search.

## Walls

### Wall 1: no frame on record; the search stops at the stronghold's wall (race 45689919380, 2026-11-25)
A development race (`dev-stronghold`, `dev_fix` (1648, -16) on the throwaway branch `issue-19-q4-dev` at c958f6f, 2 lanes x 1 bot, cap 12,000, resumed from the frontier). Development knowledge, never evidence.
- **What worked**: lane 0's overland reached the fix in 4 legs and 1,320 ticks. It turned 30 once at a 44-block water crossing (over the 40 limit) and bridged a 31-block river with 31 netherrack (`bridge block laid` x31). The dry stairs took it from y 66 to y 29 in 6 legs.
- **Cause 1, structural**: at c958f6f no frame was on record. `frames_near` reads `world::stage::kind_near(EndPortalFrame)`, and only Q1 (6a175ac) watches and indexes that kind. The real room is 20 west and 18 north of the located point (Q5's probe 45690314001: ring x 1626-1630, z -36..-32, y 19, feet y 17), well inside the view distance. With Q1 merged (c680b59) the room should be found from the surface, and the search becomes the fallback.
- **Cause 2**: the search's tunnel ends within 5 of a seen brick (`tunnel::REACH`, to the cell), but a dig reaches 4 from the eye.
  - Lane 0 stood at (1646, 29, -8), 5.3 from the bricks at (1650, 27, -10). The tunnel returned `None`, and the search fell through to the spiral with the false note "no frame and no stone bricks seen".
  - The spiral's route climbed to y 35, over `search_y` + 1, so the stairs down began again (leg 7). The bot went down and up at the wall.
  - Lane 1 stood over bricks 5 straight down at (1660, 22, -18). It flipped between two bricks and the spiral point every 20 ticks from tick 5,200 to the cap.
- **Cause 3**: lane 0's one stone pickaxe came down the stairs to 13 uses, with 67 cobblestone and no table in the pack, and broke. It ended on a wooden pickaxe. `spares_wanted` is closed once a portal is known.
- **Fixes** (8894152, plus the step down after 5e5dfa7):
  - `descended` latches the search's stairs.
  - A wall step digs toward a brick short of a dig's reach, on the level or down through the floor. A brick in reach is broken, `wall_breaks` (4) at most.
  - The spiral's note gives the bricks seen.
  - `stronghold_pickaxes` makes `spare_pickaxes` stone pickaxes from the pack's table and sticks (added to both dev kits) between tasks.
- **Count to confirm**: "portal room found" from the surface; "stronghold wall step" and "stronghold wall broken" firing only when no frame is seen; "stronghold pickaxes held".

### Wall 2: the room is found from the surface; a full pack and the floor-level flip (race 45695074789, 2026-11-25)
A development race on b09cf7b (`dev-stronghold`, no `dev_fix`: the eyes thrown 150 west of the stronghold), 2 lanes x 1 bot, cap 12,000, from the frontier. Development knowledge, never evidence.
- **What worked**:
  - Both lanes found the room from the surface, with Q1's frames: "portal room found: ring centre [1628, 19, -34] (12 frames seen, floor feet y 17)". That matches Q5's probe 45690314001 exactly.
  - Lane 1 found it at tick 300, 133 off, after one throw. Lane 0 found it at tick 1,240 on the overland's first leg, 116 off.
  - Lane 0's throw 1 shattered, and the craft safety net made the eye back from a rod and a pearl.
  - Lane 1 made a stone pickaxe at tick 1,300: table placed, crafted, table dug again.
  - Both reached stage `stronghold`, and the dry stairs went down 6 a leg with no water.
- **Cause 1, a full pack**: the frontier's pack came up full (67 netherrack, 50 soul soil, warped blocks and roots).
  - Lane 0 took no cobblestone from 9 stair legs, so it never had the makings of a pickaxe. Its one stone pickaxe was down to 5 uses at tick 4,780.
  - Lane 1's second round crafted planks into a full pack 8 times ("Missing", the stack left on the cursor) until `pick_tasks` ended the round.
  - Fix: `[stronghold] junk` (the Nether's leftovers, 64 netherrack kept) joins the junk drop in the overworld while the stronghold is the aim, and no pickaxe round begins on a full pack.
- **Cause 2, the floor-level flip**: lane 0's stairs reached the floor's height at (1591, 24, 34), 103 blocks off the target. The tunnel's 102-step route climbs (to y 40), and after its first leg the stairs down began again, the same down-and-up as wall 1. Fix: `at_floor` latches. From then on the stairs take over only beyond `tunnel::RISE` (16) above or below.
- **Stand target**: (1624, 17, -30), on the south-west side of the ring for a bot coming from the south-west. It is outside the ring's ±3 box, with no lava within 2. Not yet walked to.

### Wall 3: the yo-yo round the floor; 19 off at the cap (race 45698008920, 2026-11-25)
A development race on a3cfb50 (`dev-stronghold`, no `dev_fix`), 2 lanes x 1 bot, cap 12,000, from the frontier. Development knowledge, never evidence.
- **What worked**:
  - Both lanes found the room from the surface at tick 1,160.
  - The junk drop made room: quartz, nuggets, soul soil, warped roots and nylium, then the stairs' stone.
  - Stone pickaxes were made three times in all (lane 0 at 1,600; lane 1 at 2,040, 4,800 and 8,720, after its pickaxe broke at 8,260). No lane went without one.
  - Lane 0 ended at (1616, 17, -24) on mossy stone bricks, tunnelling at the floor's level, 19 off its standing cell (step 12 of 25).
- **Cause**: lane 1's stairs legs cut 6. From y 19 a leg ended at 13, under the floor's band (16 to 18), and the stairs up went back to 19. `at_floor` latched only within 1 of the floor, so it never latched, and the bot repeated the same two cells from tick 8,820 to the cap.
- **Fixes**:
  - `at_floor` latches within a stairs leg's cut (`STAIR_LEG`, 6) of the floor.
  - Far rooms are walked to on the surface first (`stronghold_surface`): both lanes began their stairs 112 to 118 off and reached the floor's height 78 to 92 off, leaving the tunnel the rest.

### Wall 4: 4 blocks short; the tunnel's climb against the stairs (race 45703184922, 2026-11-25)
A development race on 47589db (`dev-stronghold`, Q2's home merged), 2 lanes x 1 bot, cap 12,000, from the frontier. Development knowledge, never evidence.
- **What worked**: every earlier fix fired.
  - Home was over at tick 120-140, and the planner went to the eyes' phase (47589db).
  - Q3's walks to open ground ran before throw 1.
  - The room was found from the surface at tick 240-340.
  - Two surface walks took the bots from 89-107 off to 73 off.
  - Two pickaxe rounds a lane; no lane went without a pickaxe.
  - `at_floor` latched at y 23.
- **Cause 1, lane 0**: the tunnel took it to (1620, 20, -30), 4 across and 3 over its standing cell (1624, 17, -30). The tunnel's route ends within `tunnel::REACH` (5) of its target, and the plain walk needs the feet within 1 of the floor. So "stronghold tunnel has no way" 326 times, from tick 5,480 to the cap.
- **Cause 2, lane 1**: at the floor, the tunnel's 40-step route climbed to y 40 under a river (3 cells blocked). At 22 over the floor, beyond `RISE`, the stairs down began again and alternated with the tunnel from tick 7,780 to the cap, with 3 water escapes.
- **Fixes**:
  - At the tunnel's end within `REACH` + 2: a walk to the cell (a drop into the room), else a wall step.
  - Once at the floor, the stairs run only when the tunnel and the last steps have nothing.
- **Still open**: the junk drop and pick-up churn: 167 leather drops in lane 0, one every 40 ticks while standing still. That is the shared junk mechanism, not this file's.

### Wall 5: the climb under the river, the radius of 7 (races 45706776687 to 45716720148, 2026-11-25)
Development races (`dev-stronghold`, from the frontier, 2 lanes x 1 bot, cap 12,000). Development knowledge, never evidence.
- **Results**: `portal_room` 1/2 on dc2fee6 (45706776687), 2/2 on a6bc06d (45709849027), 1/2 on 11311c0 (45714294764), 2/2 on d867b43 (45716720148). Every bot that reached it broke the spawner, sealed a nook and waited at 20 health.
- **Cause 1, the climb**: `tunnel::SAFE_Y`'s cost under y 40 is the Nether's lava sea. In the overworld it took the routes to a room at y 17 up to y 40, under the river at x 1583 to 1614 and into its water (4 escapes, the last still trapped at the cap). Fix (73e5562, 11311c0): `tunnel::Limits::max_y` (fenced; every other caller keeps the old box) caps the room's route at the floor + 6. With no capped way, the approach turns to the room's north or south side twice, then the uncapped route.
- **Cause 2, inside by a radius**: `inside` was 7 from the ring's centre. A bot one cell into the west wall, 5 off on x, counted as in. It sealed and waited out of the stage's ±4.5 box. Fix (d867b43): `inside` reads the stage's own box, as `stand_target` does.
- **The junk churn** (a6bc06d): "junk held back" and "junk placed" fire. Crimson fungus drops fell from 123 to 29 in a run; leather still comes back while walking.

### Wall 6: the whole overland; a throw that waits, junk placed one block at a time (race 45719263137, 2026-11-25)
A development race on d52804d (`dev-overworld-eyes`, the bot at the surface by the overworld portal, from the frontier), 2 lanes x 1 bot, cap 48,000. Development knowledge, never evidence.
- **Lane 0, what worked**:
  - The throws gave a fix with err 70.
  - 11 legs east, with 3 turns, 26 bridge blocks (the rivers at x 464-528 and 1583-1614) and a night hut of 11 blocks at (647, 72, 79), from tick 5,660 to dawn.
  - 1 meal; the food never fell under 19 and the health ended at 20.
  - Stage `stronghold` at tick 27,440. Then the stairs, 2 pickaxe rounds (one pickaxe broke at 38,260), 10 silverfish fights, 15 last steps and 12 wall steps, to (1623, 18, -28), a cell from its standing cell.
- **Cause 1, lane 1**: at throw 2's spot (tick 2,660), Q3's guard read a canopy 12 out on the bearing, and the throw waited for good. Nothing moved the bot; it idled 21,000 ticks, its stuck snapshot published. Fix (b3293bc): after 600 ticks waiting at the spot, it moves 20 blocks round the compass, 4 times at most ("overland throw spot moved").
- **Cause 2, lane 0**: from tick 35,500 to the cap, `junk_place` put cobblestone down 270 times, one item a task against stacks of 64 that never freed a slot. Each placed block was dug back by the last steps. Fix: a held-back block is placed only from a stack of 8 or under (`JUNK_PLACE_MAX`); a bigger stack is released from the hold and dropped whole ("junk released").
- **Leather** is off `[survival] junk` (d52804d): kept, 4 items in one slot.

### Wall 7: boxed in by its own hut (race 45725479883, 2026-11-25)
A development race on 5c62f28 (`dev-overworld-eyes`, from the frontier), 2 lanes x 1 bot, cap 48,000. Development knowledge, never evidence.
- **What worked**: lane 1's throw spot moved after 620 ticks of waiting ("overland throw spot moved (1 of 4)"); throw 2 went and the fix came (err 70). Both lanes walked east and huted at dusk.
- **Cause**: the hut's exit digs one cell on the fix's axis at the feet and the head. Lane 1's hut took 22 blocks (the body moved while it was built), so those cells were air inside it and the wall stood beyond. From dawn (tick 17,660) every far walk died in 4 expansions: 302 failed in a row, turning the bearing each time, to the cap. Lane 0 was the same at (55, 80, -36), 2 expansions.
- **Fix**: a far walk that dies within `ENCLOSED_EXPANSIONS` (8) is boxed in. A wall cell is dug toward the leg's target, else on any side, head before feet, never infested or the furnace ("overland dig out").

### Wall 8: the moved spot counted as reached (race 45729721263, 2026-11-25)
A development race on bbfe300 (`dev-overworld-eyes`, from the frontier), 2 lanes x 1 bot, cap 48,000. Development knowledge, never evidence.
- **Cause**: a throw's walk point was reached at `[overland] arrive` (24), the fix's radius. The spot moved 20 ("overland throw spot moved", 4 of 4 in lane 0) counted as reached where the bot stood, and the throw waited on the same canopy at (216, 75, 29) from tick 2,320 through the night, idle 14,600 ticks. Both lanes did the same.
- **Fix**: a walk point is reached at a leg's own reach (`leg_reach`, 6), and never hands over to the search. A leg with nothing seen at its length looks no farther than its aim (a 20-block move is not overshot by a 32-block leg).
