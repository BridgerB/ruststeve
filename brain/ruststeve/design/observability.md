---
title: Observability
description: How ruststeve shows what the bot believed, decided and why (recordings, events with reasons, case files, senses, the batch files and a read-only snapshot stream), and the coverage rule that nothing runs blind.
type: reference
tags: [observability, case-files, events, snapshots]
aliases: [case files, senses, ledger, snapshot stream, coverage, flight recorder, tracing]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Observability here means one thing: at any tick, a human can see what the bot believed, what it decided, why, and what actually happened, and compare them."
  - "clojurecraft:LOOP.md#Full observability is the rule (Bridger, Nov 6: \"otherwise you are a ship in the night\")."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Case files and the steve contract. Killer, first hit, intent and why for every ending; per-second senses rows; the Postgres schema steve reads."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[brain-events]]"
  - "[[cc-observability]]"
  - "[[data-out]]"
---

# Observability

At any tick a human must be able to see what the bot believed, what it decided, why, and what actually happened. clojurecraft learned this late: stage-18 deaths were counted for weeks before they were explained ([[cc-observability]]).

## What every run produces
| Output | Job |
|---|---|
| Recording | replay any run exactly, at its commit ([[recording-format]]) |
| `BrainEvent`s and a `Reason` on every command | the "why", as data ([[brain-events]]) |
| Case file per ending | killer (the server's death text first), first hit, the task and its why, items gained and lost, swings sent and landed, the replay command |
| Senses | one line a second; a moment a minute for the last few minutes; the whole mind at the end |
| Truth | real: entities near the ending as the server holds them; sim: the model's view, and what the bot was blind to |
| Batch file | every run of a lane batch: stage reached, ending, case signature ([[data-out]]) |
| Snapshot stream | read-only live view of one bot, never commands |

- **Coverage** is a number on every batch: runs with each file over runs that should have one, tasks with a why over tasks, events dropped (must be 0), packets thrown away unheard. Coverage under 1.0 is fixed before any bot change.
- **What exists (issue 07a)**: recordings, case files, batch files and coverage from the race ([[race-and-results]]); the death message and hits on the bot ([[wire-nbt-text]]). The live snapshot stream to a local steve ([[snapshot-stream]]). BrainEvents and senses come with the planner.
- `tracing` stays a developer tool in the shell; it is never the record of a decision.
- The flight recorder of `FUNCTIONAL.md` is the "moment a minute" ring above.

## What was dropped
Rerun and an in-process axum debug endpoint (`FUNCTIONAL.md`): ruststeve ships no viewer; steve is the front end, reading the SurrealDB insight store ([[data-out]]). Replays and the `why` binary are the REPL substitute.

## See also
- [[brain-events]] - the events.
- [[incident-to-regression]] - from a case to a regression test.
