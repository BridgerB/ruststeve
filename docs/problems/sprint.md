# Problem: every move is a walk (issue 56, sprint)

**Status: measured; the machine is in progress** (S1 measure wrote this file; S2 to S6 append their own sections). The winning lineage of issue 20 took 466,498 ticks from spawn to the credits, almost all of it travel at the walking pace (4.317 b/s). Sprint is 5.612 b/s, and the deaths from moving too slowly (creepers keeping pace with a retreat, wither skeletons, the End's dodges) are the same gap.

## What the bot needs
- A body that sprints exactly as a 26.1.2 client does: the speed, the jump, the rules that start and stop it, and the packet that tells the server, in the right place in the tick.
- A hunger budget that counts what sprinting costs, so sprint never takes the food under the line where it stops working (6) or where the bot must eat.

## Risks
- A sprint the client simulates and the server does not believe: the server never checks the speed (below), so a wrong model shows as no correction and goes unseen. The parity rows are the only check of the body.
- A sprint over an edge or into lava: the policy (S5) refuses it there.
- Food: one point every 40 metres of flat sprint, one every 20 sprint-jumps.

## Done, in world terms
A real bot sprints along a flat floor at 5.612 b/s, sprint-jumps, stops sprinting on a wall it walks into and on food 6, and is never corrected; the same rows in the model give the same outcome classes.

## Measured (S1, probe 46115253148, `javap -c -p -constants` of the 26.1.2 server and client jars)
Every number is in `config/sprint.toml` with its class and method; the prose is `brain/ruststeve/design/sprint.md`.

| What | Value | Source |
|---|---|---|
| Sprint modifier | `minecraft:sprinting`, 0.30000001192092896, `add_multiplied_total` (id 2), transient, on `movement_speed` | `LivingEntity.<clinit>`, `LivingEntity.setSprinting` |
| Sprinting speed | 0.1 × 1.3 = 0.13; steady 0.2806 a tick, 5.612 b/s (walk 0.2159, 4.317) | `Player.createAttributes`, `LivingEntity.getFrictionInfluencedSpeed` |
| Air acceleration | 0.025999999 sprinting, 0.02 walking | `Player.getFlyingSpeed` |
| Sprint-jump | `+(-sin(yaw) × 0.2, 0, cos(yaw) × 0.2)`, `Mth`'s sine table, yaw × 0.017453292f | `LivingEntity.jumpFromGround` |
| Start | sprint key, forward impulse > 1e-5, possible, not using an item that forbids it, not crouching (unless under water), not fall flying (unless under water) | `LocalPlayer.canStartSprinting` |
| Possible | no blindness; food > 6 (or `mayfly`); not in shallow water unless flying | `LocalPlayer.isSprintingPossible`, `Player.isMobilityRestricted`, `FoodData.hasEnoughFood` |
| Stop | not possible, no forward impulse, or a horizontal collision that is not minor (input against collided move under 8°, 0.13962634 rad) | `LocalPlayer.shouldStopRunSprinting`, `isHorizontalCollisionMinor`, `Entity.move` |
| Not a stop | sneaking, using an item, releasing the sprint key | `LocalPlayer.aiStep` |
| Sprint hit | knockback hit: velocity × (0.6, 1, 0.6), sprint off; a sprinting hit is never a critical | `Player.causeExtraKnockback`, `Player.attack`, `canCriticalAttack` |
| When decided | each tick before the body moves (`AbstractClientPlayer.aiStep` comes after), from the last tick's collision | `LocalPlayer.aiStep` |
| Packet | `player_command` id 42: entity varint, action varint, data varint; START 1, STOP 2, data 0 | `ServerboundPlayerCommandPacket.write`, `$Action.$values` |
| Order a tick | `player_input` (on change), `player_command` (on change), the move | `LocalPlayer.tick`, `sendPosition`, `sendIsSprintingIfNeeded` |
| Input bit | sprint 64, kept by the server, never read (only shift is) | `ServerGamePacketListenerImpl.handlePlayerInput` |
| Server truth | shared flag 3, bit 0x08 of entity data index 0 | `Entity.setSprinting` |
| Server checks | none: the command is taken as sent; moves refused only over 100 squared beyond expected per packet, or 0.0625 squared off the server's sweep | `handlePlayerCommand`, `handleMovePlayer` |
| Exhaustion by distance | on the ground horizontally: sprint 0.1 a metre, walk 0, crouch 0; swim, eye in water, in water 0.01 a metre; in the air 0; whole centimetres per move | `ServerPlayer.checkMovementStatistics` |
| Exhaustion by jump | 0.05, sprinting 0.2 (when the client leaves the ground going up) | `ServerPlayer.jumpFromGround`, `handleMovePlayer` |
| Exhaustion to food | over 4.0: one saturation point, else one food point (not on peaceful); capped at 40 | `FoodData.tick`, `FoodData.addExhaustion` |

What this changes in the plan:
- **Hunger** is one point per 40 metres of flat sprint, not "0.1 food per 40 metres". A 1,000-block sprint is 25 points: 12.5 shanks, which saturation pays first.
- **Sneaking is not a stop rule** in the game. The agreed `physics::sprint_allowed` refuses a sneak anyway; that is stricter than the game and safe (the bot never asks for both), but a parity row must not expect a sneak to stop a real sprint.
- **Using an item is not a stop rule** either: eating while sprinting keeps the flag (the input is slowed by the item's use speed). Only the start is refused.
- **A sprint hit ends the sprint** and costs 40% of the speed: a retreat that swings must start the sprint again (S5).
- **The server never corrects a wrong sprint**, so the parity rows (S3) and the jar read are the only evidence the body is right.

Not read: the client's side of a sprint hit (`MultiPlayerGameMode.attack` running `Player.attack` locally), swimming's speed (`Player.travel` in water), and `AttributeInstance.calculateValue` (the standard `value × (1 + amount)` is assumed for `add_multiplied_total`).

## The pace policy (S5, `plan::sprint`, `config/sprint.toml` `[policy]`)
Which moves sprint, as a closed `sprint::Pace` read from the task in hand; the gate (`Plan::pace_gate`) runs last in `Plan::step`.

- **Ask**: walks (every task's walk), explores, retreats and the escape's move hold the sprint key. A retreat from a `never_fight` kind always asks.
- **Food**: a sprint needs a point to spend over `hungry_below` (saturation first), one point per 40 metres (`metres_per_point`). A `never_fight` retreat skips this; the body's floor of 6 still holds.
- **Ground**: no sprint in water (no swim model) or lava, nor toward or along a lava edge, a missing floor, the void's edge, water, awkward footing or an unloaded cell, `look_ahead` (2) blocks along the facing.
- **Path**: bridges, sneaks at an edge and stairs or travel tunnels walk; a fight's close-in walks.
- **Counted**: "sprint begun, <pace>" once a task, "sprint refused, <reason>" once a task for each reason and "sprint restarted, <n>" on each later start (a stop holds `hold_ticks`, doubled per stop up to `max_doublings`, and until `clear_blocks` from where it stopped), in the race summary's notes by kind; `Plan.pace` keeps the run's sprint and refused ticks.

Unverified until a race: the sprint's effect on waypoint following at corners (0.28 a tick against `WAYPOINT_REACH` 0.35), and the first CI and race counts of each note.
