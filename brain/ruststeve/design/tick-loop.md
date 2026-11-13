---
title: The tick loop
description: How the loop drives the core each tick on plain threads with bounded channels, why the event is recorded before stepping, and what runs on which thread.
type: reference
tags: [shell, architecture, tick, threads]
aliases: [main loop, shell loop, reader thread, actuator, epochal write]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@3ee956a clojurecraft@be85a92
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#recorder.write(&Record::Input(&input))?;           // record BEFORE stepping, so a panic still leaves the input on disk"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#One reader thread decodes packets and chunks; one loop thread steps; observers get snapshots over a bounded channel and drop rather than block."
  - "clojurecraft:src/clojurecraft/main.clj#\"The epochal write: swap the world, then perform and clear what it asked for. on-ms, when given,"
  - "ruststeve:FUNCTIONAL.md#Make long-running commands supersedable."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[sans-io-core]]"
  - "[[sync-core-and-time]]"
  - "[[recordings]]"
---

# The tick loop

The loop thread owns the world and is the only caller of `step`. It makes no game decisions ([[sync-core-and-time]]).

## How it works
1. **Threads**: a reader thread reads frames, decompresses and decodes packets and chunk columns, and puts them on a bounded channel. The loop thread waits on that channel, an external-events channel and a timer for the next 50 ms tick, like clojurecraft's `run-loop!` ([[cc-main-loop]]).
2. Whichever fires becomes the next input: decoded observations, an external event (a run's `go`), or a tick carrying the clock and a random seed.
3. **Record, then step**: the input is written to the recording *before* `step`, so a panic still leaves it on disk; then `step`, then the output is recorded.
4. **Perform**: the loop performs the effects (packets to the writer, through a bounded channel) and publishes a snapshot to observers over a bounded channel that drops rather than blocks.
5. Long commands (walking, digging) are supersedable: a newer command or a stop cancels the old one. The core never waits.

## Gotchas
- **A fixed 20 Hz schedule** (issue 13a): each tick is due at the last one's due time plus 50 ms, and the loop waits for a packet only until then; far behind (over a second, a stalled machine) the schedule starts again. Before, it waited up to a whole tick for a packet after each tick, so ticks drifted 50 to 100 ms apart and the bot played at about 0.65 of a client's pace (found by the furnace's half rows, [[smelting]]).
- Wall-clock pacing (swing gaps, reconnect backoff) is real but enters only as the recorded clock on the tick event, never as a clock read inside the core.
- A reader thread that dies silently leaves the bot on a dead wire; it must turn any error into a closed event, as clojurecraft's does ([[wire-framing]]).
- Keep `panic = "unwind"` so the recording flushes; a reconnect starts a new recording segment linked to the previous one.

## See also
- [[body-boundary]] - what sits under the reader and writer.
- [[observability]] - who reads the published snapshot.
