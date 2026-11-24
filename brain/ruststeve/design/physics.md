---
title: Physics
description: How ruststeve moves its own body - a bit-exact port of clojurecraft's physics step (collision, step-up, gravity and drag, fluids, fall distance), the world's solid and fluid oracles, when the world steps it, and the movement packets it sends as vanilla does.
type: reference
tags: [physics, movement, collision, water, packets, core]
aliases: [physics crate, step, Body, Controls, move_player_pos_rot, movement packets, differential test, server corrections]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@b72980e clojurecraft@be85a92
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
- `step(solid, fluid, &Body, &Controls) -> Body` is pure. `Body` holds position, velocity, on-ground, against-a-wall, jump cooldown, fall distance and the in-water, in-lava and eye-in-water flags; `Controls` are forward, jump, sneak, yaw and an optional pitch. `step_on` is `step` with a third oracle (below); `step` treats every block as plain.
- The world is read through two oracles, as in clojurecraft. `World::solid` is a full cube (1.0), a flat box's top (a carpet's 0.0625, from the generated collision table, [[datagen]]) or nothing; **an unloaded cell is a full cube**, so the body never falls out of the world. `World::fluid` is water or lava with its level (the state's offset from the block's first state).
- **A third oracle, the surface** (issue 16): `World::surface` gives each block's speed and jump factors from the generated table (`data::generated::movement`, [[datagen]]: soul sand 0.4; honey 0.4 and a jump of 0.5) and whether it is water or a bubble column. After each move the horizontal velocity is multiplied by the speed factor of the block the feet are in (a body on soul sand stands inside its cell), or, when that is 1, of the block 0.500001 under the body in the column of its supporting block (`Entity.getBlockSpeedFactor`); water at the feet asks no further. A walk on soul sand settles at 0.098 / (1 - 0.4 × 0.546) a tick, 2.51 blocks a second against 4.32. Friction (ice 0.98, slime 0.8) is in the table and not applied: every block is 0.6.
- **Sneaking** (issue 16): the input is scaled by the `sneaking_speed` attribute (0.3: 1.30 blocks a second) and the move is backed off an edge in steps of 0.05 while nothing is within a step's height (0.6) under the moved box (`Player.maybeBackOffFromEdge`); the velocity is kept. In a liquid sneak also sinks, as before.
- **The keys** (issue 16): on each tick the world sends `player_input` (one byte: forward 1, jump 16, shift 32) when the pressed keys differ from the last sent, before the movement packet, as `LocalPlayer.tick` does; a respawn forgets what was sent. A server learns the bot sneaks only from this: magma spares a sneaking player and its pose crouches. Sprint is never sent and never simulated.
- **When it runs**: on every `Event::Tick`, once a teleport has placed the body and its column is loaded. A teleport places the body and zeroes its velocity; a dimension change clears it, and so does `Act::Respawn` after a death (the body waits for the server's next position).
- **What it sends** (clojurecraft's `movement-packets`): `move_player_pos_rot` when the position or look changed, otherwise `move_player_status_only` every 20 ticks. Flags: bit 0 on the ground, bit 1 against a wall.
- **What to press** arrives as `Event::Controls`, recorded like every event ([[recordings]]), so `replay verify` covers physics. Today `bot --walk square` sends a scripted staircase walk; the planner replaces it (issue 09).

## Evidence
- **Differential test** (`crates/physics/tests/differential.rs`): 13 scenarios (flat walk, wall, jump onto a block, carpet, ledge fall, still water, swim up, bank impulse, lava, sneak in water, turning, jump cooldown), 752 ticks, compared with clojurecraft's own `step` run once outside this repository. **Every field is bit-equal**, including the yaw's `sin`/`cos`.
- **Live**: a 30 s staircase walk on a local 26.1.2 server: 34.5 blocks from the start, **0 server corrections** (teleports after the first), `replay verify` 0 mismatches over 5,858 events.

## Gotchas
- **No ground, still the status**: a body whose column is not loaded runs no physics but still sends `move_player_status_only` once a second; the server re-centres a player's chunks only on a reported move, so a teleport far from the loaded columns otherwise deadlocks (found by the parity table, [[parity-table]]).
- **Vanilla's collision tolerance** (`physics::COLLISION_EPSILON`, 1e-7, from `VoxelShape.collideX`): a face within 1e-7 of the box's own counts as ahead, and the other axes must overlap by more than 1e-7. clojurecraft's port had none, so a box whose edge rounded a hair past a face (-2.3 + 0.3 = -1.9999999999999998) walked into the block and the server corrected it; the server model's 1,000 generated worlds found it ([[server-model]]). The differential against clojurecraft stays bit-equal: its scenarios never sit on that edge.
- A server correction is a `player_position` after the first: the server's verdict that the client moved wrongly. Counting them is the live test.
- The world uses the controls' yaw for the look it sends; the pitch is the controls' when set (kept as the player's pitch from then on, and sent with `use_item` too), else the server's.
- The walk can die on a normal-difficulty night (no survival yet, issue 12): `xtask server --difficulty peaceful` gives a quiet local server.

## Limits
- The body stays 1.8 high and its eye at 1.62 while sneaking (a server's crouching player is 1.5 high with the eye at 1.27); the first sneaking tick is not at full input as a client's is; the supporting block's second try (the box moved back by the last move) is not made. `docs/problems/client-parity.md` has the read.
- Collision shapes are default-state only: a slab or stair in another state collides as its default shape, as in clojurecraft. Routes avoid them.

## Issue 14a
- `Controls.pitch` (`Option<f32>`, `#[serde(default)]`): the look sent with the move when set, for a bucket used along it ([[buckets-and-fluids]]). The physics step ignores it.

## See also
- [[cc-physics]] - the original.
- [[mc-water]] - what the fluid rules mean for a bot.
