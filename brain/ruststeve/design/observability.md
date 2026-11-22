---
title: Observability
description: How ruststeve shows what the bot believed, decided and why (recordings, events with reasons, case files, senses, the batch files and a read-only snapshot stream), and the coverage rule that nothing runs blind.
type: reference
tags: [observability, case-files, events, snapshots]
aliases: [case files, senses, ledger, snapshot stream, coverage, flight recorder, tracing]
status: verified
lastUpdated: 2026-11-21
verifiedAgainst: ruststeve@a45d012 clojurecraft@be85a92
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
- **What exists (issue 07a)**: recordings, case files, batch files and coverage from the race ([[race-and-results]]); the death message and hits on the bot ([[wire-nbt-text]]). The live snapshot stream to a local steve ([[snapshot-stream]]). BrainEvents, senses, moments and truth did not come with the planner: on 2026-11-21 none existed (issue 16e).
- `tracing` stays a developer tool in the shell; it is never the record of a decision.
- The flight recorder of `FUNCTIONAL.md` is the "moment a minute" ring above.

## Issue 16e: the coverage row that cannot lie (phase A)
- Every clientbound packet is decoded, `wanted` or `skipped` with a reason (`config/observe.toml`); the test `every_clientbound_packet_is_classified` (crates/results) fails on a packet in no list, in two, or decoded and still listed, so a new game version adds nothing unseen.
- `Coverage` counts thrown-away packets by `state/name` (`protocol::packet_name`), item slots cut short, and has a column for each output a run should leave (ledger, senses, moments, truth, tasks with a reason, tasks ended, ledger drops). A column not measured yet is `None` and prints `not measured !!`: it never reads as fine.
- The summary prints the coverage block first, `!!` on each blind spot, then k/n. A case signature now names what the planner was doing at the end (its last event, as a kind), not the aim it began with.
- What phase A found is the backlog: the wanted packets, and every `not measured` column, are phases B to D of `docs/problems/observability.md`.

## What was dropped
Rerun and an in-process axum debug endpoint (`FUNCTIONAL.md`): ruststeve ships no viewer; steve is the front end, reading the SurrealDB insight store ([[data-out]]). Replays and the `why` binary are the REPL substitute.

## See also
- [[brain-events]] - the events.
- [[incident-to-regression]] - from a case to a regression test.
