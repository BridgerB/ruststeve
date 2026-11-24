---
title: Observability
description: How ruststeve shows what the bot believed, decided and why (recordings, events with reasons, case files, senses, the batch files and a read-only snapshot stream), and the coverage rule that nothing runs blind.
type: reference
tags: [observability, case-files, events, snapshots]
aliases: [case files, senses, ledger, snapshot stream, coverage, flight recorder, tracing]
status: verified
lastUpdated: 2026-11-24
verifiedAgainst: ruststeve@064c33e clojurecraft@be85a92
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

## Issue 16e: the ledger, senses and moments (phase C)
- **Tasks** (`plan::ledger`): the planner numbers each task where it begins (`Plan::begin`: a choice, a dig after a walk, an always-row cutting in) and records `Started` with the note that chose it and `Ended` with the note that closed it; `Plan::close` ends the one running with the run. A real-terrain test holds every task to a reason and exactly one end.
- **Facts** (`crates/observe`, pure): `snapshot` takes the parts of the world the ledger compares each tick; `diff` gives health, food, each hit and what dealt it, catching fire and the likely cause, item changes, a tool falling under a half, a quarter and a tenth of its uses and breaking, dimension, stage, death, the win, effects, sounds and level events (within 16, or loud), chat, mobs within 32 appearing and going and showing a sign (fuse, charge, angry, facing).
- **Senses** (`observe::senses`): a row a second: position, health, food, saturation, air, fire, the held item and its uses, every tool's uses, free slots, the task and its reason, the six nearest hostiles with their signs and held item, endermen within 128, the newest sound, the planner's tick time.
- **Moments**: the planner whole with that second's senses, once a minute for the last five and at the end; the world of the same minute is the recording's checkpoint.
- **Files**: `<bot>.ledger.rsl` (tasks and facts), `<bot>.senses.rss`, `<bot>.moments.rsm`, beside the case and in the release's batches bundle; `results ledger <file>` and `results senses <file> [--every N]` print them.

## Issue 16: the files while the race runs
Until issue 16 the ledger, senses and case were written at a run's end, so a race was dark until it was over and a lane cut at its limit left nothing. Now each bot rewrites them as text every 2,400 ticks and the lane uploads them to the race's release every 5 minutes, with any new snapshot ([[race-and-results]], `docs/problems/race-tooling.md` wall 4).

## Issue 16: the text reads right the first time (`docs/problems/observability-gaps.md`)
The files were whole and were still misread twice in one night (race 45204423376). The rules since:
- **One cell convention.** A senses row keeps `cell`, the position floored (`observe::feet_cell`, held equal to the planner's `plan::path::feet_cell` by a test): z -18.3 is cell -19, and the old text's rounded `30,31,-18` was read as the cell. With it `feet_block` (the block in the cell: air on a full block, soul sand or a fluid when the feet are inside its cell), `under`, and `lava` / `water` as `Touch::In | Beside | No` over the body's two cells and the cells against them. `Senses::standing()` says it in words: `cell 30,31,-19 on netherrack, lava beside`. These read only the bot's own world, for the files; nothing of them reaches the planner.
- **Holdings include the cursor.** `Snapshot::items` counts the cursor's stack, `Fact::Cursor` says what is on it, and while a click waits for its answer (`CLICK_SETTLE`, 20 client ticks) the holdings are read as they last settled: the world forgets the cursor as it clicks, so a moved tool read `1 -> 0`, `0 -> 1`.
- **A break is a break.** `tool broke` only when a tool's uses run out in its slot or it leaves the holdings on its last use (`BREAK_AT`); gone with uses left is `Fact::ToolGone`.
- **The ledger text says each thing once** (`results::text`): the planner's records in the planner's order (ends were sorted before starts, so a task of one second read as ended before it began), a run of the same lines (a block of up to four, the same but for the task's number) once with `[xN, to tick T]`, one second's changes to an item, health, food or the cursor on one line with the count of changes. The stamp is the server's second: `set_time` comes every 20 ticks. The `.rsl` holds everything; a line is never cut.
- **The case** opens with `where:` (dimension, position and cell, what the feet stand on, lava or water, held item and cursor, the task and its reason, the last stage and the ticks since), counts every note of the ledger by kind (`story::note_kind`: leading words to the first number or bracket, an item's name as `*`), lists every note carrying `trail:` whole, says how many tasks of a kind ended within the second they began, and tells the last two minutes as the ledger does.
- **No loss of health is silent.** A fall in health with no damage event of its own (none within `HIT_WINDOW`, 10 client ticks, that an earlier fall had not taken) is `Fact::HealthDrop` with the nearest hostile and whether the bot burned; the case counts them. New blows are told from the newest blow's stamp (`Snapshot::hit_mark`), not from the length of `World::hits`, which stands still once the world prunes it.
- `replay ledger <rsr> --force [--raw] [--senses N] [--story]` prints all of it for any recording.
- **The day clock** (issue 16, `docs/problems/day-clock.md`): the ledger says `night fell (day time T)` at dusk (or when a bot joins at night) and `day broke (day time T)` at dawn (`Fact::Night`, `Fact::Day`), and each senses row keeps `day_time` and prints `day T` (` night` added after dusk). An old recording read with `replay ledger` gets both, since the time is decoded from the recorded `set_time` bytes.

## What was dropped
Rerun and an in-process axum debug endpoint (`FUNCTIONAL.md`): ruststeve ships no viewer; steve is the front end, reading the SurrealDB insight store ([[data-out]]). Replays and the `why` binary are the REPL substitute.

## See also
- [[brain-events]] - the events.
- [[incident-to-regression]] - from a case to a regression test.
