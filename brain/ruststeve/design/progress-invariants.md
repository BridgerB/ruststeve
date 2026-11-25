---
title: Progress invariants and budgets
description: The structural guards against the most expensive bug class (loops that make no progress): a progress invariant in every long intent, resources spent per unit of progress, a CPU budget per bot per tick, spatial indexes, and the churn invariant.
type: reference
tags: [design, progress, loops, cpu, watchdog]
aliases: [progress invariant, no-progress loops, resources per progress, CPU budget, spatial index, chunk buckets, churn]
status: verified
lastUpdated: 2026-11-24
verifiedAgainst: clojurecraft@be85a92 ruststeve@31545c1
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Build a progress invariant into every long intent (distance to target must shrink, or digs must open new cells) and fail fast when it doesn't."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Measure resources spent per unit of progress (digs per block of distance, blocks placed versus kept), not just endings."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Index spatial data (chunk-column buckets) from the start."
  - "ruststeve:crates/plan/src/heading.rs#pub fn heading_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
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

## The heading (issue 17d, docs/problems/heading.md)
The three unmerged fixes clojurecraft left for this leg (`cage-line`, `heading-no-pace`, `filler-keep`) are built in as the heading's invariants (`crates/plan/src/heading.rs`), not ported:
- **Holds direction**: the axis changes only by a quarter turn after a failed leg (toward fewer seen columns, alternating on a tie), or a noted re-choice after 3 turns with none gaining 48. No clock turns it.
- **Never paces**: the furthest projection of the feet on the axis grows 6 blocks every 1,200 server ticks or the leg fails `NoProgress`; a leg's route never enters a cell the heading cut or stood in at or behind the feet (`Goal::Ahead` with blocked cells, the tunnel's `paced`); a way only back over the cut is noted "paced".
- **Spends in proportion**: digs, blocks placed and pickaxe uses per metre in every leg's end note; over 3 digs or 1 block a metre ends the leg. Walk, far walk with a keep of 32, tunnel only over a pickaxe reserve; under the keep it restocks from netherrack walls first.
- **Issue 18c's travel** (`plan::endermen`, docs/problems/pearls-travel.md): the pocket's way out is at most 4 digs (`EXIT_DIGS`), then given up; a forest place is tried 3 times (plain, far, tunnel above the reserve) and skipped after; the trip's state (health, food, pickaxe uses, filler, rods) is noted every 1,200 ticks and in every heading leg's note.

## See also
- [[expectations-and-watchdog]] - the mechanism that fails a task.
- [[cc-unmerged-leads]] - the unmerged fixes these replace.
