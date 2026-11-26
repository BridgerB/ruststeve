---
title: The End's crystals
description: How the bot breaks the End's crystals (plan::spikes, issue 20f) - an end stone pillar beside each spike, a guard ring at the top, the stand 3 off the crystal's column behind two blocks at 2 off, the strike by reach alone, cages' bars dug and walled, the pillar dug back down; climbs only with the dragon far or perched and health over danger plus 6.
type: reference
tags: [plan, end, crystals, pillar, strike, dragon]
aliases: [spikes, Spike, StandPlan, spikes_task, spikes_step, SpikeState, pillar and strike, end crystal, guard ring, bars_shield, rise_jump, rise_click]
status: draft
lastUpdated: 2026-11-26
verifiedAgainst: ruststeve@34ec399 clojurecraft@be85a92
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
`Spike { centre, radius, top_y, caged }`: `top_y` is the bedrock's y (obsidian below it, the crystal at `top_y + 1`). It comes from `config/end.toml` (`SpikeState::geography`, set by the shell) or, when none matches, from the blocks seen round the crystal (`spike_for`: its bedrock, its top layer followed out along +x, bars two out).

`stand(spike, axis)`, pure: the pillar's column is the first column out from the top layer (cells with dx² + dz² <= r² + 1), `STAND_OFF` (3) at the nearest; the stand is 3 out at the bedrock's height; the walls are the cells 2 out at the feet's and the eye's height. A radius-2 spike's stand is its pillar's top; a wider one's is on the spike's top, walked onto from the pillar. `approach_along` adds the pillar's foot (the column's ground, seen) and refuses a column with something in the way; `best_approach` takes the foot nearest the feet.

## The steps
`next` reads the world each call (a reload resumes): falling, wait; at the stand, the guard ring (each side that drops gets a block, a ledge under it first), then the walls (a bar in a wall cell is dug and end stone put in: bars' thin collision lets the blast through), then the strike; in the column under the stand, the pillar up; on a wide spike's pillar top, the walk onto the stand; on a spike's top with nothing to do, back to a recorded pillar's top; on a pillar, the dig of the cell under the feet; else the walk to the best foot.

`Plan::spikes_task` (called by `end_task` while a crystal is alive) applies the rules and starts the step: a `Task::Explore` walk, a `Task::Dig`, or a driver step (`SpikeState::doing`: climb, block, strike, hold) run by `Plan::spikes_step` while no task runs, from `step_inner` after the arrival's shaft. The climb is the arrival shaft's rise (`arrival::rise_jump`, `arrival::rise_click`, lifted out of `shaft_tick` unchanged): centred, end stone in hand, jump, the block into the cell the feet left at the top of the jump, looking straight down. Pillars and walls are end stone only (`dragon_immune`); each pillar is recorded with its base so the way down digs exactly what was put.

## The rules
- A climb (each jump) only with the dragon more than `CLIMB_FAR` (64) off or perched (phases 5 to 7) and health over danger + `CLIMB_MARGIN` (6).
- The dragon strafing (1) or charging (8) within `THREAT_NEAR` (40): up high in a whole guard ring, hold crouched; on an unguarded pillar, dig down.
- No strike at or under danger health. A strike attacks every `SWING_EVERY` ticks while the crystal's box (2 by 2) is within 3.0 of the eye.
- A pillar with no rise in `PILLAR_STALL` (1,200) ticks fails, says why, and its approach is refused for `REFUSE_TICKS`.

## Ledger
Every path writes a `spikes: ...` note: pillar up from / up to / stalled / boxed / paused / down at, walk to, no way to, Guard/Ledge/Wall block at and in at, bar dug at, strike N, crystal gone N, no strike under danger health, no climb, hold in the guard ring, dig down from the dragon, wait, no crystal left. Repeats within `NOTE_EVERY` are not written twice.

## Tests
`crates/plan/tests/spikes.rs`, on hand-built End floors with a spike and its crystal: the stand 3 out behind a wall (radius 2 and 4, from the blocks and from the geography); the whole sequence walk, pillar, ledge, guard, wall, strike, dig down, a wide spike's walk on and back; a stump with a gap; falling waits; the climb's jump and block; hurt on the ground waits; the dragon's rules; a stalled pillar; the strike and the crystal gone; no strike under danger; a cage's bars dug and walled.
