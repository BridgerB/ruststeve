---
title: The server model
description: How ruststeve plays without a Java server - the sim crate (the vanilla server as a pure reducer, grown per issue), S2c encoders that make the model's bytes a server's, the model as the bot's body behind the same channels a socket gives, and what it models so far.
type: reference
tags: [sim, model, server, body, closed-loop]
aliases: [sim crate, Sim, server model, model body, In, Wire, ModelWorld, flat_world, S2c encode, NotModelled]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@ff7281f clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/sim/src/lib.rs#pub fn step(&mut self, input: &In) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#fn chunks_around(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/protocol/src/packets.rs#S2c::LoginFinished { uuid, username } => {"
  - "ruststeve:crates/bot/src/model.rs#pub fn connect(mut sim: Sim) -> Connection {"
  - "ruststeve:crates/bot/tests/model.rs#fn the_bot_joins_the_model_and_walks() {"
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

## Evidence
- `the_bot_joins_the_model_and_walks`: the bot joins a flat model world, is placed on the grass, walks its staircase past 5 blocks, runs to its 100-tick cap with no correction, no decode error, and its recording replays with 0 mismatches (done-condition 2's first half).

## Limits
- No movement check, falls or death yet (08b), no capture, simrace or parity (08c); then digging (11), mobs (12) and the rest as their issues come.
- `chunk_batch_start` goes out as its id with an empty body; the bot counts it unknown.

## See also
- [[cc-sim]] - clojurecraft's model, the original.
- [[body-boundary]] - the seam the model plugs into.
