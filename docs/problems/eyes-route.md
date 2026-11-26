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
