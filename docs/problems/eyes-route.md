# Issue 19: a safer route home through the Nether (agent Q9)

The orchestrator's index is [`eyes.md`](eyes.md); the trip's own walls are [`eyes-home.md`](eyes-home.md) (Q2, Q8). This file is agent Q9's: which way from the warped forest to the entry portal meets the least lava and the fewest open drops, measured from the frontier world's own region files, and the `[home] way` that follows it.

## Needs
Real bots on the trip home (from the forest near (495, 44, 20) to the entry portal at (22, 34, -20)) die mostly knocked into lava by magma cubes on the one-wide spans over the deltas' lava sea (x 200 to 400, z about 194, the spans at y 36 to 40 over the sea's top at y 31; eyes-home walls 12, 20, 22, 23, 27) and at the fortress stretch (x 60 to 120). Bridger's ruling allows steering by known geography: config way points, each with its source.

## Done (world terms)
A `[home] way` whose route, read from the frontier world, has the fewest cells of open lava within 2 of the feet and the fewest open drops of the candidates, and a race from the frontier on it that gets bots further than the current way.

## Risks
- One world: the frontier snapshot's (race 45621953332 lane 7); a route tuned to it.
- A route in rock costs pickaxe uses (about 2.4 a block); the frontier bot holds about 295.
- The bot sees only exposed faces (no X-ray): the route is chosen from the region files, but the bot's own legs still search what it has seen; hidden lava inside rock is met by the staircase's refusals.

## The tool
`results region-route <region dir> --from x,y,z --to x,y,z --box x0,y0,z0,x1,y1,z1 [--avoid x,z,r] [--map] --cand 'name;miny;maxy;dig,bridge,lava,drop,low;radius;x,z ...'` (`crates/results/src/route.rs`): the world read into block classes, each candidate the cheapest chain of the travel tunnel's moves (a level step, a stair up, a stair down; a cut cell never beside lava; fortress blocks and spawners never cut; columns within `--avoid` refused) under its weights in tenths of a step (a cell cut, a block bridged, each open lava cell within 2 on the plane from 4 under the feet to the head, each open drop of 2 or more beside the feet, a step under y 40), kept within `radius` of a polyline when one is given. It reports the steps, the blocks cut, the bridges (over lava or air), the open lava cells within 2, the steps beside an open drop, the biomes, the nearest fortress column and avoided point, way points every 24 steps and a plan map.

## The route table (probes 45761861773 and 45762715583, the lane-7 frontier world)
From the forest at (496, 44, 18) to the portal's tunnel cell (22, 35, -18), the blaze spawner (130, 61, 167) refused within 24. "Lava falls" are steps with a side whose fall ends in lava (the spans at z 194 are 5 to 9 over the sea at y 31); "open lava" counts the exposed lava cells within 2 on the plane, from 4 under the feet to the head; "drops" are steps beside an open drop of 2 or more; "sky" steps have four open cells over the head. The blocks cut are the survey's own (the bot's covered tunnel keeps 2 of rock over the head and cuts more: Q6 measured 1.7 uses a block on a tunnel leg).

| candidate | weights (tenths of a step) | steps | cut | bridged | open lava | lava falls | drops | sky | under y 40 | nearest fortress block |
|---|---|---|---|---|---|---|---|---|---|---|
| now: a 6-wide corridor round the old `[home] way` (z 194, the x 60 fortress bridge) | dig 30, bridge 30, lava 3, drop 3 | 907 | 14 | 102 | 38 | **434** | 513 | 634 | 228 | 1 |
| nowsafe: 24-wide corridor round the old way, safe weights | safe (below), no fortress clearance | 1001 | 701 | 23 | 7 | 5 | 94 | 361 | 9 | 1 |
| **thrifty16** (taken) | dig 40, bridge 40, lava 20, drop 10, lavadrop 100, low 20, open 5, fortress 16 | 799 | 171 | 92 | 20 | **11** | 252 | 535 | 9 | 26 |
| safe16 | dig 24, bridge 60, lava 40, drop 15, lavadrop 200, low 30, open 10, fortress 16 | 725 | 541 | 21 | 6 | 1 | 106 | 253 | 9 | 24 |
| safe24 | safe, fortress 24 | 717 | 550 | 22 | 7 | 2 | 112 | 239 | 9 | 24 |
| band75 (feet y 32 to 75) | thrifty's | 773 | 373 | 124 | 14 | 2 | 235 | 357 | 9 | 16 |
| mid16 | dig 30, bridge 40, lava 30, drop 25, lavadrop 150, low 20, open 15, fortress 16 | 729 | 567 | 25 | 11 | 2 | 71 | 201 | 9 | 19 |
| sky16 | dig 30, bridge 40, lava 20, drop 20, lavadrop 150, low 20, open 40, fortress 16 | 711 | 659 | 31 | 13 | 2 | 74 | 94 | 9 | 17 |
| tunnel16 (feet y 40 to 90) | dig 12, bridge 80, lava 60, drop 30, lavadrop 300, low 30, open 30, fortress 16 | 695 (not reached) | 1041 | 16 | 0 | 2 | 31 | 46 | 0 | 21 |

What the world holds (the plan maps):
- The lava sea is open from y 70 down nearly everywhere from x -48 to 527, z -160 to 287; dry columns are the deltas' basalt (x 160 to 470, z 0 to 140), the soul sand valley (x 30 to 160, z 40 to 130, and x 0 to 60 round the portal), and the crimson forest by the portal.
- **Three fortresses**: the known one (x 60 to 150, z 128 to 250, the spawner at (130, 61, 167)), a second as large at x 340 to 480, z -160 to -20 (north of the forest), and a third along x 480 to 527, z 70 to 160. The candidates with no clearance pass within 1 of fortress blocks; every route north of the forest must keep off the second.
- The straight line (z about 0) runs into the second fortress; the cheap ways keep south of it.
- thrifty16 climbs from the forest to y 52 to 98 over the deltas at z 21 to 80, crosses the soul sand valley's top at y 80 to 89 (x 139 to 41), and comes down the bot's own old tunnel along x 24 (y 40, z 54 to -12: no cut there) to the portal.

## What is built (branch issue-19-q9-route)
- `results region-route` (`crates/results/src/route.rs`, `Chunk::cells` in region.rs): the survey above.
- `crates/plan/src/covered_way.rs` and `[home] covered`: each leg of the trip a `tunnel_toward_covered` to thrifty16's next feet cell (25 points from (481, 52, 21) to (104, 89, 74)), out of the last leg's reach of 120; `[home] way` the same points on the plane then the tail by the portal. Lines outside the file: `pub mod covered_way` in lib.rs; `HomeRules::covered`, its default, the new default `way`, `HomeState::covered` and the one call in `home_trip` (after Q11's, before Q6's), each fenced `Q9`, in home.rs.

## Walls
1. **Covered legs to high points are slow and cost too many pickaxe uses** (race 45764756002 at 6332c24, 3 lanes from the frontier, read at tick 24,000 from the evidence uploads). No bot took a hit on the covered way: lanes 0 and 1 stayed at 19.8 the whole time. But neither got far.
   - Lane 0 spent 101 pickaxe uses on the first 33 blocks: from (513, 45, 30) at tick 7,180 to point 0, (481, 52, 21), passed at feet (481, 48, 21) at tick 8,900.
   - Point 1, (466, 72, 28), is 34 up. The covered tunnel's routes to it ran 161 to 182 steps for a 15-block leg, by way of y 78, and wandered back east to (517, 49, 32). The tunnel gave up ("no nearer than 18 in 17 stretches"), and after three "no tunnel route" the point was skipped at tick 13,460.
   - Point 2 went the same way. Meanwhile the heading's fallback walked the bot down to y 36 to 40 by the sea at x 457 to 465 and bridged 6 blocks: the edge the route was meant to avoid. Lane 0 then stood at (457, 36, 41) for 2,540 ticks.
   - Lane 1 did the same, then took a 147-step covered route to (443, 98, 49) that climbed by way of x 515 to 519. At tick 23,840 it was at (459, 79, 68), 38 off the point, with 125 uses left after 4 pickaxe runs: about 60 blocks west in 16,400 ticks.
   - Cause: the thrifty16 points follow the deltas' cliff tops (y 72 to 98). From the forest at y 44 the covered tunnel climbs 30 to 50 in rock under `COVER_COST` and `HIGH_COST`, and lava pockets in the basalt keep blocking its steps ("lava beside", 19 cells blocked).
   - Next: points the tunnel can reach from where the bot is. That means band75's route (feet y 44 to 75, the same 2 falls to lava as safe16, 373 cut), rises of at most `tunnel::RISE` 16 between points, and the plain `tunnel_toward` (no cover cost) wherever the survey shows a walk with no fall to lava. Also: a skipped point must not hand the leg to the heading's walk by the sea. Hold, or go on to the next point.
2. **Held by a provoked enderman from the top-up's end** (the same race, lane 2): at tick 6,040 the top-up ended on its clock ("12 pearls of 14"), and in the same tick "provoked enderman 4: into a sealed pocket". The bot was then "held from enderman 4" in a 0-block pocket in the forest at (513, 45, 30) to tick 23,440 and on, at 19.8 health. It never began the covered way. This is not the covered way's wall; it is the top-up's hand-over (Q2/Q8).

## The second iteration (the coordinator, 2026-11-25)
`[home] covered` is band75's line now (feet y 44 to 75, at most 16 of rise between points, 2 steps of 773 beside a fall to lava), each point `[x, y, z, cover]`: the plain `tunnel_toward` on every leg but the one the survey puts beside a fall to lava (step 32 at (474, 56, 20), the leg to (461, 64, 19)), which keeps cover. With no route the leg holds (a fail counted at most every 200 ticks); after 3 the point is skipped and the next tried in the same tick, never the heading's walk.
3. **band75: no damage, no deaths, about 6 blocks west per 1,000 ticks** (race 45770564199 at 1f22a0b, 3 lanes from the frontier, read at tick 28,700 from the evidence uploads). All three lanes stayed at 19.8 health on the way, with no lava touch.
   - Lanes 0 and 2 were past point 4 and lane 1 past point 5, at x 377 to 400 (lanes 0 and 2 were held at point 5, 19 to 24 off on the plane).
   - From the covered way's start (ticks 7,960 to 8,100, x 511 to 513): lane 0 made 136 blocks west (6.6 per 1,000 ticks), lane 1 111 (5.4), lane 2 122 (5.9).
   - Pickaxe uses, gross (held at the start plus every pickaxe made, less held now): about 590 in lane 0 (4.3 a block west), about 600 in lane 2 (4.9), and 430 to 560 in lane 1 (3.9 to 5.1).
   - Points 2 to 5 on the plain tunnel went at 11 to 25 blocks per 1,000 ticks.
   - The time went in two places:
     - (a) The one covered leg, to (461, 64, 19): 7,700 to 9,000 ticks for about 20 blocks, with one "no route ... held".
     - (b) Pickaxe runs. Q8's "home: pickaxes on the way" ran 4 or 5 times a lane. Each of the first three walked back to the forest's placed table at about (494, 41, -7), 35 to 50 blocks east, though a table was carried ("1 tables"). That cost 800 to 900 ticks a run and the tunnel retraced its way after ("tunnel off its route at (494, 41, -9)"). The fourth run, at x 400, placed the carried table and took 65 to 75 ticks.
   - Next:
     - Make the trip's pickaxe runs place the carried table where the bot stands, not walk back to a known one (Q8's `home_trip`, or base's table step).
     - Make the covered leg plain too: the survey's one fall-to-lava step there is one cell, at (474, 56, 20).
     - At about 6 blocks per 1,000 ticks, the 380 blocks left to the last leg's reach take about 60,000 more ticks. With the walk-backs gone, the plain legs' 11 to 25 a 1,000 would be 15,000 to 35,000.
4. **The covered leg made plain** (the coordinator, 2026-11-26): every point's `cover` is 0. The survey's one fall to lava on that leg is the single cell beside (474, 56, 20), which the trip's guard (eyes-home wall 28) covers.
