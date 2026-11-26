# Issue 20c and 20d: armed in the overworld, the frames, the entry (agent E2)

The orchestrator's index is [`end.md`](end.md); this file is agent E2's. The code is `crates/plan/src/end_portal.rs` (brain note `end-portal`), called from one fenced block in `nether_task`.

## Needs
- **20c, armed**: the frontier bot (race 45800967549 lane 4, on the portal room's floor at (1624.4, 17, -35.7)) holds no pickaxe, no wood and no table, 3 sticks, 30 raw meat, 7 raw iron, a furnace and 116 cobblestone. It goes up its own stairs (their top at (1608, 63, -32), its ledger at tick 43,720), takes wood by hand, makes a table, sticks, 3 stone pickaxes, an iron sword and iron boots (worn) from the 7 iron with planks as fuel, cooks all the meat (a hut at night), eats to 20 and comes back down to the room.
- **20d, the portal**: the spawner at (1632, 19, -34) broken with the pickaxe from a cell not over lava; an eye into each of the 11 empty frames (E1: only (1627, 19, -32) holds one; 13 eyes held) from the floor outside the ring within 4.5 of the frame's top; the step into the lit 3x3 at y 19; done when the dimension is `minecraft:the_end`.

## Done (world terms)
A race from the frontier (`snapshot=frontier`, revive false) reaches stage `armed`, then `end`, each with its snapshot asset.

## Risks
- Silverfish from the spawner while the bot is within 16 (the frontier bot broke it by hand only after 3,400 ticks of fights).
- Night on the surface; the hut must be dug out of in the morning.
- The lava pool at (1628, 17, -34) under the ring's middle.
- One world: one stairs, one surface, one room.

## What is built (branch `issue-20-e2-portal`)
- The kit's phases (up, wood, row, cook, eat, down), each noted at its end with the kit held, each bounded by `[end_kit] phase_ticks` and `phase_fails`.
- The `end-kit` goal row (`config/goals.toml`): 3 stone pickaxes, an iron sword, iron boots.
- The spawner and the eyes (`frame_has_eye`, `empty_frames`, `eye_stands`).
- The step in (`entries`): E1's room has the frames at y 19 floating two over the floor (feet y 17), the lava pool at (1628, 17, -34) with air over it. Two blocks are laid outward of a side's frame on the floor (two high next to the frame, one beyond), then a forced walk up them, over the frame's top (19.81) and into the portal cell at y 19. Ledger: "end portal: the way in by the frame at", "step block at", "step in by the frame at", "given up".

## Walls
1. **No route up from the room** (race 45865135195 on fc46fa7, frontier, 2 lanes x 1 bot, cap 20,000). "end kit: no walk up its own stairs to (1608, 63, -32) (route Some(None))" at tick 40; the dry stairs then cut into the frames ("a dig for the stair to (1627, 19, -36) failed Refused", 12 times), relocated, and by tick 12,020 lane 0 stood at (1641, 22, -31), no progress for 10,420 ticks, no pickaxe. Probe 45867561645 (`replan --route` on lane 1's recording at tick 40): from the room's feet (1624, 17, -36) the reachable cells (524 nodes) are the tunnel and a stronghold corridor at y 19 (x 1605 to 1620); the route toward (1609, 23, -38), the bot's own tunnel start, ends at (1609, 19, -38), 4 straight under it: on the way down the bot dropped 4 from its stair into the corridor, which no walk climbs. Fix: the way a leg at a time (`[end_kit] way`, the ledger's legs' ends), the route's reach under a point walked to, then a pillar up to the point's height on held blocks (`arrival_rise_to`, Q7's shaft with a stop) or stairs up toward it.
