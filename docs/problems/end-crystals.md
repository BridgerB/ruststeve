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
