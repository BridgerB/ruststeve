---
title: clojurecraft's main loop
description: How clojurecraft's one impure loop turns socket packets, a 50 ms clock and external events into reducer calls, and what that means for ruststeve's notion of a tick.
type: reference
tags: [oracle, loop, clock, io]
aliases: [run-loop!, apply-event!, swap!, epochal write, run-bot!, physics tick]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/main.clj#\"The loop: the one place with an atom, a clock, randomness and effects. Reads packets and"
  - "clojurecraft:src/clojurecraft/main.clj#:else {:event/kind :tick :event/now (now) :event/rand (rand)})]"
  - "clojurecraft:src/clojurecraft/main.clj#\"The epochal write: swap the world, then perform and clear what it asked for. on-ms, when given,"
  - "clojurecraft:src/clojurecraft/game.clj#\"pos-rot when something changed, status-only once a second otherwise (vanilla's rule).\""
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-reducer-and-effects]]"
  - "[[tick-loop]]"
  - "[[cc-randomness-on-the-tick]]"
---

# clojurecraft's main loop

`main.clj` is the one place with an atom, a clock, randomness and effects. It reads packets and external events, feeds the reducer, drains `:bot/effects`, and prints `RESULT`.

## Key files
- `src/clojurecraft/main.clj`: `run-loop!`, `apply-event!`, `perform!`, `run-bot!`.
- `src/clojurecraft/game.clj`: `on-tick`, `movement-packets`.

## How it works
1. `run-loop!` waits (`alts!!`) on three channels: the socket's decoded packets, the external events queue (a fixture's `:go` on stdin), and a timeout until the next physics tick (every `physics/tick-ms`).
2. Whichever fires becomes one event: `:packet`, the external event, or `:tick` with `(now)` and `(rand)`. A closed socket becomes `:closed`.
3. `apply-event!` is the epochal write: tap the event to the recorder, `swap!` the world through `step`, then take the effects, tap them, clear them and perform each (`:send` to the socket's out channel, `:log` to stderr). It also measures how long the swap took (`on-ms`), in the I/O layer, never in a reducer.
4. On a tick the world reducer advances `:time/now` and `:time/tick`, runs physics if ready, and emits the movement packet vanilla would: position-and-look when something changed, a status-only packet once a second otherwise.

## For ruststeve
- clojurecraft's clock is the *client's* own 50 ms timer measured in wall-clock ms (`:time/now`), and every intent deadline is an absolute ms value. FUNCTIONAL wants game `Tick`s in the core. Porting means converting each ms deadline to ticks, and deciding what a tick is when the loop runs late. clojurecraft already keeps both: `plan/wait-ticks` and `wait-ms` exist because a loop behind schedule fires missed ticks back to back ([[cc-stall-and-budgets]]).
- The server's own ticks are counted separately (20 per `set-time` packet heard), so a slow server is measurable.
- Swing pacing is by the clock (650 ms), not ticks; see [[hist-v1-load-bearing-fixes]].

## See also
- [[tick-loop]] - ruststeve's version.
- [[cc-record-replay]] - the tap in step 3.
