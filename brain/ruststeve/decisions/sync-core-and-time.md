---
title: A synchronous core, threads, and time as inputs
description: Why ruststeve's loop is plain threads with bounded channels (tokio only at the edges, if at all), and how time and randomness enter: the client clock and a random seed on every tick event, caps in server ticks.
type: decision
tags: [decision, threads, time, randomness, tokio]
aliases: [no tokio in the core, crossbeam, sync_channel, Event::Tick, now_ms, rand_seed, server ticks, Tick vs ms]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Keep the core synchronous; tokio only at the edges (HTTP/WebSocket to steve, Postgres) if at all."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Event::Tick { now_ms, rand_seed }. No Instant::now() or rand::thread_rng() anywhere outside the loop."
  - "ruststeve:FUNCTIONAL.md#Use #[tokio::main(flavor = \"current_thread\")]."
  - "clojurecraft:src/clojurecraft/game.clj#\"The server ticks the bot has seen since it logged in: 20 for each set-time packet heard (the"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[tick-loop]]"
  - "[[cc-main-loop]]"
  - "[[determinism-rules]]"
---

# A synchronous core, threads, and time as inputs

## The choice
- **Threads and bounded channels**: one reader thread decodes packets and chunks; one loop thread steps the world; observers get snapshots over a bounded channel and drop rather than block (`std::sync::mpsc::sync_channel` or `crossbeam-channel`). `tokio` only at the edges (the snapshot stream, the SurrealDB ingest), if at all.
- **Time as an input, as clojurecraft did**: the tick event carries the client clock (`now_ms`) and a random seed; deadlines inside intents are absolute times compared against the event's clock; nothing in the core reads a clock or an RNG of its own.
- **Caps and budgets in server ticks**: the bot counts the server's ticks (20 per `set-time` heard) and the server's slowdown, and every cap (a run's length, a stage's time) is in game time.

This resolves the guide's open question of `Tick` versus clojurecraft's milliseconds: the loop's own 50 ms tick is the event; the clock value on it is recorded, so replay is exact; game time comes from the server.

## What was rejected
- `FUNCTIONAL.md`'s `tokio` current-thread runtime and `async` body trait for the core loop: a synchronous loop is simpler and what clojurecraft ran successfully.
- A seeded RNG living inside `Brain` as the only randomness: either is deterministic, and the handoff keeps clojurecraft's per-tick seed. Whichever is used, it is recorded with the input.

## What would change the answer
A shell edge (many bots in one process, the snapshot stream) that is clearly simpler async; it stays at the edge.

## See also
- [[tick-loop]] - the loop.
- [[cc-main-loop]] - clojurecraft's loop.
