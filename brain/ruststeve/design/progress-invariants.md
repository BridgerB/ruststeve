---
title: Progress invariants and budgets
description: The structural guards against the most expensive bug class (loops that make no progress): a progress invariant in every long intent, resources spent per unit of progress, a CPU budget per bot per tick, spatial indexes, and the churn invariant.
type: reference
tags: [design, progress, loops, cpu, watchdog]
aliases: [progress invariant, no-progress loops, resources per progress, CPU budget, spatial index, chunk buckets, churn]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Build a progress invariant into every long intent (distance to target must shrink, or digs must open new cells) and fail fast when it doesn't."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Measure resources spent per unit of progress (digs per block of distance, blocks placed versus kept), not just endings."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Index spatial data (chunk-column buckets) from the start."
  - "ruststeve:FUNCTIONAL.md#Every task kind defines a numeric progress metric and a patience budget. The watchdog is generic and doesn't know anything about blazes."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[expectations-and-watchdog]]"
  - "[[cc-stage18-lessons]]"
  - "[[cc-unmerged-leads]]"
---

# Progress invariants and budgets

clojurecraft's stage 18 died of loops that made no progress, each invisible in counts: tunnels that dug squares, legs that paced a corridor, blocks placed and dug back, a craft and a hunt preempting each other 410 times. ruststeve makes these structurally impossible rather than patching each one.

## How it works
- **Every long intent carries a progress invariant**, checked every tick: a walk's distance to its target must shrink; a tunnel's digs must open new cells; a heading must not revisit cells it already cut. Broken for a short window → the task fails fast with `NoProgress` and the planner recovers ([[expectations-and-watchdog]]). This is the guide's watchdog metric made specific.
- **Resources per unit of progress are measured**, not just endings: digs per block of distance, building blocks placed versus kept, pickaxe durability per metre. A reserve (the handoff's 32 building blocks) is a done-condition, not a hope.
- **Targets are held, not re-derived into flips**: a target chosen for an intent is kept until reached or failed; two candidates flipping each tick is a churn violation.
- **The churn invariant** (from clojurecraft's `churn_guard`): in any state, the task the planner chooses is not already over after one tick. A property test over recorded and generated worlds ([[cc-stall-and-budgets]]).
- **A CPU budget per bot per tick, measured in CI**: clojurecraft's planner was about 93% of its race JVM's CPU and starved the server on the same runner. Spatial data is indexed by chunk column from the start; no planner question scans the world.
- **Grid items count as held** while a craft is laid out (the 410-flip case).

## In ruststeve (issue 09a)
- The walk's invariant: no gain toward the target for 200 ticks fails it `NoProgress`, and its longest stall is reported per run ([[walk-and-watchdog]]).
- Targets are held; the churn invariant holds by construction (every candidate routed before it is chosen) and is tested over generated worlds; 12 failures in a row stop the planner, said so ([[planner]]).
- Spatial questions read memory's chunk-column index in rings; searches are budgeted in node expansions ([[pathfinding]]).
- **The CPU budget per bot per tick is measured in CI** (the `walk` job): the planner's step timed by the shell over every tick of the 200-landing measure, against `config/race.toml`'s `[budget]` (mean 500 µs, p99 5 ms); first measure mean 22 µs, p99 74 µs ([[planner]]).

## See also
- [[expectations-and-watchdog]] - the mechanism that fails a task.
- [[cc-unmerged-leads]] - the unmerged fixes these replace.
