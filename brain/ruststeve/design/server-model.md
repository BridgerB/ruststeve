---
title: The server model
description: How ruststeve plays without a Java server - the sim crate (the vanilla server as a pure reducer, grown per issue), S2c encoders that make the model's bytes a server's, the model as the bot's body behind the same channels a socket gives, and what it models so far.
type: reference
tags: [sim, model, server, body, closed-loop]
aliases: [sim crate, Sim, server model, model body, In, Wire, ModelWorld, flat_world, S2c encode, NotModelled, movement check, moved wrongly, moved too quickly, fall damage, lockstep, generated worlds, sim terrain, 1000 worlds]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@d683c9b clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/sim/src/lib.rs#pub fn step(&mut self, input: &In) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#fn chunks_around(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/protocol/src/packets.rs#S2c::LoginFinished { uuid, username } => {"
  - "ruststeve:crates/bot/src/model.rs#pub fn connect(mut sim: Sim) -> Connection {"
  - "ruststeve:crates/bot/tests/model.rs#fn the_bot_joins_the_model_and_walks() {"
  - "ruststeve:crates/sim/src/lib.rs#fn on_move(&mut self, to: [f64; 3], on_ground: bool) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#fn fall(&mut self, dy: f64, on_ground: bool) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/terrain.rs#pub fn world(seed: u64, radius: i32) -> Result<ModelWorld, EncodeError> {"
  - "ruststeve:crates/bot/src/lockstep.rs#pub fn run("
  - "ruststeve:crates/bot/tests/worlds.rs#fn the_bot_walks_a_thousand_generated_worlds_clean() {"
  - "clojurecraft:src/clojurecraft/sim.clj#Two ways to drive it: run folds bot and model together, deterministic and fast (generated worlds); connect puts the model on a thread behind a channel pair shaped like conn/open!'s, so main's own loop (its clock, its effects, its tap) runs against it with no Java process."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[cc-sim]]"
  - "[[body-boundary]]"
  - "[[measurement-rules]]"
---

# The server model

Issue 08: the vanilla server as a pure reducer, so the bot can be tested closed-loop without a Java process ([[cc-sim]]). It **grows per issue** (Bridger, 2026-11-12): each issue adds the part of the server its gameplay meets, with parity rows, before any sim number counts ([[measurement-rules]]).

## Key files
- `crates/sim/src/lib.rs`: `Sim`, `In`, `Wire`, `ModelWorld`, `ModelColumn`, `flat_world`.
- `crates/protocol/src/packets.rs` (`S2c::encode`), `crates/protocol/src/chunk.rs` (`Paletted::pack`, `Column::encode`, `packet_body`).
- `crates/bot/src/model.rs`: the model as a connection; `crates/bot/tests/model.rs`.

## How it works
- **`Sim::step(&In) -> Vec<Wire>`** (core: no I/O, no clock): a client packet (`C2s`) or a server tick in; the server's packets out as the wire carries them (state, id, bytes). Who ticks it is the caller's choice.
- **The bytes are a server's**: every packet the model sends is written by `S2c::encode`, the inverse of `decode` (a property test round-trips each, and a chunk column cell for cell). A packet the model does not send yet is `EncodeError::NotModelled`, never invented. A captured world's columns go out as the server's own bytes; a generated one's through `Column::packet_body`.
- **The model as the bot's body**: `bot::model::connect(sim)` runs it on a thread behind the same `inbound`/`outbound` channels `net::connect` gives, decoding its bytes as the reader thread does, and ticking every 50 ms. `Opts::model` picks it; the bot's loop, recordings and replay are unchanged ([[body-boundary]]).
- **What 08a models**: login (offline uuid), configuration (known packs, finish; the bot reads no registry), the play login with the race's view and simulation distances, chunks before the first teleport, health, chunk batches within the view distance and `forget_level_chunk` as the player moves, `set_time` every 20 ticks, a keep-alive every 300 ticks with vanilla's 15 s kick, and moves ignored until the teleport is accepted. What a real server would refuse is kept in `violations`.

## The movement check and falls (08b, read from the 26.1.2 jar)
The 26.1.2 server jar is not obfuscated, so the rules are read with `javap` from `ServerGamePacketListenerImpl.handleMovePlayer`, `Entity.checkFallDamage`, `LivingEntity.calculateFallDamage` and `VoxelShape.collideX`, never from memory:
- **Too quickly**: the squared distance from the tick's first position over 100 × the move packets this tick (more than 5 count as 1) → the server sends the player back (`player_position`, a correction).
- **Wrongly**: the server moves the old position by the client's delta with its own collision; the error to the client's position, its y forgiven inside (-0.5, 0.5), over 0.0625 squared → a correction, but only when the old box was collision-free. **Colliding with anything new**: the new box (deflated 1e-5) in a block the old box was not in → a correction.
- **Falls**: falling (not in water) adds to the fall distance; landing hurts `floor(fall + 1e-6 - 3)` (safe fall 3, multiplier 1) and resets it; moving up resets it. A hurt sends `damage_event` (type `fall`) and `set_health`; at 0 health `player_combat_kill` with `death.attack.fall`.
- Not modelled: the server's own velocity for the player (knockback adds to the allowance), elytra, sleeping, vehicles.

## Generated worlds and lockstep (08b)
- `sim::terrain::world(seed, radius)`: seeded rolling ground (value noise), one-block steps, still water to y 62 over sand, carpets, cobblestone pillars, encoded as real chunk columns; a pure function of its seed.
- `bot::lockstep::run`: the bot's world and the model folded together in one thread with no clock: each tick the model ticks, the world ticks, and every packet is delivered both ways before the next. A run is a pure function of world, seed and code.

## Evidence
- **1,000 generated worlds** (`the_bot_walks_a_thousand_generated_worlds_clean`, CI's `worlds` job, release): every run placed, no correction, no violation, no decode error, no kick, no death; walked median 31 blocks in 400 ticks. A run repeats exactly from its seed. The first run found 1 world of 1,000 where the server corrected the bot 31 times at a water bank: a bot physics bug, fixed in [[physics]] (vanilla's 1e-7 collision tolerance).
- Model tests: a step accepted; 20 blocks in one move too quick; walking into a wall and sinking into the ground corrected; a 10-block fall hurts 7; into water it does not; a long fall kills with `death.attack.fall`.
- `the_bot_joins_the_model_and_walks`: the bot joins a flat model world, is placed on the grass, walks its staircase past 5 blocks, runs to its 100-tick cap with no correction, no decode error, and its recording replays with 0 mismatches (done-condition 2's first half).

## Limits
- Capture, simrace and the parity table: [[parity-table]]. Then digging (11), mobs, air and drowning (12) and the rest as their issues come.
- The server-side move uses the bot's own collision (`land_tick`), not vanilla's `Entity.collide` order (y, then the larger of x and z first): the parity table is what says whether it matters.
- `chunk_batch_start` goes out as its id with an empty body; the bot counts it unknown.
- Peaceful regeneration (`ServerPlayer.tickRegeneration`, 1 every 20 ticks below full) is modelled when `Config::peaceful`; found by the parity table.

## See also
- [[cc-sim]] - clojurecraft's model, the original.
- [[body-boundary]] - the seam the model plugs into.
