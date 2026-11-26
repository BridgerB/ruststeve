# Issue 19: the way home's fortress stretch (agent Q11)

The orchestrator's index is [`eyes.md`](eyes.md); the way home is Q2's and Q8's ([`eyes-home.md`](eyes-home.md)); the last leg within 120 of the portal is Q6's ([`eyes-portal.md`](eyes-portal.md)). This file is agent Q11's: the stretch from the blaze spawner's detour at about (100, 45, 205) past the fortress (its bridge at x 60, z 109 to 175) to within `[portal_way] reach` 120 of the entry portal (22, 34, -20), where Q6's last leg takes over. Code: `crates/plan/src/fortress_way.rs`, called from fenced `Q11` blocks in `home.rs` and `heading.rs`.

## Needs
A bot on the way home with 14 eyes at about (100, 45, 205), the fortress west and south of it (wither skeletons on its bridges and in its corridors, blazes at the spawner near (130, 61, 167)), ghasts over the soul sand valley, reaches a point within 120 on the plane of the portal alive.

## Done (world terms)
3 of 4 development bots from `dev-nether-fortress` (the frontier bot at (100.5, 45, 205.5) with 14 eyes, 128 netherrack, a table, sticks and 8 stems; race `start=dev-nether-fortress snapshot=frontier`, goal race, revive false, cap 24,000) within 120 of the portal alive, on a commit green in CI. Development knowledge, never evidence.

## Risks
- No healing: food 10 to 11 and nothing to eat; every hit is kept, and fire has no cure in the Nether.
- Wither skeletons (the wither effect), blazes (fire), ghasts over the valley, piglins.
- A tunnel into the fortress's bricks opens its corridors (eyes-home wall 26); bricks are a wall to the trip's tunnels since Q8's 9db467f.
- One world: one fortress, one route.

## Walls
1. **The guard clicked into the body's own cell** (race 45760537530 at 62ea711, 4 lanes x 1 bot from `dev-nether-fortress`). Lanes 0 to 2 each stood 3,000 to 4,000 ticks on one cell, "home: guard laid at" every tick with the filler unchanged (guards 221 to 411): the server refuses a block inside the player's box. Lane 0 at x 98.8 (box to 99.1) guarded (99, 73, 182); lane 1 at x 81.7 guarded (82, 80, 162), and two blaze fireballs, a ghast's and their fire took it from 19.8 to 2.4 standing there; lane 2 at z 168.1 on soul sand (box from 167.8) guarded (104, 69, 167). Fix: `home::guard_wanted` never picks a cell the body's box overlaps (`fortress_way::body_in_cell`), the rail under it still when missing; the skip is noted ("fortress way: no guard in the body's own cell").
2. **A fortress turn at the first point** (the same race, all three lanes at tick 60 to 80): the first way point (100, 205) reached, the heading turned south ("a fortress seen ahead (9300 nether bricks)", lane 2 "a ticking WitherSkeleton 33 off") in the one tick before the next point was aimed at, walked 48 south into ghast and blaze fire and back. The way crosses the fortress by design. Fix: in Home with a point aimed at, no fortress turn (`fortress_way::home_way_steers`; "fortress way: no fortress turn on the way home").
