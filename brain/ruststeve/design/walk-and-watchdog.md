---
title: The walk and its watchdog
description: How ruststeve walks a route - clojurecraft's walk intent (waypoints, follow, replanning) plus the progress invariant that fails a walk which stops getting closer, the deadline, and the failures as data.
type: reference
tags: [plan, walk, progress, watchdog, invariants]
aliases: [Walk, walk task, follow, waypoint, replan, NoProgress, NoPath, Deadline, patience, longest stall, TaskStatus, Failure]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@d683c9b clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/walk.rs#pub fn step(&mut self, w: &World, tick: u64) -> (Controls, TaskStatus) {"
  - "ruststeve:crates/plan/src/walk.rs#pub const PATIENCE_TICKS: u64 = 200;"
  - "ruststeve:crates/plan/src/walk.rs#fn follow(body: physics::Body, (wx, wy, wz): Cell) -> Controls {"
  - "clojurecraft:src/clojurecraft/intent.clj#(def stuck-ticks 40)                  ; no waypoint reached for 2 s → plan again"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[pathfinding]]"
  - "[[progress-invariants]]"
  - "[[expectations-and-watchdog]]"
---

# The walk and its watchdog

Issue 09a: clojurecraft's walk intent, made to fail fast when it stops getting closer ([[progress-invariants]]).

## How it works
- **Following** (`follow`): face the next waypoint's centre and go forward; jump only when the waypoint is above and within 1.3, or when blocked; stop once wholly over a descending waypoint. A waypoint is reached within 0.35 horizontally and 0.5 vertically.
- **Replanning** (clojurecraft's `route-stale?`): a waypoint ahead no longer standable, no waypoint reached for 40 ticks, or a short route walked out; at most 6 plans, then `NoPath`. Deadline 1,200 ticks plus 10 per waypoint.
- **Arriving**: for a log, the eye within 4.0 of its centre and the feet at most 2 below it; for an explore, the route's goal.
- **The progress invariant**: every tick the walk keeps the nearest it has come to the target (a gain under 0.25 does not count). No gain for **200 ticks** (10 game seconds) fails it `NoProgress`; the longest stall is kept and reported, so "no walk 60 game seconds without progress" is counted per run, not assumed.
- **Failures are data**: `NoPath`, `NoProgress`, `Deadline`, `TargetLost`; the planner refuses and chooses again ([[planner]]).

## Evidence
- Over 24 generated worlds the longest stall never passes 201 ticks; on the captured plains landing the walk reaches a log 30+ blocks out with no correction (`crates/bot/tests/planner.rs`).
- Live: a bot on the local 26.1.2 server walked to a log 30+ blocks away in about 13 s, no correction, its recording replaying with 0 mismatches.

## See also
- [[pathfinding]] - the routes.
- [[expectations-and-watchdog]] - the general mechanism.
