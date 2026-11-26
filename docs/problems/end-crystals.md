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
