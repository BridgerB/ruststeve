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
- **Fix** (368bf264, merged at d64f327d): `kit_iron_pickaxe = 1` ([survival]). After the meat, `cook_goal_with` smelts held raw iron for the kit's ingots plus three more for each iron pickaxe due (`iron_pickaxes_due`: fewer than `kit_iron_pickaxe` held with `spare_uses` left, from the iron held beyond `kit_ingots` less the shields made, never a trip for iron). The shields come next, then the pickaxe is crafted at the cook's table (`cook-pickaxe`). Digs already take the fastest held pickaxe (`plan::dig::Dig` → `data::dig::best_tool`: iron 6 against stone 4, 8 ticks a stone block against 12, 15 a deepslate against 23, 250 uses against 131; checked, not changed). Notes: "iron pickaxe at the cook site: <step>" and "pickaxe in hand: <kind>" (`Plan::pick_watch`, once per change). Test: `crates/plan/tests/iron_pickaxe.rs`.
- **Counted by**: "pickaxe in hand: iron_pickaxe" lines per bot; "tool broke: stone_pickaxe" on the leg; stairs and dig ticks on the leg; the `iron_kit` stage's tick (an iron pickaxe and a bucket held). Not raced yet.

## 3. Lanes from one snapshot are not independent samples (races 46139640077 and 46144128199)

- **What happened**: on far-fortress's `gear` snapshot (race 46144128199, 2 lanes x 1 bot), the two lanes stood at tick 16,800 in the same cell (26, -39, 45), both at 11 health, their stone pickaxes at 88 and 89 uses, the same task ("site at (16, -53, 36) facing east, no walk to it"); at tick 12,000 and 9,580 they had been on the same stairs a couple of thousand ticks apart. On typecraft's `ready` (race 46139640077) the two lanes split early: lane 1 cast at the y 46 pool, lane 0 lost it and cast at the lava sea (9,600 against 28,640 ticks on the leg).
- **Read** (code at 3fbc7a62): the bot's tick seed is made from the clock (`crates/bot/src/lib.rs`, `Seeds(now_ms() | 1)`) and put on every tick event (`world.seed`), but nothing in `crates/plan` or `crates/world` reads it: the planner is deterministic given the world. race.yml restores the same world into every lane and resets one random sequence per lane (`random reset minecraft:entities/enderman`, the drops); there is no per-lane seed or jitter for the bot. What differs between lanes is the server's own randomness (mob spawns, mob moves, the tick timing) and the network's timing.
- **What it means**: k lanes from one snapshot are k correlated samples of one world, between one sample (when no mob meets the bot) and k (when one does early). A median from 2 x 8 lanes of one snapshot is not a 16-run median.
- **What this issue does**: judges each fix on several snapshots and worlds, and says so: typecraft `ready` (race-45128866973 lane 6), far-fortress `gear` (race-46127598383 lane 2), and the `gear` snapshots of the other seeds that reached it in #69's relay (G3 stalls). Per-lane spread is reported beside each median. Not fixed here: a per-lane bot seed that the planner reads would be a change to every issue's races (the core reads no randomness today, by design).

## The far-fortress read (race 46144128199, 32d56fc3 = the stack's plan code), 2 lanes x 1 bot from `gear`, goal `race`, cap 48,000

| lane | portal leg | nether | what the files say |
|---|---|---|---|
| 1 | own stairs 11,400; cast at (24, -55, 39) south from 18,720, 5,460 ticks, 124 ops, 10 fills, lit 24,180 | 24,620 | first lava fill at 22,040, 3,320 ticks into the cast; 5 stone pickaxes broken in the run |
| 0 | the same stairs and the same site from 18,700 | never (`Cap`) | cast out of lava after 68 ops with **0 fills** at 21,220 (wall 1 again); "no lava to fetch"; then "hunt lava at" 308 times to the cap, flipping between (23, -7, 35) and (66, -55, 13) (wall 5) |

The two lanes played the same game to tick 18,700 (wall 3) and split at the first lava fill at the same site.

## 4. A refill at a station in reach fails in 46 ticks (race 46144128199 lane 0, 21,173 to 21,219; race 46139640077 lane 0, 31,798 to 32,780)

- **What happened**: lane 0's refill trail: "station at (24, -54, 43) h 1 with 12 sources, feet (24, -54, 42)" then "tunnel level toward (22, -55, 43)", four times, 4 ticks apart, then "no route to Some((22, -55, 43)), 4 tunnels". The feet were one block from the station's stand with 12 sources below it; lane 1 at the same site filled its first bucket (22,040). On typecraft lane 0's cast #212: "220 sources, no station / tunnel level toward (-3, -55, -279)" four times in 1,000 ticks, then no route.
- **Cause**: not known. The station step gives up and falls to the tunnel within a tick, and the tunnel step ends at once. Read with `why` at 21,173 on lane 0's recording (the stand, the look, the line to each of the 12 sources) before any change.
- **Wall 1's fix makes this fail sooner, not succeed**: a lava-first fill at this site would run the same ladder before the room.

## 5. The lava hunt flips between two targets (race 46144128199 lane 0, 21,820 to the cap)

- **What happened**: after the site was given up, "hunt lava at (23, -7, 35)" and "hunt lava at (66, -55, 13)" alternated 308 times over 26,000 ticks, each "no site within 48: sources 3 landings 25 refused/spent 100 … beside 4"; the tasks between were stairs and tunnels "no exposed None in sight, at its depth" toward one target and then the other. No `lava at … given up` line: each target's stall count started again each time it came back.
- **Cause (hypothesis)**: the hunt takes the nearest lava seen (`cast::far_lava`) from the feet each tick, with no target held; two targets at near distances swap as the feet move. The aim stalls (`lava_aims`, `LAVA_AIM_STALLS`) count per target and never reach the limit on either.
- **Fix (to build)**: the hunt holds its lava target until it is reached, given up by its stalls, or refused, and a target held counts its stalls across the flips. Notes: "lava target held …" and the existing "lava at … given up".

## 2b. Wall 2's fix gave the cook up (race 46148925674, 6dfd4daf, typecraft `ready`, 1 lane)

- **What happened**: the cook phase began at 1,740 with cook-fuel plank crafts, then "no candidate had a way" to 2,860 (a full pack: "drop Chicken from slot 9 (fewer than 6 main slots free)"; four relocates), and "cooking given up (61 tasks begun; the next step toward cook-fuel was gather)" at 2,880: no meat cooked, no shields, no iron pickaxe. On 201dfcfe the same snapshot cooked all of it (race 46139640077 lane 1, smelts 6,340 to 12,000).
- **Cause (hypothesis)**: the pickaxe's three smelts raise the cook's fuel need past the wood held (3 birch logs, 1 oak log, 1 plank, 25 sticks), so cook-fuel asks for a gather and the gather has no way.
- **Fix (to build, 83-ironpick)**: a pickaxe is due only when the fuel held, less what the meat and the kit's ingots take, covers its smelts; never a fuel gather for it. Note "iron pickaxe at the cook site: skipped, fuel …".
