# The portal (issue 14)

## Needs
From the `kit` start (an iron pickaxe, two buckets, flint and steel, a stone pickaxe and sword, a table, sticks, 16 cobblestone; spread 128), within 72,000 server ticks: a water bucket filled at surface water, lava seen with an exposed face (a surface pool, a cave, or the lava sea under y -54), a site beside it, the room cut, ten cells cast (a lava source and a water pour each), the inside opened, and flint and steel struck on the frame.

## Risks
- Lava: refills at a pool, deaths at the frame (both earlier bots' dominant costs).
- Strict no X-ray (Bridger, 2026-11-15): lava hidden in the rock is met only when a dig exposes it; then the site is given up.
- Going down to the lava sea through deepslate is slow.

## Done, in world terms
At least 28 of 56 real bots light a portal within 72,000 ticks of the `kit` start, confirmed by two 8-lane races pooling at least 50%.

## Open
- **Lava unaccounted in the model test** (14c, 12-source pool): nine cells cast and the pool spent, three buckets more than the cells took. A pour that misses (lava left somewhere the cast does not see), a refill sent twice, or the model's flow; not read yet. The test pool was made 18 sources; watch races for `NoRefill` endings.

## Walls
