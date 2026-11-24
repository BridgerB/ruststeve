# Problem: the bot's client is not a vanilla client (sneaking, block speed)

**Status: open** (issue 16). Two differences between what the bot's client does and what a vanilla 26.1.2 client does, found by the Nether test bed ([`nether-testbed.md`](nether-testbed.md), "Found on the way"). Both matter on the Nether walk: bridging at an edge over lava, standing on magma, walking soul sand.

## What the bot needs
- When the planner presses sneak, the server must know it (magma does not burn a sneaking player) and the body must behave as a sneaking body does (it stops at an edge, it is slow).
- Its walk over soul sand and honey must be the walk a client makes, not a faster one: a route's cost per block and a walk's timing are otherwise wrong, and a faster walk than the game allows is a cheat the plan forbids.

## Risks
- Every bot sends a new packet. A wrong byte gets every bot kicked: the encoder has a byte test, and the parity rows run it against a real server before any race.
- Sneaking is slower and stops at edges. Any planner path that pressed sneak expecting the old behaviour (full speed, no stop) changes. Read below: there is one such path.
- A slower walk on soul sand makes every soul sand crossing take 1.7 times as long; tasks with tick budgets tuned on the fast walk may now time out.

## Done, in world terms
A real bot that walks a soul sand floor and sneaks to an edge is never corrected by the server, stops on the last block, and crosses magma sneaking with no damage; the same rows in the model give the same outcome classes (the parity table).

## The read (what the code did before, with the game's side from javap)
The game's side is read from the 26.1.2 jars with `javap -c -p` on a runner (probe 45339873719: `Entity`, `LivingEntity`, `Player`, `Input`, `ServerGamePacketListenerImpl`, `MagmaBlock`, `Blocks`, `Attributes` from the server's inner jar; `LocalPlayer`, `ClientInput` from the client jar).

### 1. Sneaking never reached a server
- `physics::Controls::sneak` was read in exactly one place, `physics::step`: in a liquid it sinks the body by 0.04 a tick. On land it did nothing: no slower walk, no stop at an edge.
- `protocol::C2s` had no `PlayerInput`; the generated table had the packet all along (`PlayServerbound::PlayerInput`, id 43). No recording can hold one: a recording's `Send` records are the encoded `C2s` values, and no value encoded to id 43.
- The planner presses sneak in **one** place only, `scoop.rs` (the centring step before a bucket is aimed: "sneaking so no edge is walked off", up to 20 ticks). `defend.rs`, `escape.rs`, `walk.rs` and the stairs always send `sneak: false`. So until now that centring step was an ordinary full-speed walk that could walk off the edge it meant to respect, and nothing else changes.
- The game: `LocalPlayer.tick` sends `ServerboundPlayerInputPacket` whenever the pressed keys differ from the last ones sent, after the body's tick and before its position. The packet is one byte, the `Input` record's flags: forward 1, backward 2, left 4, right 8, jump 16, shift 32, sprint 64. `ServerGamePacketListenerImpl.handlePlayerInput` keeps the input (`setLastClientInput`) and, once the client has loaded, calls `setShiftKeyDown(input.shift())`.
- What the shift key does on the server: `MagmaBlock.stepOn` spares an entity that `isSteppingCarefully`; `Player.updatePlayerPose` gives the crouching pose (a 1.5 high box, the eye at 1.27), from the next player tick.
- What it does on the client (the bot's own physics): the input vector is scaled by the `sneaking_speed` attribute, default 0.3 (`LocalPlayer.modifyInput`); `Player.maybeBackOffFromEdge` cuts the move back in steps of 0.05 while the box, moved, would have nothing within a step's height (0.6) under it; the velocity is kept.
- Sprinting is a different packet (`player_command` START_SPRINTING) and is never sent either; the planner has no sprint and the physics walks at the walking speed, so the two sides agree. Left alone.
- Does a server correct a client that walks at full speed while "sneaking", or runs over soul sand? No. `handleMovePlayer` checks only that the move is not too long (a squared distance over 100, "moved too quickly") and that the body the server sweeps the same way ends within 0.0625 squared of the reported place ("moved wrongly"). It never simulates the client's speed. So the `corrected` counts of old races show nothing about either gap, and cannot: the reference for these two changes is the game's code, tested by unit tests on its numbers; the real server proves the packet (magma) and that the new moves are still accepted (no correction).

### 2. No block speed factors
- `crates/physics` had none. `Entity.move` ends with `setDeltaMovement(delta × (f, 1, f))` where `f = getBlockSpeedFactor()`: the factor of the block the feet are in (soul sand is 14/16 high, so a body on it stands inside its cell), unless that block is water or a bubble column; when that factor is 1, the factor of the block 0.500001 under the feet at the supporting block. `LivingEntity` blends it with the `movement_efficiency` attribute (0 without Soul Speed). A flying or gliding player takes 1.
- `Blocks`: `soul_sand` `speedFactor(0.4)`; `honey_block` `speedFactor(0.4)` and `jumpFactor(0.5)`. No other block sets either. `friction` (ice 0.98, packed ice 0.98, frosted ice 0.98, blue ice 0.989, slime 0.8) is the third per-block movement number; the physics uses 0.6 for every block.
- The jump factor multiplies the jump's velocity (`LivingEntity.getJumpPower`), read the same way.

## The fix
1. **The packet and the sneaking body** (one commit): `C2s::PlayerInput { flags }` with the `INPUT_*` bits; `World::tick` reports the keys (forward, jump, shift) when they change, before the movement packet, and forgets what it sent on a respawn as a new `LocalPlayer` does; the model takes the packet and sets `Life::sneaking`; `physics::step` scales a sneaking body's input by 0.3 and backs its move off an edge (`back_off_from_edge`). Scripts `sneak:<yaw>` and `crouch` for the parity table.
2. **Block speed and jump factors** (a second commit, revertible alone): read from the game's classes by datagen's probe (`Block.getSpeedFactor`, `getJumpFactor`, `getFriction`) into `data::generated::movement`, applied in `physics::step_on` through a third oracle, `World::surface`.

Left out, on purpose:
- **Friction** (ice, slime) is generated but not applied: it changes every overworld walk over a frozen river for every bot and needs its own rows first.
- **The crouching box** (1.5 high): the bot's body stays 1.8 high while sneaking, which only ever refuses a gap a sneaking player could enter. **The crouching eye** (1.27): every aim in the planner uses 1.62; the one sneaking path (the scoop's centring) lets go of the key before it aims, several ticks ahead of the use.
- The one-tick lag of the crouching pose behind the shift key on a client (the first sneaking tick moves at full input).

## Evidence
See the table at the end; filled in as runs finish.
