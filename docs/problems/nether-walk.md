# The Nether walk, by route (issue 16)

Thirteen armed, fed bots stood in the Nether of the clean snapshot's world and none reached the fortress whose bricks they saw about 110 blocks away (`docs/problems/blaze.md`, walls 22 and 23 and the second "Stopped"). This file is the route's side of that wall: what the ground is, why no route was found over it, and each fix with its evidence. The tunnel's side is another agent's.

## The ground, read before any fix

Read with `navmap` (crate `navprobe`, probes 45334641630 and 45334717081) on `rs-l05-b00.rsr` of race 45194253932 lane 5 at tick 25,700, lane 3 of race 45204423376 at tick 50,000 (the same blocks, 329 columns known) and lane 5 of race 45204423376 at ticks 22,200 to 22,710. All coordinates are blocks; "feet y" is the cell the feet stand in.

- **The lava sea's surface is y 31** (lava at y 31 and under, air from y 32). It is one sea: a cavern north and west of the portal (x -8 to 40, z -50 to -22), joined round the north-east (x 40 to 77, z -45 to -34) to a cavern on the east (x 68 to 100 and beyond, z -30 to 100 and beyond).
- **The portal** (obsidian x 21 to 24, y 34 to 37, z -20) opens on a ledge of netherrack at feet y 34, about nine cells by three (x 22 to 30, z -24 to -21), three blocks over the sea on its north side. South of it is a hill: netherrack with magma veins (most of y 29 under the ledge, and bands all through), gravel pockets and soul soil, solid from the sea to above y 48 over x 0 to 66, z -18 to 97. A search from the portal at tick 24,980 spends 42 expansions and its frontier is empty: the ledge is an island for `path::plan`.
- **Lava comes down onto the ledge after the bot arrives.** A lava stream runs down the hill's north face from above y 44 (x 25 to 33, z -23 to -18 at y 36 to 44) and a fall stands at (26, 35 and up, -23). At tick 22,200 the ledge is dry; at 22,560 lava lies at (26 to 28, 34, -23); at 22,700 it is at y 36 over (29 to 32, -23 to -21) and one tick-row later on the bot. By tick 25,700 the row (21 to 25, 34, -19) in front of the portal is lava. The chunks start ticking when a bot comes through, so every bot of this snapshot meets the same flood 700 to 900 ticks in: the four lava deaths at (30, 31, -18) and (20, 34, -18) are this, not only the bot's own stairs.
- **The bricks in view** are a pillar standing in the sea: (61 to 62, 28 to 34 and up, 95 to 98), widening to x 58 to 66 at z 99 to 104. West of it the hill ends in a sheer face of soul soil and soul sand at x 56 to 57 (z 85 to 97), four cells of lava (x 57 to 60) between the face and the pillar. At z 100 a shelf at feet y 34, (55, 34, 100), is two lava cells from bricks at x 58. `at_fortress` is within 6 of a brick, so the shelf, or the face's foot, is enough. 1,927 bricks are on record; the nearest are these.
- **Soul sand and soul soil** line every shore: the hill's east shore (x 62 to 78, z -50 to 100) is soul sand on slopes that drop 2 to 5 a column into the sea; a valley at z 44 to 72, x 15 to 66 has soul sand floors at feet y 32 to 40 round lava pools; the plateau at z 85 to 97 is soul soil from y 31 up. Soul sand is `Awkward` to `path::classify` (neither floor nor clear), so none of it is ground today; soul soil is a full block and is.
- **Magma** is in the hill and under the ledge (y 29 to 30), and on shores at feet y 32 to 35 in places (x 32 to 39, z -42 to -40). It stays a wall for the route.
- **No ground joins the portal to the bricks at or under y 48.** Between them is the hill (tunnel: 115 blocks of netherrack and soul soil at feet y 33 or 34, coming out at the face five blocks from the pillar) or the sea (round by the north-east and down the east cavern: about 200 cells of lava). Over y 48 was not read in this probe.

## What a route needs here

1. Soul sand as a floor, with the feet's cell right for a body standing at y + 0.875.
2. A start that is allowed beside lava: a bot with lava at its door must still be given a way off.
3. A way over lava: a bridge of held blocks, as a move the search can choose and the walk can make.
4. A search that reaches 110 to 250 blocks: 6,000 expansions do not.
