# Issue 83: the portal reached sooner (lava portal, no diamonds)

The cast itself (water on lava in a mould cut from rock, no diamonds) has existed since issue 14c ([`portal.md`](portal.md), [`cast.md`](cast.md), brain `ruststeve/design/portal-cast.md`). Issue 83 is the check that the cast builds a frame the game accepts (`plan::portal_shape`, measured from the 26.1.2 jar, `crates/plan/tests/portal_shape.rs`) and the `nether` stage reached sooner from the snapshot where the kit is held.

- **Needs**: from the `ready` snapshot (the issue-20 lineage, race-45128866973 lane 6, typecraft; no `iron_kit` snapshot exists in any release), a lava site whose first fill is known to work before its room is cut; tools that cut the descent's stairs and the room at the pace the held iron allows.
- **Risks**: a site given up after thousands of ticks of room digs; a fix that saves ticks on one world only (the far-fortress `gear` snapshot, race-46127598383 lane 2, is the second world); fire safety at the pool.
- **Done, in world terms**: the portal leg (first lava task to the portal lit) shorter on typecraft's `ready` and on far-fortress, pooled over 8-lane pairs against the stack commit, each new path's ledger notes counted.
- **The surface provisioning before the leg** (0 to about 13,000 ticks, "provisions N of 30 pieces: look for animals", cooking, shields) is the food issue's (#78, seeds.md G1), not this one's: it is reported apart and never judges a fix here.

## The read (2026-11-28): race 46139640077 (201dfcfe, the stack), 2 lanes x 1 bot from `ready`, goal `race`, cap 48,000

| lane | provisioning ends | portal leg | nether | what the files say |
|---|---|---|---|---|
| 1 | 13,700 | 13,700 to 23,300 (9,600) | 23,300 | own stairs 13,700 to 14,560; "dig toward the landing" (-29, 47, -252) eight stair tasks to 18,780 (about 45 blocks of tunnel at 70 to 77 ticks a step, 4 stone pickaxes broken in the run); one cast of 4,180 ticks, 121 ops, 10 fills, lit |
| 0 | 12,580 | 12,580 to 41,220 (28,640) | 41,220 | own stairs toward the y 46 pool (-31, 46, -253), dug toward its landings 17,220 to 20,020, then "no site within 48: … footprint 19"; own stairs to the lava sea to 24,980; dug toward landings to 29,540; two casts lost "out of lava" with **0 fills** after 68 and 50 ops (3,240 and 2,800 ticks), each "no lava to fetch … site given up"; the third site lit by 41,220 |

The summary: nether 2/2, median 23,300 (41,220 the other); stairs tasks were 16,640 ticks of lane 0's run.

**Ranked, ticks lost on the portal leg (2 bots; hypotheses until a larger read confirms them):**
1. Casts lost at their first lava fill after their room was cut (lane 0: 6,040 ticks, two sites; cast.md's sealava baseline: 17 of 38 "cast out of lava" at the first cell). Wall 1.
2. Every stair, tunnel and room dig cut with stone pickaxes (no iron pickaxe is held at `ready`; the 8 raw iron go to shields; 4 stone pickaxes broke in each run, 3 and 7 spares made). Wall 2.
3. A site dug toward and then refused (lane 0, about 2,800 ticks at the y 46 pool, `footprint`). Read again on the 8-lane baseline before any change.

## 1. The room cut before the lava is known to fill (race 46139640077 lane 0)

- **What happened**: cast #212 at (-10, -53, -290) worked 68 ops (the room) and stopped at its first `:pour :lava` with "buckets water 2 lava 0 empty 1; uses filled 0"; its refill trail "220 sources, no station / tunnel level toward (-3, -55, -279)" four times, "no route … 4 tunnels"; the fetch found "no scoop routes to a source within 48; site given up". Cast #220 at (6, -54, -283) the same after 50 ops, with a station dig `Uncollected`.
- **Cause (hypothesis)**: a cast fills its lava bucket only when a pour needs it, after every room op before it; a site whose lava no scoop reaches is found out only then.
- **Fix** (121b5c02, merged at 5be93fad): a cast at a newly chosen site (`Cast::fresh`) that holds no lava bucket while a lava pour is still to do fills one by the refill ladder (station, nearest sources, sight digs, tunnels) before its first op. A failed fill fails the cast `NoRefill` at 0 ops; the planner's lava fetch, then its give-up, apply as to any cast out of lava, with no room cut. A cast resumed at a kept site (`Cast::new`, after a fetch or a pause) works as before. Notes: "lava first: filled at <feet> from <source> after N ticks"; "lava first failed: NoRefill at 0 ops" with the refill trail; every ending note's uses end with the lava-first state. Unit tests: `crates/plan/tests/lava_first.rs`.
- **Counted by**: "lava first: filled" and "lava first failed" lines; "cast out of lava after 0 ops" followed by "no lava to fetch"; the N in the filled note; casts lost at the first lava pour after room ops (should fall to zero for fresh sites). Not raced yet.

## 2. Stone pickaxes for the whole leg (race 46139640077, both lanes)

- **What happened**: no `iron_pickaxe` line in either ledger; "tool broke: stone_pickaxe" 4 times in each; lane 1 smelted 3 of its 8 raw iron at the cook site for shields ("shields at the cook site: craft:shield … 3 iron ingots").
- **Cause (hypothesis)**: no row asks for an iron pickaxe after `ready`; the nether kit asks 4 stone ones. Stone with a stone pickaxe is 12 ticks a block, 8 with iron (speed 4 against 6; deepslate 23 against 15); 131 uses against 250.
- **Fix (to build)**: an iron pickaxe made at the cook site from raw iron held beyond the shields' ingots, and dug with first. Notes: "iron pickaxe at the cook site …". Counted: digs and stairs ticks on the leg, pickaxes broken.
