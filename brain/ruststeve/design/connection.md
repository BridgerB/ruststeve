---
title: The connection (protocol, net, bot)
description: How ruststeve connects to a vanilla server and stays in the world - the pure protocol crate (framing, typed packets, states, the upkeep Session), the net crate's reader and writer threads, the bot binary - and how to run a server and a bot locally.
type: how-to
tags: [protocol, net, bot, connection, keep-alive, threads]
aliases: [protocol crate, net crate, bot binary, Session, keep-alive, xtask server, run a bot locally, local server, receive-side state]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@58850720 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/protocol/src/session.rs#pub fn on_packet(&mut self, p: &S2c) -> Vec<Out> {"
  - "ruststeve:crates/protocol/src/state.rs#pub fn after_receive(self, p: &S2c) -> State {"
  - "ruststeve:crates/protocol/src/frame.rs#pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, ParseError> {"
  - "ruststeve:crates/net/src/lib.rs#pub fn connect(addr: &str) -> std::io::Result<Connection> {"
  - "ruststeve:crates/protocol/tests/properties.rs#fn keep_alives_answered_exactly_once("
  - "ruststeve:crates/xtask/src/main.rs#fn server(args: &[String]) -> ExitCode {"
  - "clojurecraft:src/clojurecraft/packet.clj#ClientboundLoginPacket.write"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[wire-connection-phases]]"
  - "[[wire-boundary-parsing]]"
  - "[[workspace-crates]]"
---

# The connection (protocol, net, bot)

A bot connects with our own protocol layer ([[own-protocol-not-azalea]]): a pure `protocol` crate does all the byte work, a `net` crate owns the socket, and the `bot` binary joins and stays in the world.

## Key files
- `crates/protocol` (core, no I/O): `bytes.rs` primitives, `frame.rs` framing and zlib, `packets.rs` typed `S2c`/`C2s`, `state.rs` the transitions, `session.rs` the upkeep reducer, `uuid.rs` the offline UUID.
- `crates/net`: `connect(addr)` starts a reader and a writer thread over bounded `sync_channel`s.
- `crates/bot`: `bot --host --port --name --seconds`.
- `xtask server`: a local vanilla 26.1.2 server from datagen's cached jar.
- `.github/workflows/session.yml`: the same on a runner (dispatch only).

## How it works
1. `bot` sends `opening(...)`: `intention` (next state 2) and `hello` with the offline UUID ([[wire-offline-login]]).
2. **The reader thread** reads bytes into `FrameDecoder`, takes whole frames (inflating once `login_compression` arrives), splits the id, `decode`s in its own `State`, and sends `Inbound::Packet` to the loop. A frame that will not decode is `Inbound::DecodeError`, the end is `Inbound::Closed(why)`: values, never dropped.
3. **The receive-side state.** The reader flips its state on the server's trigger packet itself (`login_finished`, `finish_configuration`, `start_configuration`), with no word from the writer: the server sends nothing in the next state until our acknowledgement arrives, and nothing in the old state after the trigger. Each flip is guarded by its source state, so the state only ever moves along `State::EDGES` (a property test found the unguarded version jumping login → play).
4. **The loop** folds each packet through `Session::on_packet`, which returns the replies: known packs, keep-alive echoes, pongs, finish-configuration, the teleport acknowledgement (and `player_loaded` once per play session), chunk-batch acknowledgements, configuration acknowledgement, and `SetCompression`. Keep-alives are answered here, as protocol upkeep that never waits on planning ([[wire-boundary-parsing]]).
5. **The writer thread** encodes each `C2s` (ids from `data`'s generated enums) and applies `SetCompression` in order, so every frame after `login_compression` carries a data length.
6. `Session` counts server ticks, 20 per `set_time` ([[sync-core-and-time]]).

## Running it locally (`nix develop`)
```bash
cargo run -p datagen -- generate     # once: downloads and sha1-checks the server jar
cargo run -p xtask -- server         # target/server, port 25565, offline, simulation distance 3
cargo run -p bot -- --seconds 600    # in another terminal
```
The world in `target/server` is kept between runs; nothing deletes it.

## Tests
`crates/protocol/tests/properties.rs`: any bytes into `decode` and `FrameDecoder` never panic; frames, varints and positions round-trip; every keep-alive is answered exactly once; the state moves only along the table. `PROPTEST_CASES=10000` runs them deeper.

## Gotchas
- Only what the bot uses is decoded; every other id is `S2c::Unknown` and counted, and decoders ignore the unread tail ([[wire-packets-as-data]]).
- The play `login` is read up to its game type, in the field order of the game's `ClientboundLoginPacket.write` as clojurecraft's spec records it.

## Limits
- No packet becomes an `Observation` yet; that is the world value, issue 04.
- The bot answers configuration with no known packs, so the server sends every registry in full.

## See also
- [[wire-connection-phases]] - the states and their packets.
- [[wire-framing]] - frames and compression.
