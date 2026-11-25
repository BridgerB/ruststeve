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
