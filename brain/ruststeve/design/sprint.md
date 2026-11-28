---
title: Sprint
description: What sprinting is in 26.1.2, read from the server and client jars (issue 56) - the movement_speed modifier, the air acceleration, the sprint-jump boost, the rules that start and stop it, the player_command packet and its place in a tick, the exhaustion it costs, and why the server takes it on trust.
type: reference
tags: [physics, sprint, movement, hunger, packets, jar]
aliases: [sprinting, sprint-jump, START_SPRINTING, STOP_SPRINTING, player_command, sprint exhaustion, config/sprint.toml]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@f2408e5
sourceRefs:
  - "ruststeve:config/sprint.toml#sprint_modifier_amount = 0.30000001192092896"
  - "ruststeve:config/sprint.toml#air_acceleration_sprint = 0.025999999"
  - "ruststeve:config/sprint.toml#sprint_jump_boost = 0.2"
  - "ruststeve:config/sprint.toml#minor_collision_radians = 0.13962633907794952"
  - "ruststeve:config/sprint.toml#order = [\"player_input\", \"player_command\", \"move_player\"]"
  - "ruststeve:config/sprint.toml#sprint_per_metre = 0.1"
  - "ruststeve:crates/physics/src/lib.rs#pub const AIR_ACCELERATION: f64 = 0.02;"
  - "ruststeve:crates/protocol/src/packets.rs#pub const INPUT_SPRINT: u8 = 64;"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[physics]]"
  - "[[server-model]]"
  - "[[food-and-hunting]]"
---

# Sprint

Every number is in `config/sprint.toml`, each with the class and method it came from (probe 46115253148: `javap -c -p -constants` of the 26.1.2 server's inner jar and of the 26.1.2 client jar). This note is the prose; the bot's use of it is `docs/problems/sprint.md`.

## The body
- **Speed**: `LivingEntity.setSprinting` adds the transient modifier `minecraft:sprinting`, 0.3 `add_multiplied_total` (wire id 2), to `movement_speed` (base 0.1): 0.13. On a plain block the ground acceleration is the speed itself (`getFrictionInfluencedSpeed`: `speed × 0.21600002 / friction³`, friction 0.6), times the 0.98 input. Steady on flat ground: walk 0.2159 a tick (4.317 b/s), sprint 0.2806 (5.612 b/s).
- **Air**: `Player.getFlyingSpeed` is 0.026 sprinting (the float 0.025999999), 0.02 walking ([[physics]]'s `AIR_ACCELERATION`).
- **Sprint-jump**: `LivingEntity.jumpFromGround` adds `(-sin(f) × 0.2, 0, cos(f) × 0.2)` with `f = yRot × 0.017453292f`, through `Mth`'s 65536-entry sine table.

## When it starts and stops (`LocalPlayer.aiStep`)
- Decided each tick **before** the body moves, so the move of the same tick already uses it; the collision it reads is the last tick's.
- **Start** (sprint key held, `canStartSprinting`): not sprinting yet, forward impulse over 1e-5, sprinting possible (below), not using an item that forbids it, not crouching unless under water, not fall flying unless under water.
- **Possible** (`isSprintingPossible`): no blindness, food over 6 (`FoodData.hasEnoughFood`; or `mayfly`), not in shallow water (in water with the eye above it).
- **Stop** (`shouldStopRunSprinting`): not possible any more, no forward impulse, or a horizontal collision that is not minor. Minor: the angle between the yaw-turned input and the move after collision is under 8 degrees, so brushing a wall keeps the sprint and walking into one ends it. Swimming has its own rule (`shouldStopSwimSprinting`).
- **Not stop rules**: sneaking and using an item only refuse the start; releasing the sprint key does nothing. The flag holds until a stop rule.
- **The sprint hit**: a hit with knockback (a sprinting hit at strength over 0.9 adds 1) slows the attacker to 0.6 horizontally and ends the sprint (`Player.causeExtraKnockback`); a sprinting hit is never a critical.

## The wire
- `player_command` (play id 42): entity id, action, data, three varints. `START_SPRINTING` 1, `STOP_SPRINTING` 2, data 0.
- Order in `LocalPlayer.tick`: `player_input` when the keys changed, then `sendPosition`, which first sends the command when `isSprinting` differs from the last sent, then the move. The `player_input` sprint bit (64) is kept by the server and never read.
- The server's answer is the shared flag 3, bit 0x08 of entity data index 0.

## The server
- `handlePlayerCommand` sets the flag with no check of food, collision or speed. `handleMovePlayer` never compares a move against the sprint: it refuses only a move over 100 squared beyond the expected velocity per packet, and one its own sweep puts 0.0625 squared from the report. **The server takes the sprint from the client**; a wrong client sprint is no correction, only a cheat.
- **Exhaustion** (`ServerPlayer.checkMovementStatistics`, per accepted move, whole centimetres): on the ground, sprinting costs 0.1 a metre horizontally and walking or crouching nothing; swimming, eye in water and in water 0.01 a metre; in the air nothing. A jump costs 0.05, a sprint-jump 0.2 (`ServerPlayer.jumpFromGround`, called when the client leaves the ground going up). Each 4.0 takes a saturation point, else a food point: a sprint costs one point every 40 metres, a sprint-jump one every 20 jumps.

## Gotchas
- The plan's "0.1 food per 40 metres" is out by ten: it is one point (half a shank) per 40 metres.
- A sprint never stops for a sneak; the agreed `physics::sprint_allowed` refuses sneaking as a policy, which is stricter than the game and safe.

## See also
- [[physics]] - the step the multiplier goes into.
- [[server-model]] - where the sim's exhaustion lives.
- [[food-and-hunting]] - the food the sprint spends.
