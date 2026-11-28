---
title: Inertia before the move
description: Issue 56's decision that a tick's horizontal inertia is the ground the body stood on before it moved, as vanilla's LivingEntity.travelInAir reads it, not where it ended as clojurecraft's physics does; why the oracle is overruled here, and how the differential test holds the rest bit for bit.
type: decision
tags: [decision, physics, oracle, clojurecraft, jump, sprint]
aliases: [inertia before the move, pre-move inertia, landed inertia, travelInAir inertia]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@97525c3 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/physics/src/lib.rs#clojurecraft read it after the move; decision inertia-before-the-move."
  - "ruststeve:crates/physics/tests/differential.rs#fn every_step_from_clojurecraft_state_agrees() {"
  - "clojurecraft:src/clojurecraft/physics.clj#inertia (cond in-lava? lava-inertia"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[physics]]"
  - "[[sprint]]"
  - "[[clojure-is-the-oracle]]"
  - "[[cc-physics]]"
---

# Inertia before the move

## The decision
Issue 56 (lead-56, 2026-11-28): on land, the horizontal velocity a tick keeps is the ground's 0.546 when the body stood on the ground **before** it moved, else the air's 0.91. Vanilla's `LivingEntity.travelInAir` reads `onGround()` for the friction (`f = onGround ? blockFriction : 1.0`, `f1 = f × 0.91`) before `handleRelativeFrictionAndCalculateMovement` moves the body. clojurecraft (`physics.clj:214-217`, `landed? ground-inertia`) and ruststeve until 56 read it **after** the move. The acceleration already read the ground before the move, so only the inertia changed.

## Why
The real server is the reference; clojurecraft is an oracle ([[clojure-is-the-oracle]]) and it is wrong here. The two rules differ on two ticks of every jump. On the jump tick, vanilla keeps the jump's horizontal speed at 0.546 where the old rule kept it at 0.91. On the landing tick, vanilla keeps 0.91 where the old rule kept 0.546. The product over a hop is the same, but the speed the jump adds lasts longer under the old rule.

The sprint-jump showed it ([[sprint]]): `a_sprint_jump_is_pushed_along_the_yaw` measured 8.756 blocks a second (CI 46118649784). Worked by hand over the 12-tick flat cycle (a jump tick at 0.1274 plus the 0.2 push, then 11 air ticks at 0.02548 and 0.91):
- the old rule gives a steady speed of 0.2023 a tick at the jump, 5.253 blocks a cycle: **8.756 b/s**, CI's figure to four digits;
- vanilla's gives 0.2848 at the jump, 4.276 a cycle: **7.127 b/s**, the known vanilla sprint-jump.

Walking jumps and steps off a ledge carried the same error, hidden from the parity table because a moment compared only the floored cell.

## What was rejected
- Keeping clojurecraft's rule and setting the test's bound to 8.76. A sprint-jump row would then fail against the real server, and every predicted jump would run too far.
- Dropping the differential scenarios that diverge. The test now checks every tick two ways (`crates/physics/tests/differential.rs`):
  - **`every_tick_equals_clojurecraft`** runs each scenario as a chain, bit for bit. The scenarios where the rules part are listed by name (`DIVERGING`); their ticks are compared up to the first parting. The test asserts that each listed scenario does part, and that no other one does, so the list cannot rot. A walk from a fixture's standing start parts on its first tick, because that start has no downward velocity and does not land.
  - **`every_step_from_clojurecraft_state_agrees`** steps every tick once from clojurecraft's own state before it. It must match bit for bit, except on a parting tick. There the horizontal velocity must be clojurecraft's times the ratio of the two inertias, and nothing else may differ.

## What would change it
A real-server trace of a jump that settles at 0.91 on its take-off tick. The parity rows that jump (p02-sprint-jump, the m-rows' `jump:` scripts) are where it would show.
