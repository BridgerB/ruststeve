# Issue 20f: the crystals by pillar and strike (agent E4)

**Status: racing from dev-end-island (2026-11-26).** Code: `crates/plan/src/spikes.rs`; tests: `crates/plan/tests/spikes.rs`; brain: `brain/ruststeve/design/end-crystals.md`. Index: [`end.md`](end.md).

## Needs
From the island (stage `island`), every end crystal destroyed with the bot alive and on the ground: stage `crystals`. Nothing alive heals the dragon then (it heals 2 a second from the nearest crystal within 32, E1).

## Done (world terms)
A case file with stage `crystals`: the ledger shows a `spikes: crystal gone N` for each crystal, `spikes: pillar up to` and `spikes: pillar down at` in pairs, and no death.

## The design (Bridger's choice, clojurecraft's `dragon.clj`)
- **The stand:** 3 cells out from the crystal's column along an axis, feet level with the bedrock. The crystal's box (2 by 2) is then 2.0 from the eye, inside the 3.0 reach.
- **The wall:** end stone in the two cells 2 out, at the feet's and the eye's height. e03: the strike from 3 off through it lost 0.42 (exposure 0). The explosion sits at the crystal's feet (bedrock + 1). Every ray to the 1.8-high body crosses the wall's plane under bedrock + 2 and within the stand's own column, so the wall shuts them all. That matters because, fully seen at 3 off, the blast takes about 56 (E1's formula).
- **The attack:** 26.1 judges it by `isWithinAttackRange` alone, with no line-of-sight check (E1, jar), so the strike past the wall is legal.
- **The pillar:** end stone only, the only placeable `dragon_immune` block the bot carries (E1: END_STONE, OBSIDIAN, IRON_BARS, BEDROCK). It goes in the first column out from the spike's top layer, 3 out at the nearest, and is recorded with its base so the way down digs exactly it.
- **The guard ring:** at the stand, every open side at the feet's height not over the spike's obsidian or a recorded pillar gets a block, with a ledge under it first (a ledge alone is no guard; CI run 45865201752 caught the first version taking a ledge for one).
- **Cages:** the bars (x and z c±2, y top to top+3) sit in the wall's own cells. Parity e03 (E6, run 45864998071, Normal, full iron with boots) struck through the intact bars and lost 0.42, the blast's least: a bar on the ring's edge is a full-width pane facing the stand. So the bars are the wall and the strike goes through them (`SpikeState::bars_shield`, true). The fallback, with it false, digs each wall cell's bar (no drop fetched) and puts end stone in.
- **One blast, one wall (e03):** the blast leaves both wall cells air, end stone or bars. The wall is read from the world before every strike, so a crystal still standing is walled again first. With no wall, e03's bot died through full iron.
- **Down:** the cell under the feet dug, one at a time, with no jump. The drops come back as end stone.

## Rules
- A climb (checked before each jump) needs the dragon more than 64 off or perched (phases 5 to 7), and health over danger + 6.
- If the dragon strafes (1) or charges (8) within 64 (40 until wall 1): inside a whole guard ring, hold crouched. On an unguarded pillar, dig down.
- Never a strike at or under danger health.
- A pillar that rises no block in 1,200 ticks fails, the note says why, and that approach is refused for 1,200 ticks.

## What it needs from others
- **E3:** `end_task` calls `Plan::spikes_task` while a crystal is alive. The driver `Plan::spikes_step` is already called from `step_inner` (fenced E4). End stone in stock: the plan's 260-odd blocks of pillar per run, recovered on the way down.
- **E1 or the shell:** `config/end.toml`'s `[[spikes]]` rows (`centre = [x, z]`, `radius`, `top_y` = the bedrock's y, `caged`), loaded into `plan.spikes.geography`. Without them, the spike is read from the blocks seen round the crystal (its bedrock, its top layer, bars 2 out).

## Risks, unverified
- Dragon knock-back at the stand with a whole guard ring: not measured.
- Each crystal's dig down loses nothing, but a pillar the dragon cannot break still stands if the bot dies on it. The record is then lost to a respawn: the stump is end stone on end stone and is no longer known as a pillar.
- The walk from a wide spike's pillar top onto the spike (1 to 2 cells at height 76 to 103) relies on the pathfinder. Untested real.
- Endermen: every look goes through `Plan::step`'s `keep_look`; the strike looks at the crystal, everything else looks down.

## First race: 45871173293 (dev-end-island, 2 lanes, cap 12,000, commit 415d61c, development only)
Read from the lanes' evidence at about tick 4,700 (11:10 AM Denver):
- **Both bots broke 2 crystals each:** the caged (42, 0) and (12, 39).
  - Caged: struck through the intact bars, as e03 says.
  - Each strike lost 1.0 (`player_explosion`); the guard ring and wall took 6 to 8 blocks.
  - Pillars rose 34 blocks in 150 ticks and 15 in 155.
  - The dig down was a block every 40 ticks, back to the pillar's base, noted.
- **Notes counted** (lane 0): `pillar up to` 2, `pillar down at` 1 (the second still digging), `strike` 2, `crystal gone` 2, about 18 `dig down from the dragon` while it strafed.

## Walls
### Wall 1: held on an unguarded pillar while the dragon strafed from 62 (fixed)
- **What happened** (race 45871173293 lane 0, ticks 1,020 to 1,120): the climb paused at (42, 69, -3) with the dragon 62 off in phase 1 (strafe). The bot held there, then a fireball took 6 (`indirect_magic`) and it fell 8 blocks for 7.5 more (health 8.3). The bunker healed it, and it came back.
- **The cause:** strafing within 40 was the only "threat". A strafing dragon fires from up to 64 (`DragonStrafePlayerPhase`), and a hold was never re-checked.
- **The fix:**
  - `THREAT_NEAR` is now `CLIMB_FAR` (64).
  - A hold on an unguarded pillar ends the tick the dragon strafes or charges near ("spikes: hold ended by the dragon"), and the planner digs down.
  - Unit test `a_hold_on_an_unguarded_pillar_ends_when_the_dragon_strafes_within_64`.
- **The second guard** (Bridger: a taller spike's fall would kill): the water clutch, proven by parity e06 (a 21-block fall onto water poured while falling cost 0, with none 18).
  - In the End, falling with more than 4 under the feet and a water bucket held: the bucket to hand, looking down, used every tick from 3 over the ground until it pours.
  - Landed, the water is taken back.
  - Notes "spikes: clutch begun / poured / missed / over"; unit test `a_fall_from_a_pillar_is_caught_with_the_water_bucket_and_the_water_taken_back`. Not yet counted in a race.

### Wall 2: struck off a tall pillar by the circling dragon, and the fall killed (both bots of race 45871173293)
- **Lane 0:** crystal 3 broke at (33, 24), from the stand on the spike's top at y 94. 280 ticks into the dig down, at y 93, the dragon hit twice (`mob_attack`, 20 to 12.8), and the bot died of the fall (`death.fell.assist ender_dragon`, tick 6,000).
- **Lane 1:** it held at (28, 85, 24), 9 under the goal, with the dragon 61 to 63 off in phase 0 or 1 (`no climb`). One blow took 10 and another 5, then it fell to its death (tick 6,260).
- **Totals:** 5 crystals in 2 runs (3 and 2). Every strike cost 1.0. No death was a blast.
- **The cause:** the dragon's holding pattern flies past the spikes' tops (its path nodes sit over them), so a bot high on a pillar is in its way whatever the phase. The blows themselves were survivable (10 and 5 from 20). The falls of 25 to 33 were not.
- **The fixes:**
  - The water clutch (7c08206) catches the fall.
  - The way down no longer waits to collect each block's drop (the next commit): the drop falls into the cell the body falls into, so the bot spends less time up high.
  - Both are to be counted in race 2.
- **The structural answer** (the orchestrator, from e03's reach-not-sight; commit after 7f2d5fa):
  - **The box:** the stand is closed in end stone, every side at the feet's and the eye's height plus a roof, the blast wall part of it. That is 9 blocks, 12 on a one-wide pillar with its ledges, and is noted "stand closed at … (N blocks)".
  - **The climb:** it begins only with the dragon far or perched, and once begun it goes on to the box, with no waits half way ("climb on through").
  - **Out of the box:** only with the dragon far or perched ("hold in the box").
  - A blow inside the box is noted ("blow in the box").

## Second race: 45875049490 (dev-end-island, 2 lanes, cap 12,000, commit 7c08206, development only)
- **The clutch fired once and worked.** Lane 1 was struck off at y 77 and fell 19.6 (health 14). It poured 1.3 over the ground, landed at 16 health with no fall damage, and took the water back. Noted "clutch begun / poured / over".
- **No crystal broke.** Both bots died of `indirectMagic` from the dragon, a breath cloud or fireball, on the ground:
  - Lane 0 at tick 1,080 held still at its pillar's foot (42, 59, 3), "hold in the guard ring" while the dragon strafed 23 off, and took 6 three times.
  - Lane 1 at tick 2,780 was on E3's walk back to the bunker, an enderman provoked as well.
- The holds on a half-built pillar and the dig from the top ("dig down from the dragon" at y 82) are the b15c51b logic, which f8cc3da replaces (climb on through, the box, hold in the box).

### Wall 3: a breath cloud at the feet, held in (fixed)
- **The cause:** no step of the crystals looked at the dragon's breath, so a hold on the ground stood in a cloud.
- **The fix:** 8311848 walked out of a breath at the feet. E5's global rule (`dragon::Plan::breath_step`, bb3b247), run ahead of every task in the End, now does the same for every agent, so the local version is gone. Nothing box- or pillar-specific was needed.
- **Not mine:** lane 1's death on E3's bunker walk (`end: back to the bunker`, through the breath), reported to the orchestrator.

## Third race: 45880674581 (dev-end-island, 2 lanes, be02c32, development only)
- **No crystal broke; both bots died at their first stand** (the caged (42, 0), y 82).
- Each built the walls (the bars), the three other sides at the feet's and eye's height, and the ledges: 9 blocks. Then "nothing holds" the roof, 617 times. The roof has no face of its own to go against.
- **While there, the dragon struck through the open-topped box:**
  - Lane 1: wings 5 and 5, then the head 10.
  - Lane 0: two wing hits, then two breaths of 6.
- The box held both bodies on the pillar: no fall.
- **Fixed in 6805ab7:** a block over one side goes first, then the roof. E6's sim (`end_loop`) failed the same way and is the check.

### Wall 4: blows land inside the box
- **What we know:** the dragon's wings and head hurt through the walls (the jar's part boxes; no line-of-sight test), so the box keeps the body on the pillar but not out of harm.
- **The open question:** whether a roofed box keeps the breath off. A cloud's box is about 0.5 high, so one on the roof should miss a body 1.8 high under it, but that is not yet measured.
- **The rule now:**
  - "hold in the box" while the dragon is near and not perched;
  - a blow inside the box ("blow in the box") within 200 ticks ends that hold, and the bot goes down at once, the clutch under it.
- **Race 4 counts both.**

### Wall 5: the box stranded a wide spike's bot (E6's sim, CI runs 45880795371 and 45881998970; fixed)
- **What happened:**
  - It pillared to (38, 76, 0), walked onto the spike to its stand at (39, 76, 0) and walled (40, 76..77, 0).
  - It then put a box side at (38, 76, 0), the cell over its own pillar's top.
  - From then on its way back read "stranded on a spike's top", with no strike to the cap.
- **The fix:** at the stand, the blast's two blocks and then the strike at once: no other block first. A box's blows were not kept out anyway (wall 4), and each block was more time up high. The box (`box_gap`) is kept as a function only, for a later hold up high. The note is "stand walled at".

## Fourth race: 45885217441 (dev-end-island, 2 lanes, 5c16f26, development only)
- **5 crystals struck in 2 runs (3 and 2).** "stand walled" ×5, each strike 1.0.
- **Lane 0 lived to the cap** with 3 crystals.
- **Lane 1 died at tick 5,680**, struck by the charging dragon (10, then 5) while digging down 2 over the ground at (12, 63, 42). The blow flung it to (-92, 101, 56), 48 over the ground.
  - The clutch began there ("clutch begun", water bucket in hand) but never poured, and the fall killed it.
  - A fall that fast crosses the pour's reach (the eye within 4.5 of the ground) in about one tick, so a miss is possible; no "clutch missed" was written because the landing killed it first. Watch for it.

### Wall 6: the way down left the body beside its shaft (fixed)
- **What happened** (lane 0, tick 4,060):
  - It walked from the wide spike's stand back to its pillar's top (28, 94, 24), off the column's centre, and dug (28, 93, 24).
  - The body stayed up, held by the spike's edge beside the hole, neither on the pillar nor on the spike.
  - It looked for the next crystal from up there ("no way to", "no approach"). Then, on the spike's top, it was "stranded ... no pillar down" from 5,680 to the cap: the filter wanted the pillar's top whole.
- **The fix:**
  - The dig down centres over the column first ("centre over the pillar").
  - A body over a recorded pillar's dug shaft steps into it ("step into the shaft").
  - From a spike's top, a dug shaft is walked to beside it and stepped into.
  - Unit test `the_way_down_centres_first_and_steps_into_a_dug_shaft`.

### Wall 7: the clutch missed a fast fall, and a charge caught the way down (both answered, the orchestrator's design)
- **The pour is predicted** (`pour_now`). The position this tick's movement sends is the feet plus the velocity (the move comes before gravity 0.08 and drag 0.98). The pour goes in the last tick that position has the eye within 4.4 of the ground's top face with the feet still over it; if two ticks qualify, the later. It is not waited for.
  - Under 2.78 a tick (falls from rest up to about 100 blocks) one tick always lands in that band.
  - At terminal speed (3.9 a tick) a fall's phase can skip the band; the unit test's 48-block fall at terminal speed does not.
  - Unit test `a_48_block_fall_at_terminal_speed_pours_once_in_the_last_tick_in_reach`, plus every start from rest from 5 to 60 over the ground.
- **A charge** (phase 8) within 40, with the bot on its pillar digging down, climbing or stepping: it holds still in the column ("hold in the shaft from the charge"). The climb's driver stops for it too ("climb held from the charge").

## Fifth race: 45890149072 (dev-end-island, 2 lanes, df44b87, development only)
- **3 crystals in 2 runs (1 and 2).** The clutch fired once and poured. The other new notes fired too: "hold in the shaft from the charge", "climb held from the charge", "centre over the pillar".
- **Lane 1 died at tick 8,940 in E5's breath walk.** "out of the breath to [8, 58, -35]" names the feet's own cell 480 times while the cloud grew (radius 4.9 to 5.4); `indirectMagic`. That is E5's, reported.

### Wall 8: the clutch never ran while another row held the planner (fixed)
- **What happened** (lane 0, ticks 1,240 to 1,580): provoked endermen put the planner "into a sealed pocket" (`ender_threat`, ahead of `spikes_step`) on the pillar at y 80. The charging dragon struck it (5) and knocked it off. It fell 22 with the water bucket held and no clutch ("death.fell.assist").
- **The fix:** `Plan::spikes_clutch` has its own call site in `step_inner`, ahead of every row (E5's breath step included), fenced E4.
- **And its cause:** up a pillar or a spike, `ender_threat` is held off by a fenced E4 guard (`Plan::spikes_up_high`, noted "spikes: up high, no enderman pocket"). No enderman reaches a one-wide column's top, and the crystals' work goes on: the strike, or the way down. Unit test `up_a_pillar_a_provoked_enderman_gets_no_pocket`.

### Wall 9: a walk from a cell with no floor (E3's race 45890445738; fixed on my side)
- **What happened:** after E3's bunker opened, the body stood at x 14.2, half on the stair's step and over the bunker's air. Every spikes walk from the feet's cell (14, 59, 0) was refused ("no way to", every approach) from tick 10,000 to the cap.
- **The fix:**
  - E3 walks up the bunker's stair before the hand-off.
  - On my side, a body held up by the floor beside the feet's cell first steps onto that cell ("step onto the floor at").
  - Unit test `a_body_held_up_by_the_floor_beside_steps_onto_it_first`.

## Sixth race: 45895161187 (dev-end-island, 2 lanes, a102cfa, development only)
- **7 crystals struck in 2 runs (4 and 3), the best yet.**
- **The clutch** began 5 times and poured 4. Lane 1 lived through two falls of 11.6 and 4.8 on it, the water taken back each time.
- **Other counts:** "up high, no enderman pocket" 73, "hold in the shaft from the charge" 13, "centre over the pillar" 3.
- **Lane 1 died at tick 8,200 of a blow** (`mob_attack`, health 9 to 0) as it went down from (33, 24)'s pillar; the clutch had just begun. The blow itself, not the fall.
- **Lane 0 lived to the cap**, but spent ticks 10,980 to 12,000 walking to a diagonal "beside" cell of its pillar's shaft. That is wall 10.

### Wall 10: the cell beside a dug shaft was a diagonal one (fixed)
- **The cause:** E5's breath walk took the body off along the spike's top, and the way back to the shaft picked the cell beside it by the signs of both axes, (-9, 97, -41) for the shaft at (-8, -40). From there the shaft was not "next to" (two off), so the walk was asked again, 900 times.
- **The fix:** the cells beside the shaft along an axis with a floor and room, the nearest walked to, then the step in. Unit test case added.
- **Lane 1's death** (the orchestrator's answer): on the way down under danger plus one blow (14), with the dragon within 64 and not perched, the bot holds in its shaft ("hold in the shaft, hurt"). It eats there when food is short ("eat in the shaft") and goes down once the dragon is far or perched. Unit test `hurt_on_the_way_down_with_the_dragon_near_the_bot_holds_in_its_shaft`.

## Seventh race: 45899557907 (dev-end-island, 2 lanes, 1fafc15 with issue-20-end c1135d7, development only)
- **6 crystals struck (3 and 3).** Both bots died of falls after the dragon: lane 0 at tick 6,780, lane 1 at 4,860.
- **New notes:** "hold in the shaft, hurt" once; the clutch began 4 times and poured 3.

### Wall 11: E5's walk out of a breath on a pillar's top (fixed)
- **What happened** (lane 1, ticks 4,800 to 4,860): a breath landed on its pillar's top at (-30, 83, -25). E5's `breath_step`, ahead of every task, looked for a cell out of it ("NoPath" every tick) while the breath took 6, and the bot fell.
- **The fix:**
  - A fenced E4 guard in `breath_step`: up a pillar or a spike it stands aside.
  - The crystals dig down out of the breath at once ("dig down out of the breath"), ahead of every hold, and a hold ends when a breath comes.
  - Unit test `a_breath_on_a_pillar_top_is_dug_down_out_of`.

### Wall 12: a clutch with no pour (open)
- **What happened** (lane 0, tick 6,680): digging down at y 88, health 18, the dragon struck twice (10 and 5, to 3), and the eat row cut the dig in that same tick.
  - The clutch began 31.6 over the ground, but no pour was noted.
  - The fall at 3 health killed it 100 ticks later.
- **Not known:** whether the bucket was in the hotbar, and whether the eat in progress held the hand.
- **Now noted:** the clutch's begin note carries the held item and the bucket's slot, and each pour sent is noted ("clutch pour sent"). The next race reads why.
- **Made structural** (the orchestrator; the clutch is the one rule that must be certain: every death in races 4 to 7 was a fall after a blow):
  - Once the clutch starts it owns the hand. The use key is let go (`ReleaseUse`), any eating task is dropped ("clutch stops the eating"), the bucket's slot is selected, and the pour goes on its predicted tick.
  - Up a pillar or a spike, the water bucket is kept in the hotbar (index 8) by a hotbar swap click ("water bucket to the hotbar"), so selecting it is one packet.
  - Unit tests `a_clutch_that_begins_while_eating_still_pours` and `up_a_pillar_the_water_bucket_is_kept_in_the_hotbar`.

### Wall 13: shut in at the bunker's stair, every approach refused (the frontier race 45898928805 lane 1; fixed)
- **What happened:** at tick 9,940 a provoked enderman's pocket put blocks on E3's bunker stair: (12, 60, 0), the next step, and (13, 61, 0), over the first. After the bunker opened, the bot stood at (13, 59, 0).
  - E3's walk up the stair failed.
  - Every spikes walk to (42, 0)'s four pillar feet was refused ("no way to ... refused"), then "no approach" from 10,260 to 22,000+.
- **The fix:**
  - With every approach refused but one there in the geometry, the bot digs its way out toward the nearest foot along its larger axis ("dig the way out at").
  - A stair of up to three steps when the foot is higher: the cell over the head, the step's cell and the one over it. Otherwise the way along. Never obsidian or bedrock.
  - The refusals are cleared after each dig so the walks are tried again.
  - Unit test `shut_in_a_pit_with_every_approach_refused_the_bot_digs_its_way_out`, from a pit three deep.

## Eighth race: 45903987747 (dev-end-island, 2 lanes, 6b08484, development only)
- **4 crystals struck (3 and 1).**
- **The clutch:** begun 5 times, a pour sent 3 times, poured 3 of 3 sent; "clutch stops the eating" never needed. The bucket was in hotbar slot 44 every time (the hotbar rule).
- **Both deaths were falls after double blows** (20 to 6, and 10 then 5). Each bot was flung sideways off near the island's edge: the cases end at (65.5, 37.7, -20.3) and (30.5, 34.5, -99.5), "over air", far below the island's top.

### Wall 14: a sideways fling lands on another column than the fall began over (fixed in part)
- **The cause:** the pour was judged against the ground under the current position. A body flung sideways at speed lands on another column (here the island's sloped underside past its rim), and no tick qualified.
- **The fix:**
  - The pour is judged against the ground under the predicted positions (this tick's sent position, then the next, with the horizontal velocity and air drag 0.91, `pour_now_at`).
  - A clutch also begins over no known ground at a real fall's speed (over 0.5 a tick): a fling over the void may come over ground.
  - Unit test `a_body_flung_sideways_pours_on_the_column_it_will_land_on`.
- **Open:** a body that strikes the island's side, not its top, has no top face to pour on. The structural answer is fewer blows up high; the clutch cannot save that one.

### The waterfall descent (the orchestrator's design, after races 4 to 8: every death a blow up high and a fall; the dig down was the most time up there)
- **The way down:** at a pillar's top with the crystal gone, holding two water buckets (one kept for the clutch) and 48 or more end stone, the bot pours water on the pillar's top where it stands ("waterfall poured at").
  - It steps off a side that drops ("waterfall down from") and rides the falling water to the ground; water resets the fall's distance (E1, e06).
  - Landed at the pillar's base it notes "waterfall down to"; the pillar is left standing. Under 48 end stone the pillar is dug back for its blocks, as before.
- **The pool:** within 8 of E3's bunker, on the ground, two end stone cells side by side under the feet's level, walled round, are dug ("water pool dug at"). Both buckets go in ("water pool poured at"), making an infinite source ("water pool at"). An empty bucket is refilled there ("refilled at"), walked to from within 64 ("to the pool at").
- **The climb rule:** to begin a climb the dragon must also not be closing (nearer by a block than its reading within 60 ticks), unless perched ("the dragon closing").
- **Unit tests:** `the_waterfall_is_poured_on_the_pillars_top_and_ridden_down`, `a_pool_is_made_by_the_bunker_and_refills_the_buckets`.
- **Not yet counted in a race.**

## Ninth race: 45908299055 (dev-end-island with two water buckets and a bucket, 1a70f53, development only)
- **1 crystal each, and both bots fell dead early** (blows while digging down).
- **No waterfall and no clutch: neither bot held any water.**

### Wall 15: the pool took both buckets and the bot walked off (fixed)
- **What happened** (lane 0, tick 800):
  - Both buckets went into the pool at (10, 60, -7) and (11, 60, -7) ("the pool's pour done" twice).
  - The next tick, before the second cell's update was seen, the pool was not yet "made", and the refill waited on "made". The bot walked to (42, 0) with three empty buckets.
  - It dug down from (29, 91, -25) and was struck off with no water.
- **The fix:**
  - With both buckets poured and the pool not yet seen whole, the bot waits by it ("waiting on the pool").
  - A refill no longer waits on "made": any pool cell with water refills an empty bucket.
- **The invariant** (the orchestrator): no climb begins without a full water bucket in the hotbar ("no climb: no water").
  - A bucket in the main slots is moved to the hotbar first.
  - With none, the pool refills one; with no pool, no pillar is climbed.
  - A pool is begun only with both its buckets in hand.
  - Unit test `no_climb_begins_without_a_water_bucket_in_the_hotbar`. E6's sim crystal runs are given a water bucket (fenced E4 in `crates/bot/tests/end_loop.rs`).

## Tenth race: 45912100302 (dev-end-island, e67f786, development only), read at tick ~4,700

### Wall 16: the escape out of water took the bot off its waterfall (fixed)
- **What happened** (lane 1, ticks 1,220 to 1,340):
  - After the caged (42, 0) it poured the waterfall at (39, 82, 0) and stepped off (-1, 0).
  - Mid-ride, at y 77, the always-row "escape-water (trapped in water)" walked it to dry land: (41, 82, 0), the spike's top.
  - There the pillar's watered top read as no way down: "stranded on a spike's top" from 1,340 to the cap.
- **The fix:**
  - While a waterfall is ridden (in its water, not on the ground) the crystals hold the controls ahead of every row ("riding the waterfall"; the same call site as the clutch).
  - A pillar's top holding water counts as a way back.
  - Unit test `the_waterfall_ride_is_kept_and_its_pour_is_no_bar_to_the_way_back`.
- **No pool in race 10, read from the ledger:** lane 1's ledger has no pool note at all. The only chance was tick 680, at the bunker stair's top (11, 60, -2), and the pool was looked for only beside the feet.
  - **The fix:** every refusal is now noted ("no pool: buckets", "no pool: no site" with each axis's first reason, "no pool: no way to its site").
  - The site is looked for within 3 columns (`pool_site`) and walked to ("to the pool site at").
  - Unit test `no_pool_site_says_why`.

## Eleventh race: 45916169323 (dev-end-island, 966d029, development only)
- **Lane 1:**
  - The pool was made at (9, 61, 1) and (8, 61, 1) ("water pool at", 3 refills). The bunker's own floor had refused it first, which "no pool: no site" said; the site search found it 1 off at the stair's top.
  - Then the caged crystal, the waterfall poured, and the ride. The ride went 82 to 72 in 400 ticks in the falling water, the eye under all the way, and the bot drowned (`death.attack.drown`, tick 1,980).
- **Lane 0:** 1 crystal, then a fall after a blow (the clutch began).

### Wall 17: a waterfall too slow to ride (fixed: a landing pool instead)
- **The cause:** water's drag holds a body to about 0.03 blocks a tick, and a 23-block ride outlasts the air.
- **The fix:**
  - Before the climb, at the pillar's foot, the landing water is poured in the cell beside it with clear air up past the top ("landing water poured at"; two buckets held, one stays for the clutch).
  - At the top after the strike, the bot steps off that side and falls straight into it ("waterfall down from"; a fall into water does no harm).
  - Landed ("waterfall down to"), the water is taken back ("landing water taken back at").
  - The pour on the pillar's top is gone.
  - Unit test `the_landing_water_is_poured_at_the_foot_and_dropped_into_from_the_top`.

## Twelfth race: 45920382795 (dev-end-island, c61e98a, development only)
- **Lane 0:** 2 crystals.
  - The pool was made, but only one bucket came back from it: "refilled" once, then "the refill gave up".
  - With one bucket there was no landing water, so the way down was the dig. The clutch saved one fall of 18.6.
  - The second clutch sent its pour at 2.9 over the ground, and the bot died of the fall (`death.fell.assist`): no "poured" was seen before the landing killed it.
- **Lane 1:** killed by endermen during E5's "dig a notch" (E5's).

### Wall 18: a 2 by 1 pool is no infinite source (fixed)
- **The cause:** a water source regenerates only with two sources beside it. In a 2 by 1 pit each cell has one, so the first bucket taken empties a cell for good.
- **The fix:**
  - The pool is a 2 by 2 square, dug whole ("water pool dug at" ×4). Water goes in two diagonal corners ("water pool poured at"), which fills the other two, and every cell then has two sources beside it.
  - It refills from any cell of the square.
  - "no landing water" is now noted with its reason (buckets, end stone, or no clear side).

### Wall 19: a dig-out that never got out (the frontier race 45918666403 lane 2; fixed)
- **What happened:** after a breath put the bot in a notch 2 under its bunker, the hand-off came from (14, 56, 0). The dig-out toward (39, 59, 0) dug a three-step stair, eight cells from tick 10,920 to 12,260, rising only to the foot's height, 59, still under the ground at 62. The bot never moved, every walk was refused, and it sat 19,560 ticks with no pillar and no strike.
- **The fix:**
  - The stair rises to the higher of the target's height and the ground over the feet (or over the next column).
  - With its steps open the bot walks up to the top step, and the dig-out goes on from there.
  - The progress invariant: a dig-out no nearer its target in 600 ticks fails ("dig-out: no progress at …", with the next cell and the distance) and hands back for 1,200 ticks.
  - Unit test `a_dig_out_climbs_to_the_ground_and_fails_when_it_gets_no_nearer`.

## Thirteenth race: 45924374354 (dev-end-island, ed132b8, development only)
- **Lane 0:** 3 crystals.
  - The 2 by 2 pool was made ("water pool at (9, 61, 1) and (8, 61, 2)"). One refill came back, and the second "gave up" again, aimed at the cell the first had just emptied.
  - With one bucket, "no landing water" (noted 7 times with its reason) and the dig down.
  - The clutch saved one fall (21.8). The bot died on a second fall, its pour sent.
- **Lane 1:** died in E5's breath walk ("a notch down where the feet are").
- **The refill:** each try now takes the next cell of the square in turn, within the pour's reach (4.4). A give-up notes the held item, the buckets' slots and the point aimed at.
- **The pour a tick earlier** (the orchestrator: two sent pours did not save a bot, and the server places the water when it handles the packet, perhaps after the feet arrive):
  - The pour goes in the tick whose sent position is in reach while the one two ticks on is not, so it is one tick before the last in reach when two qualify. Only one is sent (`Clutch::sent_at`): a second use would take the water back up.
  - "clutch pour sent" now carries the tick and the predicted ticks to landing.

## Fourteenth race: 45927433568 (dev-end-island, b4f48ee, development only)
- **Lane 0:** 2 crystals.
  - Landing water was poured at (38, 59, 0) for the (39, 0) pillar, but the next choice was the South axis, (42, 59, 3), with one bucket left: "no landing water", and the poured bucket lost.
  - The clutch saved a fall of 19.6 (sent 2 ticks before the landing).
  - A second clutch sent its pour 3 ticks before the landing (3.31 over, falling 1.16) at 10 health, and the bot died of the fall.
- **Lane 1:** the refills worked at the fourth turn (2 buckets) and landing water was poured at (43, 59, 3). Later it was killed by breath in E5's notch.
- **The give-up notes** say the bucket was held and aimed at a pool cell's middle with no fill: some cells were not sources yet.

### Wall 20: the pour sent too late, and the landing water left behind (fixed)
- **The pour:** sent in the first tick whose sent position is in reach, for the most time for the server.
- **The approach:** an approach whose column already has its landing water goes first, so a poured bucket is not lost to another axis.

### The clutch's window, measured (E6's parity e09-pour, run 45929976619; docs/problems/end-model.md "The water clutch window")
- **The rule now:**
  - The server judges a use from the last position it heard (the body's before this tick's move), and places water only with the eye within 4.5 of the ground there. 3.93 or less always placed it; 4.56 or more never did.
  - So the pour goes by the current height: the feet 2.7 or less over the ground under the body (`POUR_FEET`).
  - It is tried again every 2 ticks until the water shows under the feet, and never once it is there.
  - A late pour is never too late: K1 saved every drop up to 60.
- **The races fit:** race 45927433568's pour "3.31 over" was the eye at 4.93, out of reach.

## Fifteenth race: 45930464018 (dev-end-island, 17f10c1, old pour timing, development only)
- **Lane 1:** 4 crystals.
  - Two clutch pours were sent with the feet 3.39 and 4.01 over the ground: eyes at 5.0 and 5.6, out of reach (E6's window).
  - "missed", then a death by the fall. 7a0bb59's height rule answers this.

### Wall 21: the pool's water went down the bunker's stair, and the bot drowned in its bunker (fixed)
- **What happened** (lane 0, ticks 880 to 2,400):
  - Both pool pours were "done" with both pool cells still Air.
  - The body stood at x 10.8, off its stand (10, 62, 1)'s centre. The ray to the pool cell's floor middle met the top face of the stand's own floor, so the water went into the stand's cell.
  - It ran down E3's bunker stair and flooded the roofed bunker. The bot drowned there (`death.attack.drown`).
- **The fix:**
  - Before a pool pour, the bot centres on its stand ("centre on the pool's stand").
  - It aims at the far side of the pool cell's floor (0.3 past its middle, away from the stand), so the ray clears the stand's floor edge.
- **Made structural** (the orchestrator: no misplaced pour may ever flood the shelter):
  - The pool's stand and its four cells keep 5 columns from the bunker's column (its stair runs four out), and the stand is never higher than the ground over the bunker, so water cannot run down to it.
  - The site search widens to 8 columns to fit.
  - Unit test `the_pool_is_never_sited_by_the_bunker_or_its_stair`.

## Seventeenth race: 45934621769 (dev-end-island, 69eabcf, development only)
- **0 crystals in either lane.**
  - Lane 0 dug one pool cell, then went back into its bunker 6 off (E3's retreat). From there it tried to dig the next pool cell, refused 25 times, every 380 ticks, to the cap.
  - Lane 1 died of breath going "into the bunker" (E5's).

### Wall 22: the pool worked from out of reach (fixed)
- **The fix:**
  - The pool's digs and pours are made from its stand only. Away from it, the bot walks back ("to the pool site at").
  - With no way back, or after 8 digs for its four cells, the pool is given up ("no pool: given up"), and the crystals go on with the buckets in hand.
  - Unit test `the_pool_is_worked_from_its_stand_or_given_up`.

## Eighteenth race: 45937952400 (dev-end-island, dc7ed65, development only)
- **Both bots alive at the cap, 2 crystals each.**
- Lane 0 made the pool, and after a few give-ups held 2 full buckets. It poured landing water at (38, 59, 0) for the West pillar.
- That approach was then refused ("no way to (39, 59, 0)" 40 times from inside E3's bunker). The South pillar was climbed and dug down: no drop, the bucket lost.
- Lane 1 gave its pool up correctly and poured landing water. One clutch "missed" a fall of 85, and the bot lived at 7.6 health.

### Wall 23: landing water left by a refused approach (fixed)
- **The fix:** when the way goes to another pillar than the one whose landing water is poured (and not yet climbed), the water is taken back first ("landing water taken back at", "back for the landing water at").
- With no way back to it, it is noted and left ("landing water left at"). If it is gone, it is forgotten.
- Unit test `landing_water_left_by_a_refused_approach_is_taken_back`.
- **The refills, by the server's word** (the orchestrator): a refill aims only at a pool cell the server shows as a source (its block state's water level 0, `source`), never one predicted to be.
  - A cell a refill got nothing from waits until the server shows it in another state before it is tried again.
  - A give-up notes the cell's last seen fluid state.

### Fireballs up high (E5's jar read, DragonFireball.onHit: the cloud goes on the first living entity within 4 of the impact, walls or not)
- **No climb begins** with a dragon fireball in flight within 64 ("no climb": "a fireball in flight").
- **Up a pillar when one comes:** the climb stops ("climb stopped by a fireball"), and the bot goes down at once ("down from a fireball"). It drops into the landing water when that is poured and the feet are at the top, else digs.
- Unit test `a_fireball_in_flight_keeps_the_bot_off_its_pillar_and_takes_it_down`.
- **The landing water from another bucket:**
  - The landing pour uses a water bucket other than the clutch's (the hotbar's highest water slot): another hotbar one, else one from the main slots swapped into a hotbar slot that is not the clutch's (`pour_hold`).
  - So the clutch's stays in the hotbar, with no "no climb: no water" pass after the pour (race 45942992471 lane 1 lost 40 ticks to it).

### Wall 24: the dig could not beat the fireball (frontier race 45947478869 lane 1; fixed)
- **What happened:**
  - The climb began at tick 13,160 with the dragon 93 off in phase 0. The strafe began at 13,280 with the bot at y 70; it climbed on through.
  - At 13,320 a fireball 58 off: "climb stopped by a fireball", "down from a fireball", the dig at (39, 73, 0), with no landing water on that pillar ("no side of the foot with a floor and clear air").
  - The cloud came 40 ticks later, at 13,360, with one block dug. E5's dodge found "no dodge cell" up there but did not pre-empt my rule. My rule fired in time; its way down was too slow.
- **The fix:**
  - A strafe within 64 now counts as a fireball for this rule, and stops a climb.
  - With no landing water, the bot drops off a side that falls clear, holding the clutch's bucket in the hotbar ("drop with the clutch from"): a fall of about 2 s that the clutch catches (E6's window, race 16's 5 of 5), where the dig is a block every 40 ticks.
  - The dig stays only for a bot with no bucket in the hotbar.
  - Unit test `a_fireball_in_flight_keeps_the_bot_off_its_pillar_and_takes_it_down`, extended.

## Twenty-first race: 45949063425 (dev-end-island, 4e86a78, development only)
- Both died to breath clouds (indirectMagic): lane 0 at tick 5,680 in E5's strafe notch after a dodge with no path; lane 1 at 2,900 in E5's charge notch. 0 strikes from a pillar in either.

### Wall 25: the landing water flooded the pillar's foot, and no climb rose (fixed)
- **What happened (lane 0):**
  - At 2,440 the landing water went in at (38, 59, 0), beside the foot (39, 59, 0) at the same height; "pillar up from (39, 59, 0)".
  - It ran into the foot's cell: senses at 2,540, 2,740 and 2,940 read "cell 39,59,0 in water". In water the jump does nothing, so no block went in.
  - The climb was held from a charge at 2,900 with the feet still at 59. The second climb (3,240) stalled 1,200 ticks at 59.06, "on the ground false". The third (4,640) was held at the foot by the charge at 4,780. That was three climbs and about 1,900 ticks with no rise. The death cell (45, 57, 0), a notch at another foot, was in landing water too.
- **The fix:** the landing water is poured from the first block up, never from the foot. The climb hands back on its first rise ("climb paused for the landing water at"), the pour follows, then the climb goes on. The water sits beside the pillar's base block, below the feet. Unit test `the_landing_water_is_poured_from_the_first_block_up_and_dropped_into_from_the_top`.
