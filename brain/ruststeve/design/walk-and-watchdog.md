---
title: The walk and its watchdog
description: How ruststeve walks a route - clojurecraft's walk intent (waypoints, follow, replanning) plus the progress invariant that fails a walk which stops getting closer, the deadline, and the failures as data.
type: reference
tags: [plan, walk, progress, watchdog, invariants]
aliases: [Walk, walk task, follow, waypoint, replan, NoProgress, NoPath, Deadline, patience, longest stall, TaskStatus, Failure]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@3c1b500 clojurecraft@be85a92
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
- **Arriving**: for a log to reach (issue 09), the eye within 4.0 of its centre and the feet at most 2 below it; for a log to dig (`to_dig`, issue 10c), beside it by the `Pickup` goal ([[digging]]); for an explore, the route's goal.
- **The progress invariant**: every tick the walk keeps the nearest it has come to the target (a gain under 0.25 does not count). No gain for **200 ticks** (10 game seconds) fails it `NoProgress`; the longest stall is kept and reported, so "no walk 60 game seconds without progress" is counted per run, not assumed.
- **Failures are data**: `NoPath`, `NoProgress`, `Deadline`, `TargetLost`; the planner refuses and chooses again ([[planner]]).

## Evidence
- Over 24 generated worlds the longest stall never passes 201 ticks; on the captured plains landing the walk reaches a log 30+ blocks out with no correction (`crates/bot/tests/planner.rs`).
- Live: a bot on the local 26.1.2 server walked to a log 30+ blocks away in about 13 s, no correction, its recording replaying with 0 mismatches.

## Since 09b
`route_now` decides whether a target is walkable before the planner chooses it: a found route, or a partial one toward a column not loaded yet; an explore takes any route with waypoints ([[planner]]).

## Issue 16: bridges and far walks (docs/problems/nether-walk.md)
- **The bridge's executor** (`Walk::bridge`): at a waypoint reached by `MoveKind::Bridge` with no floor yet, the body stays within `BRIDGE_STAND` of the centre of the block it stands on, holds a bridge block (`hold::toward`) and clicks the side of the block under its feet (`Act::UseOn`, every `PLACE_EVERY_TICKS`, `MAX_PLACE_TRIES` times); the floor seen, it walks on. It never sneaks (the protocol sends no sneak input) and never steps over a floor not laid: a bridge it cannot lay fails the walk `NoPath`. Each block laid is a ledger note, "bridge block laid at …".
- **A walk's acts and notes** are left in `Walk::more` and taken by the planner each tick (`take_out`).
- **A far walk** (`Walk::far`) searches with `FAR_NODES` at `FAR_WEIGHT`; each search after its first is a note. On a found route a waypoint reached counts as progress, so a route that leads round is not failed `NoProgress`.
- **`follow` jumps** only for a waypoint more than a step (`physics::STEP_HEIGHT`) over the feet: from soul sand the next cell is an eighth up.
- **The fortress leg's order** (`fortress_task`): a found plain route; on rock (`rock_to_tunnel`: two full blocks under the feet, no lava at the door) the travel tunnel, then the far walk; with no rock to tunnel, the far walk first. The far walk's target is the nearest brick in a fortress's body (`brick_in_the_body`: 200 brick columns in the nine 16-block squares round it), before a nearer lone pillar. `Walk::far_to` serves the planner's other legs.
- **Raced** (45337580375, the Nether snapshot, 3 x 1): two bots at `fortress` at ticks 1,600 and 1,720 with 20 health, 162 and 172 blocks laid; the third killed by piglins on land before its far walk.

## See also
- [[pathfinding]] - the routes.
- [[expectations-and-watchdog]] - the general mechanism.
