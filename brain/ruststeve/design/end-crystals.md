---
title: The End's crystals
description: How the bot breaks the End's crystals (plan::spikes, issue 20f) - an end stone pillar beside each spike, a guard ring at the top, the stand 3 off the crystal's column behind two blocks at 2 off, the strike by reach alone, cages struck through their bars, the pillar dug back down; climbs only with the dragon far or perched and health over danger plus 6.
type: reference
tags: [plan, end, crystals, pillar, strike, dragon]
aliases: [spikes, Spike, StandPlan, spikes_task, spikes_step, SpikeState, pillar and strike, end crystal, guard ring, bars_shield, rise_jump, rise_click]
status: draft
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/spikes.rs#pub fn stand(spike: &Spike, axis: Axis) -> StandPlan {"
  - "ruststeve:crates/plan/src/spikes.rs#pub fn next(w: &World, st: &SpikeState, tick: u64) -> Next {"
  - "ruststeve:crates/plan/src/spikes.rs#pub fn spikes_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/arrival.rs#pub(crate) fn rise_click("
  - "ruststeve:crates/plan/tests/spikes.rs#fn the_planner_walks_pillars_walls_strikes_and_digs_down() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[overworld-arrival]]"
  - "[[digging]]"
---

# The End's crystals

Issue 20f (agent E4, `docs/problems/end-crystals.md`), the port of clojurecraft's `dragon.clj` crystal half. A hit of any kind blows a crystal up, power 6: about 56 damage at 3 off with the body fully seen (E1, jar 26.1.2), so every strike is from behind a wall.

## The geometry
`Spike { centre, radius, top_y, caged }`: `top_y` is the bedrock's y (obsidian below it, the crystal at `top_y + 1`). It comes from `config/end.toml`'s ten `[[spike]]` rows (E1), loaded into `SpikeState::geography` by the shell (`bot::end_spikes`) or, when none matches, from the blocks seen round the crystal (`spike_for`: its bedrock, its top layer followed out along +x, bars two out).

`stand(spike, axis)`, pure: the pillar's column is the first column out from the top layer (cells with dx² + dz² <= r² + 1), `STAND_OFF` (3) at the nearest; the stand is 3 out at the bedrock's height; the walls are the cells 2 out at the feet's and the eye's height. A radius-2 spike's stand is its pillar's top; a wider one's is on the spike's top, walked onto from the pillar. `approach_along` adds the pillar's foot (the column's ground, seen) and refuses a column with something in the way; `best_approach` takes the foot nearest the feet.

## The steps
`next` reads the world each call (a reload resumes): falling, wait; at the stand, the walls first (a cage's bars are the wall: parity e03 struck through them for 0.42; digging them and putting end stone in is the fallback, `bars_shield` false), then the strike at once ("stand walled at"; the box, `box_gap`, every side at the feet's and the eye's height and a roof, is kept for a later hold up high: E6's sim stranded a wide spike's bot behind a box side over its pillar's top, and race 45880674581 took blows through the box anyway); in the column under the stand, the pillar up; on a wide spike's pillar top, the walk onto the stand; on a spike's top with nothing to do, the box's side toward a recorded pillar dug open (`Next::Open`), then back to that pillar's top; on a pillar, the dig of the cell under the feet; else the walk to the best foot.

`Plan::spikes_task` (called by `end_task` while a crystal is alive) applies the rules and starts the step: a `Task::Explore` walk, a `Task::Dig`, or a driver step (`SpikeState::doing`: climb, block, strike, hold) run by `Plan::spikes_step` while no task runs, from `step_inner` after the arrival's shaft. The climb is the arrival shaft's rise (`arrival::rise_jump`, `arrival::rise_click`, lifted out of `shaft_tick` unchanged): centred, end stone in hand, jump, the block into the cell the feet left at the top of the jump, looking straight down. Pillars and walls are end stone only (`dragon_immune`); each pillar is recorded with its base so the way down digs exactly what was put.

## The rules
- A climb begins and goes on only while `Plan::calm` (E3's rule in `end.rs`, shared): no dragon sent, or the dragon holding (phase 0) at least `CALM_FROM` (40) off; and health over danger + `CLIMB_MARGIN` (6) (`climb_refusal`). A perched dragon is no longer a reason to climb: a scan ends in a charge (race 46038776204).
- A refusal mid-climb stops it ("climb stopped at", "climb stopped by the perch / by a fireball") and the bot comes down by water or holds; there is no climb on through. The one exception: within `STAND_NEAR` (5) of the stand the climb finishes in any phase but a landing, sitting or charge within `KNOCK_NEAR` ("climb on, the stand near", `climb_on_near_stand`). A stopped climb's pillar is kept as a stub and resumed from (a side pillar to its top, "on the stub at"), never refused. Out of blocks half way, down.
- The phases in `DESCEND_PHASES` (landing approach, landing, sitting, charge) end a stay up a pillar.
- Out of the box (down, or out toward a wide spike's pillar) only with the dragon far or perched ("hold in the box"). A blow inside the box is noted ("blow in the box").
- No strike at or under danger health. A strike attacks every `SWING_EVERY` ticks while the crystal's box (2 by 2) is within 3.0 of the eye.
- A pillar with no rise in `PILLAR_STALL` (1,200) ticks fails, says why, and its approach is refused for `REFUSE_TICKS`.

## The water clutch
In the End, a fall with more than `CLUTCH_FROM` (4) under the feet and a water bucket held is caught (`Plan::spikes_clutch`, run from `spikes_step` ahead of any task): the bucket to hand, looking straight down, the pour sent on its predicted tick (`pour_now_at`: the eye within `POUR_REACH` (4.4) of the ground under the landing predicted over the whole fall, `landing_ground`), never while rising; landed, the water is taken back only when the bucket count says it was poured, and an empty bucket refills from water the bot left first (a dry stand within `LEFT_WATER_STAND` (3); a failed walk is retried after 200 ticks, dropped after 5). Parity e06 (E6): a 21-block fall onto water poured while falling cost 0, with none 18. Notes: clutch begun, poured, missed, over (the water taken back or left).

## Blocks, the way down, the knock-off (part 2)
Any carried full block builds a pillar (`PILLAR_BLOCKS`, counted together by `pillar_blocks`): the body in the others, the top two (`PILLAR_TOP`) and the stand's walls in end stone, the only dragon-immune one. The way down is the water: the landing water when poured, else the clutch off a side ("clutch down from"), the pillar left standing; the dig only with no water bucket. With the dragon landing, taking off or charging within `KNOCK_NEAR` (30) a bot up a pillar holds crouched: no drop, climb or stand (`knock_risk`).

## Which crystal
The crystals that heal the perch first (`crystals_in_order`, `heals_the_perch`: within 40 on x and z of the perch point, E5's jar read; only (42, 0) and (-42, -1) are outside), nearest first among them, then the rest nearest first; the first with an approach not refused is worked, so a spike whose four approaches are refused gives way to the next. Shut in a pocket (`shut_in`, fewer than `POCKET_CELLS` standable cells reachable), the bot digs out at once rather than walk at each approach in turn. Never from a spike's top: stranded there it goes to the top's edge over ground and off onto the clutch ("stranded on the spike's top"); caged with no edge, the bar toward the island's centre is broken first ("out of the cage at"). Crystals the bow line may still take (`SpikeState::bow_keep`, fenced E8) are left by `next` ([[end-bow]]). `blocks_wanted` is the one count of blocks the chosen approach needs (foot to stand plus `PILLAR_SPARE`), read by E3's stock stop and the climb checks.

## The landing water
With two water buckets (one stays the clutch's) and end stone to spare, the way down is a drop into water poured beside the pillar's foot (`landing_cell`: a floor at the foot's height, clear air to over the top). It is poured from the first block up, never from the foot: the climb hands back on its first rise ("climb paused for the landing water") and the pour follows ("landing water poured at"). The body is centred over the column first ("centre for the landing water at") and the pour aims `LANDING_AIM` past the landing cell's centre: off centre, the ray met the first block's top and the water went into the feet's own cell (frontier race 45954274931). Poured from the foot, it ran into the foot's cell and the bot, in water, could not jump-place (race 45949063425 lane 0). A fireball or a strafe within 64 up high: into the landing water if poured, else off a side onto the clutch, else the dig.

## Ledger
Every path writes a `spikes: ...` note: pillar up from / up to / stalled / boxed / paused / down at, walk to, no way to, Guard/Ledge/Wall block at and in at, bar dug at, strike N, crystal gone N, no strike under danger health, no climb, hold in the guard ring, dig down from the dragon, wait, no crystal left. Repeats within `NOTE_EVERY` are not written twice.

## Tests
`crates/plan/tests/spikes.rs`, on hand-built End floors with a spike and its crystal: the stand 3 out behind a wall (radius 2 and 4, from the blocks and from the geography); the whole sequence walk, pillar, ledge, guard, wall, strike, dig down, a wide spike's walk on and back; a stump with a gap; falling waits; the climb's jump and block; hurt on the ground waits; the dragon's rules; a stalled pillar; the strike and the crystal gone; no strike under danger; a cage struck through its bars; a blast's lost wall built again.
