---
title: The live snapshot stream
description: How a running bot shows itself live in steve's local front end - the stream crate (an outbound WebSocket on its own thread, MessagePack frames per schema/snapshot-v1.md), what the bot sends and how often, reconnects, counts, the stand-in relay, and why it can never change a run.
type: reference
tags: [stream, live-view, steve, websocket, observability]
aliases: [snapshot stream, STEVE_RELAY, relay, relay-stub, live view, Sink, snapshot-v1, hello frame, self frame]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@9ae5ef0
sourceRefs:
  - "ruststeve:crates/stream/src/lib.rs#pub fn send(&self, tick: u64, body: Body) {"
  - "ruststeve:crates/stream/src/lib.rs#fn connect(url: &str, id: &Identity, mirror: &Mirror, c: &Counters) -> Result<Conn, String> {"
  - "ruststeve:crates/bot/src/live.rs#pub fn self_state(w: &World, doing: &Doing) -> Option<Body> {"
  - "ruststeve:crates/stream/tests/relay.rs#fn a_reconnect_sends_hello_self_and_chunks_again() {"
  - "ruststeve:schema/snapshot-v1.md### The sink's rules (the bot side)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[data-out]]"
  - "[[observability]]"
  - "[[insight-store]]"
---

# The live snapshot stream

Issue 07c: Bridger watches a bot live in steve, both running on the Mac, to see where it gets stuck. Runners never stream; their results reach steve through the [[insight-store]]. The contract is `schema/snapshot-v1.md`.

## Key files
- `crates/stream/src/lib.rs`: `Sink`, `Body`, `Frame`; `src/bin/relay-stub.rs`, a stand-in relay.
- `crates/bot/src/live.rs`: the bodies read from the world after a step.
- `schema/snapshot-v1.md`: frames, kinds, the sink's rules.

## How it works
- **Out only.** With `STEVE_RELAY` set (`ws://localhost:8787`), `bot` and `race` start a `Sink` that connects out to `$STEVE_RELAY/ingest/<run>-<bot>` and only sends. Unset: no sink.
- **Frames**: one binary message each, a flat named MessagePack map: the envelope `v` (1), `kind`, `bot`, `run`, `tick` (server ticks), `seq` (per connection), then the kind's fields. Kinds: `hello`, `self`, `chunk` (the raw `level_chunk_with_light` body as received), `unload`, `block`, `entities`, `gone`, `event`, `end`.
- **When**: `chunk`, `unload`, `block` and `gone` as their packets fold; `self` every 4 ticks (5 a second at most) and `entities` every 2 (10 a second), on the loop's tick; `end` once with the ending, the server's death text and the stage reached.
- **Never in the way**: the loop hands bodies to a bounded channel (1,024) and never waits; full means dropped. The sender thread owns the socket and a mirror of the latest `self` and every loaded chunk, so a reconnect (backoff 1 s doubling to 30 s) sends `hello`, `self` and the chunks again. Frames with no connection count as failed. A run's case records sent, dropped, failed and connects.
- **An observer**: bodies are read after the step; nothing comes back. The recording is the same with the sink on, off or failing.
- `relay-stub` (`cargo run -p stream --bin relay-stub`) decodes every frame as v1 and tallies kinds, for checking without steve.

## Evidence
- Live, 20 s walk to `relay-stub`: 1,091 frames (348 chunks, 393 blocks, 67 `self`, 133 `entities`, 118 `gone`, 30 unloads, hello, end), 0 dropped, 0 failed, every frame decoded with consecutive `seq`; `replay verify` 0 mismatches.
- A dead relay (`ws://localhost:9`), 15 s: the bot ran the same, 549 frames counted failed, `replay verify` 0 mismatches.
- Tests: the frames in order with the envelope; a frame is one flat map; 5,000 sends with no relay take under 0.5 s; a relay that hangs up gets `hello`, `self` and the chunk again on the reconnect, `seq` from 0.

## Gotchas
- `held` and `air` are not folded yet (`""` and -1); `tasks` is the scripted task until the planner (issue 09), and `event` waits for BrainEvents.
- `hostile` is the game's monster category from the generated mob table, not the bot's threat model.

## See also
- [[data-out]] - the stream beside the store.
- [[observability]] - what every run leaves.
