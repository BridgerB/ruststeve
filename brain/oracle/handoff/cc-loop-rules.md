---
title: The operating loop (LOOP.md)
description: How clojurecraft was driven day to day: the stuck stage and its throughput first, the checkpoint rule, evidence read from the bot's own logs, the ML principles and the review bar.
type: reference
tags: [oracle, handoff, process, loop, ml-principles]
aliases: [LOOP.md, stuck stage, checkpoint rule, ML principles, never asleep at the wheel, review bar]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:LOOP.md#The stuck stage is the lowest stage under about 5% in the real or sim race."
  - "clojurecraft:LOOP.md#If the stuck stage gets fewer than a few hundred attempts a day, fixing the cap comes before any behaviour fix."
  - "clojurecraft:LOOP.md#The moment a stage becomes the stuck stage, it gets its own start in both races. Bots begin at the stage, not at the start of the chain before it."
  - "clojurecraft:LOOP.md#A reason code is where a run ended, never why."
  - "clojurecraft:LOOP.md#3. A scripted expert per stage sets the ceiling. If the expert fails, fix the plan, kit or gate, not thresholds."
  - "clojurecraft:LOOP.md#No log-line cause, no merge. Check each claimed cause yourself."
related:
  - "[[oracle/handoff/_moc|Handoff]]"
  - "[[measurement-rules]]"
  - "[[cc-observability]]"
---

# The operating loop (LOOP.md)

## The stuck stage first
- The stuck stage is the lowest stage under about 5% in the real or sim race. Every tick first asks: how many attempts did it get, real and sim, and what caps that number? Under a few hundred a day, fix the cap before any behaviour.

## The checkpoint rule
- The moment a stage is stuck it gets its own start in both races: a kit (the race kit plus what earlier stages yield) and a place (where the stage begins). Checkpoint bots are their own kind on the results page, and every lane plays the checkpoint. Real moments of bots that reached a stage can become starts too. The gym is curriculum; the races are the measure.

## Evidence
- Every ended run leaves a case file, a ledger, senses, a moment, server truth and (if failed) its recording ([[cc-observability]]). The routine: coverage first (cases over runs, under 1.0 is a blind spot fixed before any bot change), the signatures ranked, three case texts of the top one, and only then a replay probe.
- A reason code is where a run ended, never why. A worker's cause is a hypothesis until one bot's log lines confirm it. Count before you fix.

## The ML principles (Bridger, 2026-10-31)
1. Sim and real must agree; a sim number counts only after a parity table.
2. The real race is the honest metric; every rate as k/n.
3. A scripted expert per stage sets the ceiling; if it fails, fix the plan, kit or gate, not thresholds.
4. Structure beats magnitude; every fix names its structural cause from a log line.
5. Sweep numbers in the sim; thresholds live in data tables.
6. Reverse curriculum: practise every phase (the checkpoint rule).
7. Observation discipline: the bot uses only what the server sends, and the sim sends only that.

## Review
- No log-line cause, no merge. A change touching a passing stage needs a paired sim (same worlds and seeds) showing the stage does not drop. After a merge, read the next runs' logs for the fixed behaviour, not just counts.

## Never asleep at the wheel
Every working session ends with something changed: a commit, a finding with run id and log lines, a dispatched run that answers a named question. Waiting happens in the background, never as the work.

## See also
- [[measurement-rules]] - these rules as ruststeve decisions.
- [[cc-stage18-lessons]] - what happened when they were honoured late.
