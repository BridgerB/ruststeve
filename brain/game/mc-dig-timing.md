---
title: Dig timing
description: How long a block takes to break (vanilla's damage-per-tick rule, tools, penalties), what the server does with an early FINISH (a delayed destroy, read from the 26.1.2 jar), and the margin clojurecraft sends FINISH with.
type: reference
tags: [game, digging, timing]
aliases: [delayed destroy, hasDelayedDestroy, break time, hardness, early FINISH, finish margin, dig penalties, player-action]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve@3042214
sourceRefs:
  - "ruststeve:crates/sim/src/survival.rs#const FINISH_ACCEPT: f32 = 0.7;"
  - "ruststeve:crates/sim/src/survival.rs#pub(crate) fn tick_digging(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "clojurecraft:src/clojurecraft/dig.clj#damage per tick = speed / hardness / (30 when the tool harvests the block, else 100)"
  - "clojurecraft:src/clojurecraft/intent.clj#(def finish-margin 1.15)              ; FINISH is sent this late: the server takes it at 70% of its own ticks, aborts it before"
  - "clojurecraft:src/clojurecraft/intent.clj#margin, since an early FINISH aborts the break and a late one is accepted.\""
  - "clojurecraft:src/clojurecraft/intent.clj#(def confirm-ticks 60)                ; after FINISH, ticks to wait for the server's block update: then the ack alone is a refusal, and silence trusts the overlay"
  - "clojurecraft:src/clojurecraft/sim/world.clj#by 1 horizontally and 0.5 vertically)?\""
  - "clojurecraft:src/clojurecraft/sim/world.clj#(def pickup-delay 500)                ; ms before a dropped item can be picked up (vanilla: 10 ticks)"
related:
  - "[[game/_moc|Game]]"
  - "[[wire-actions]]"
  - "[[cc-datagen]]"
---

# Dig timing

The client digs with two `player-action` packets, START and FINISH. The server breaks the block at once if enough of its own ticks have passed; an earlier FINISH leaves the break to the server, which finishes it when its own count reaches the full time.

## Key files
- clojurecraft `src/clojurecraft/dig.clj` (the rule), `src/clojurecraft/intent.clj` (`finish-delay`, `slowdown`, `penalties`, the dig stages).

## How it works
- **Vanilla's rule**: damage per tick = speed / hardness / (30 when the tool harvests the block, else 100); ticks = ceil(1 / damage), or 0 when damage ≥ 1. Speed is the tool tier's speed when the tool is the right kind for the block, else 1, divided by 5 for each penalty in force: the eyes in water, the feet off the ground. The tool decides harvestability (whether it drops), never the penalised speed. All numbers come from generated tables.
- A log by hand: hardness 2 → 60 ticks → 3000 ms at 20 ticks a second.
- **What the server does** (`ServerPlayerGameMode.handleBlockBreakAction` and `tick`, read with `javap` from the 26.1.2 jar, issue 10b): progress = damage per tick × (server ticks since START + 1). FINISH at progress ≥ 0.7 breaks the block. **Below 0.7 it is not an abort**: the server records a delayed destroy and its tick breaks the block once progress reaches 1.0 — unless a delayed destroy is already pending, when the FINISH is ignored. ABORT ends only the dig in progress. Out of reach (eye to the block's box over 4.5 + 1) the action does nothing. Confirmed by the parity table's `dig:5` rows: 10 of 10 real blocks broke without a second FINISH ([[parity-table]]).
- **FINISH timing**: the server takes a FINISH at 70% of its own ticks; clojurecraft believed an earlier one aborts (a hypothesis for why it looked so: its 60-tick confirm window ran out before the delayed destroy landed, or a second dig was refused while one was pending). clojurecraft sends FINISH at `ms × 1.15 × slowdown + 100 ms`, where `slowdown` is how much longer the server's ticks are taking than 50 ms (measured from `set-time` arrivals). A late FINISH breaks at once; an early one breaks only at the full time, after the bot may have stopped waiting.
- **Confirmation**: after FINISH the server sends the breaker a block update (to air) and then a `block-changed-ack` for the FINISH sequence. clojurecraft waits up to 60 ticks for the update; an ack without it is a refusal.
- Pickup needs no packet: the server hands a drop over when the player's box (grown 1 sideways, 0.5 up) touches it, after a 10-tick delay.

## Gotchas
- A loaded server's ticks stretch; a fixed wall-clock FINISH that worked at 50 ms ticks aborts at 300 ms ticks. Scale by the measured slowdown.
- Hold the right tool *before* START and confirm the held slot; ruststeve v1 and steve both lost digs to a drifted held slot ([[hist-v1-load-bearing-fixes]]).

## See also
- [[datagen]] - ruststeve's hardness and needs-correct-tool tables (`data::generated::hardness`), read from the game by the JNI probe and equal to clojurecraft's for all 1,168 blocks.
- [[wire-actions]] - the packet.
- [[cc-datagen]] - hardness and tool tables.
