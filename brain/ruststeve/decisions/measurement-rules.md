---
title: How ruststeve is measured
description: The measurement rules ruststeve starts with (the real race is the metric, a parity table before any sim number counts, a checkpoint start per stage, caps in game ticks, tens of paired runs, firings counted, case files on every ending).
type: decision
tags: [decision, measurement, races, sim, parity]
aliases: [real race metric, parity table, checkpoint starts, k/n, game-tick caps, paired runs, scripted expert]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#The real race is the metric. Report every rate as k/n."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A sim number counts only after a parity table against real at the same moments (lesson 4.2)."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Caps in game ticks, not wall time. Model knobs match the race config from one shared file."
  - "clojurecraft:brain/bot/tooling/race.md#Game time, not the clock, because the real servers tick far slower than 20 a second:"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[cc-loop-rules]]"
  - "[[replay-limits]]"
  - "[[cc-race-and-gym]]"
---

# How ruststeve is measured

## The choice
1. **The real race is the metric**: real vanilla 26.1.2 servers on runners, many bots each; every rate reported as k/n with its commit.
2. **A sim number counts only after a parity table**: the same recorded moments run in sim and real, outcome classes matching within a stated tolerance. A gap is a sim bug until shown otherwise.
3. **One config file for the race and the model**: simulation distance (3 on the race servers), tick rate, caps. The model reads the same file.
4. **Caps in game ticks**, counted from the server (20 per `set-time` heard), never wall time. A real server under 14 bots ticks at 150–300 ms; a wall-clock hour was under 12 game minutes there.
5. **A checkpoint start for every stage from day one** (a kit and a place), and every lane can play any stage.
6. **Judge a change on tens of paired runs** (same worlds, same seeds), and count the new code path's firings before reading the result.
7. **Every ending has a case file**: killer, first hit, intent and its why ([[cc-observability]]).
8. **A scripted expert per stage sets the ceiling**: if it fails, fix the plan, kit or gate, not thresholds.

## What was rejected
- Measuring by the gym alone (curriculum, not the measure), and `FUNCTIONAL.md`'s phase gates without parity: a sim pass at 30% against a real 3% is what clojurecraft learned to distrust ([[cc-stage18-lessons]]).
- Wall-clock timeouts in the bot or the harness.

## What would change the answer
A sim with a standing parity table that holds across stages could carry more of the measuring; the real race still decides.

## See also
- [[cc-loop-rules]] - the ML principles these come from.
- [[replay-limits]] - open loop vs closed loop.
