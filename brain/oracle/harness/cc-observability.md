---
title: Observability in clojurecraft
description: What every ended clojurecraft run leaves behind (case file, ledger, senses, moment, server and sim truth, recording), the coverage row that proves nothing is blind, and what ruststeve must produce from day one.
type: reference
tags: [oracle, observability, case-files, ledger]
aliases: [case file, ledger, senses line, moment, truth ring, blind, coverage, botlogs, why]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:brain/bot/tooling/observability.md#The bot's whole mind is one world value, so everything a reader needs about a run is either in it or heard by it;"
  - "clojurecraft:brain/bot/tooling/observability.md#Why: every running intent carries a why (intent/why, on metadata, so equality is unchanged):"
  - "clojurecraft:LOOP.md#Full observability is the rule (Bridger, Nov 6: \"otherwise you are a ship in the night\")."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Case files for every death. Once each ending carried its killer, its first hit and the intent with its \"why\", causes could be counted instead of guessed."
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[observability]]"
  - "[[cc-entities]]"
---

# Observability in clojurecraft

Deaths on stage 18 were counted but never explained until 2026-11-06, when full observability became the rule.

## What every ended run leaves

| File | Holds |
|---|---|
| `.case.txt` / `.case.edn` | the ending, the killer (the server's death text first, then the server log, then the last hit), the last minute's first hit and what the bot was doing then and *why*, what lit it, items gained and lost, hits by attacker, swings sent and landed, the replay command |
| `cases.edn` | the batch's table of signatures `[ending killer doing why]` |
| `.events.edn` | the ledger: every change between two worlds, unsampled, with a count of drops (must be 0) |
| `.senses.edn` | one line a second, the last 300 |
| `.moment.edn` / `.ring.edn` | the whole mind at the end; a moment a minute for the last five |
| `.server.txt` (real) | every entity within 32 blocks of the ending, as the server holds it |
| `.truth.edn` (sim) | the model's view: each mob near, its health, anger, target and what it dealt; plus a ring every 5 simulated seconds |
| `.edn.zst` | the full recording of every failed run |

- **Why on every intent**: each running intent carries the planner branch that chose it (`:hunt/back-off-from-enderman`), kept in intention facts and printed in cases.
- **Blind**: beside a sim run's moment, the mobs it never tracked, the ones targeting it that it ranked no threat, and blows from attackers it did not know.
- **Coverage row** per batch: runs with each file over runs that should have one, intents with a why over intents, ledger drops, and packets thrown away unheard. Full sight is every pair whole, 0 dropped, nothing unheard.
- Fights became explicable only after decoding damage types, sounds, hurt flashes (set by the damage event that names the mob: a real server never sends a mob's hurt animation), equipment and attributes ([[cc-entities]]).

## For ruststeve
Issue 8 of the plan makes case files and per-second senses a done-condition before gameplay (coverage 1.0) ([[first-twenty-issues]]). The `BrainEvent` stream and `Reason` on every command are the Rust form of the ledger and the why ([[brain-events]]); case files are written by the shell from them, as files in the recording format ([[data-out]]).

## See also
- [[observability]] - ruststeve's design.
- [[cc-loop-rules]] - the reading routine.
