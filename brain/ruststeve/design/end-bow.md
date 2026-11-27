---
title: The End's crystals by bow
description: How a bot with a bow breaks the End's open crystals from the ground (plan::bow, issue 20's bow line) - the arrow's flight from the 26.1.2 jar, the low arc's pitch by bisection, the hit test on the crystal's box grown by the jar's margin, the blocks along the flight, the stands round a spike, the draw held 24 ticks and released on the heard look; the caged ones left to the pillar.
type: reference
tags: [plan, end, crystals, bow, arrow, ballistics]
aliases: [bow, BowState, bow_task, bow_step, solve_pitch, stand_columns, Shot, the bow line]
status: draft
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@117843e
sourceRefs:
  - "ruststeve:crates/plan/src/bow.rs#pub fn aim(feet: P, crystal: P, blocked: &mut impl FnMut(Cell) -> bool) -> Result<Aim, Miss> {"
  - "ruststeve:crates/plan/src/bow.rs#pub fn solve_pitch(flat: f64, rise: f64) -> Option<f64> {"
  - "ruststeve:crates/plan/src/bow.rs#pub fn bow_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/bow.rs#pub fn bow_step(&mut self, w: &World, tick: u64) -> Option<Controls> {"
  - "ruststeve:crates/plan/tests/bow.rs#fn a_crystal_is_shot_from_the_ground_drawn_full_and_released_on_the_aim() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[end-crystals]]"
  - "[[end-island]]"
---

# The End's crystals by bow

The bow line (agent E8, `docs/problems/end-bow.md`): every real player shoots the open crystals from the ground and climbs only for the two caged ones. This replaces the climb for the eight open spikes; the caged two stay with [[end-crystals]].

## The flight (26.1.2 jar, probe 46039452326)
- Full power after a draw of 20 ticks (`BowItem.getPowerForTime`), speed 3.0, inaccuracy 1.0: each axis of the unit direction plus `triangle(0, 0.0172275)`, a deviation of 0.0070 of the range.
- The arrow starts at the eye less 0.1 and each tick moves by its velocity, then the velocity is times 0.99 and 0.05 comes off its y (`AbstractArrow.tick`).
- It hits an entity whose box, grown by `clamp((age - 2) / 20, 0, 0.3)`, meets the tick's segment. A crystal's box is 2 by 2 by 2 over its position.
- Iron bars are a full plane to an arrow (`CrossCollisionBlock(2, 16, 2, 16, 16)`), so a caged crystal is never shot.

## The aim (pure)
`aim(feet, crystal, blocked)`: refused nearer than 16 (the blast reaches 12) or farther than 48; the yaw to the box's middle; `solve_pitch` scans from 60 down to 80 up for the first pitch whose flight reaches the middle's height at its flat distance, then bisects (the low arc); the flight along the pitch as sent (f32) is followed tick by tick, each segment sampled every 0.1 for a cell `blocked` says stops an arrow (anything but clear air or lava, unloaded included), up to where it meets the grown box. `stand_columns(centre, toward)` gives columns on rings 18 to 40 from the spike, within 90 degrees of the way to the island's middle, that way first.

## The shot (`Plan::bow_task`, `Plan::bow_step`)
- `bow_task` is asked first at E3's two crystal hand-offs (`end_spikes`, and `open_task` before the end stone stock): with a bow and an arrow, on the ground, health over danger, the open crystals nearest first (caged ones noted and left); a shot from here at any, else a walk to the nearest stand with one for the nearest crystal (6 walks, then it is left to the pillar).
- `bow_step` runs in `step_inner` ahead of `spikes_step` while no task runs: the bow to hand (`hold::toward`), still on the ground, `use_item`, the draw held `DRAW_HOLD` (24) ticks, `player_action` 5 once the look the server last heard is the aim within 0.25 degrees (or 60 ticks later), then the flight waited for its predicted ticks plus 20. A gap in the ticks it drove (another row had the controls) lets the draw go and begins again.
- A hit is the crystal gone from the entities and its blast heard within 2 of its spot after the release (10 ticks waited); gone with no blast is "bow: crystal N out of sight after the shot", not a hit (the server untracks and re-sends crystals on a resume).
- Notes: "bow: aim at", "bow: shot at", "bow: crystal N gone by arrow", "bow: shot at crystal N missed", "bow: crystal N left to the pillar" (4 misses), "bow: to the stand at", "bow: draw on crystal N let go".
