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
- **Waits on**: Q3's `Plan.fix` (the caller sets `overland.fix` from it at the merge); Q1's memory record of `EndPortalFrame` (`frames_near` reads memory's sightings; until then no frame is ever seen by the planner).
- **Unit tests** (`crates/plan/tests/overland_stronghold.rs`, hand-built worlds): the room from its frames, the standing cell clear of lava, the dry descent out of the footprint, infested blocks never opened, a river bridged, an ocean gone round, a hut at dusk, the progress turn, the arrival's hand-over to the search.

## Walls
(none yet)
