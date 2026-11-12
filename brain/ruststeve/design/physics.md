---
title: Physics
description: How ruststeve moves its own body - a bit-exact port of clojurecraft's physics step (collision, step-up, gravity and drag, fluids, fall distance), the world's solid and fluid oracles, when the world steps it, and the movement packets it sends as vanilla does.
type: reference
tags: [physics, movement, collision, water, packets, core]
aliases: [physics crate, step, Body, Controls, move_player_pos_rot, movement packets, differential test, server corrections]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@ff7281f clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/physics/src/lib.rs#pub fn step(solid: &impl Solid, fluid: &impl FluidAt, body: &Body, controls: &Controls) -> Body {"
  - "ruststeve:crates/physics/src/lib.rs#pub fn land_tick(solid: &impl Solid, pos: Vec3, [vx, vy, vz]: Vec3) -> (Vec3, Vec3, bool, bool) {"
  - "ruststeve:crates/physics/tests/differential.rs#fn every_tick_equals_clojurecraft() {"
  - "ruststeve:crates/world/src/lib.rs#pub fn solid(&self, x: i32, y: i32, z: i32) -> Option<f64> {"
  - "ruststeve:crates/world/src/lib.rs#fn tick(&mut self) -> Vec<Effect> {"
  - "ruststeve:crates/bot/src/lib.rs#pub fn walk_controls(tick: u64) -> physics::Controls {"
  - "clojurecraft:src/clojurecraft/physics.clj#siblings' ports of the client): no buoyancy to speak of, a swim-up of 0.04 a tick while jumping,"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[cc-physics]]"
  - "[[mc-water]]"
  - "[[world-value]]"
  - "[[recordings]]"
---

# Physics

The server does not move the player: the client simulates its own movement each tick and reports where it is. The `physics` crate (core, no I/O) is clojurecraft's `physics.clj` in Rust ([[cc-physics]]), function for function and constant for constant.

## How it works
- `step(solid, fluid, &Body, &Controls) -> Body` is pure. `Body` holds position, velocity, on-ground, against-a-wall, jump cooldown, fall distance and the in-water, in-lava and eye-in-water flags; `Controls` are forward, jump, sneak and yaw.
- The world is read through two oracles, as in clojurecraft. `World::solid` is a full cube (1.0), a flat box's top (a carpet's 0.0625, from the generated collision table, [[datagen]]) or nothing; **an unloaded cell is a full cube**, so the body never falls out of the world. `World::fluid` is water or lava with its level (the state's offset from the block's first state).
- **When it runs**: on every `Event::Tick`, once a teleport has placed the body and its column is loaded. A teleport places the body and zeroes its velocity; a dimension change clears it.
- **What it sends** (clojurecraft's `movement-packets`): `move_player_pos_rot` when the position or look changed, otherwise `move_player_status_only` every 20 ticks. Flags: bit 0 on the ground, bit 1 against a wall.
- **What to press** arrives as `Event::Controls`, recorded like every event ([[recordings]]), so `replay verify` covers physics. Today `bot --walk square` sends a scripted staircase walk; the planner replaces it (issue 09).

## Evidence
- **Differential test** (`crates/physics/tests/differential.rs`): 13 scenarios (flat walk, wall, jump onto a block, carpet, ledge fall, still water, swim up, bank impulse, lava, sneak in water, turning, jump cooldown), 752 ticks, compared with clojurecraft's own `step` run once outside this repository. **Every field is bit-equal**, including the yaw's `sin`/`cos`.
- **Live**: a 30 s staircase walk on a local 26.1.2 server: 34.5 blocks from the start, **0 server corrections** (teleports after the first), `replay verify` 0 mismatches over 5,858 events.

## Gotchas
- **Vanilla's collision tolerance** (`physics::COLLISION_EPSILON`, 1e-7, from `VoxelShape.collideX`): a face within 1e-7 of the box's own counts as ahead, and the other axes must overlap by more than 1e-7. clojurecraft's port had none, so a box whose edge rounded a hair past a face (-2.3 + 0.3 = -1.9999999999999998) walked into the block and the server corrected it; the server model's 1,000 generated worlds found it ([[server-model]]). The differential against clojurecraft stays bit-equal: its scenarios never sit on that edge.
- A server correction is a `player_position` after the first: the server's verdict that the client moved wrongly. Counting them is the live test.
- The world uses the controls' yaw for the look it sends; the pitch is the server's.
- The walk can die on a normal-difficulty night (no survival yet, issue 12): `xtask server --difficulty peaceful` gives a quiet local server.

## Limits
- Collision shapes are default-state only: a slab or stair in another state collides as its default shape, as in clojurecraft. Routes avoid them.

## See also
- [[cc-physics]] - the original.
- [[mc-water]] - what the fluid rules mean for a bot.
