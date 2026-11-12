---
title: The reducer and its effects
description: How clojurecraft changes its world (one pure reducer over events, effects written as data and drained by the loop), and the Rust shape of the same thing.
type: reference
tags: [oracle, reducer, effects, events]
aliases: [step, on-event, on-packet, emit, :bot/effects, compose, multimethod dispatch]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/game.clj#{:event/kind :tick :event/now ms :event/rand r}   ; the clock and randomness are inputs"
  - "clojurecraft:src/clojurecraft/game.clj#\"Queue a packet to send; this is the only place the protocol phase advances.\""
  - "clojurecraft:src/clojurecraft/game.clj#Packet handling is a multimethod on [phase name], so a new namespace can handle a packet"
  - "clojurecraft:src/clojurecraft/main.clj#\"The whole bot: the world reducer, then the planner. Requiring clojurecraft.wood and"
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-main-loop]]"
  - "[[sans-io-core]]"
---

# The reducer and its effects

`(step world event) → world'`, pure. Everything the bot wants done is written into the world as data under `:bot/effects`; the loop drains and performs them.

## Key files
- `src/clojurecraft/game.clj`: `step`, `on-event`, `on-packet`, `emit`, `say`, `effect`, `compose`.
- `src/clojurecraft/main.clj`: `step` = `(game/compose game/step plan/step)`.

## How it works
- **Events** are maps: `:start` (host, port, name), `:packet` (a decoded packet), `:tick` (`:event/now` ms and `:event/rand`), `:go` (a run begins), `:closed`.
- `on-event` dispatches on `:event/kind`; a packet goes to `on-packet`, a multimethod on `[phase packet-name]`, so a new namespace adds a packet handler without editing `game.clj` (`chat.clj`, `ear.clj`, `eye.clj`, `hear.clj` do exactly that).
- **Effects**: `{:effect/kind :send :effect/packet p}` and `{:effect/kind :log ...}`. `emit` is the only place the protocol phase advances (via `packet/transitions`), so the phase has one source of truth.
- The whole bot is two reducers composed left to right: the world reducer (`game/step`: packets, physics on ticks), then the planner (`plan/step`: goals and intents).

## For ruststeve
- `Brain::step(&mut self, &Input) -> Output` merges both reducers; `Output.commands` replaces `:bot/effects` and is returned, not stored in the world, which the essay itself flagged as a braid ([[cc-lessons]]).
- clojurecraft feeds one event per call; ruststeve batches a tick's observations into one `Input`. The fold order inside `step` must keep "packets before the tick's planning".
- Open multimethod dispatch on `[phase name]` becomes an exhaustive `match` on a typed packet enum ([[enums-for-closed-sets]]).

## Gotchas
- The protocol layer and the game share one reducer in clojurecraft (handshake, keep-alives, teleports are reducer code). ruststeve splits protocol upkeep into the wire layer ([[wire-boundary-parsing]]).

## See also
- [[cc-main-loop]] - who calls `step`.
- [[sans-io-core]] - the Rust API.
