---
title: The walk and its watchdog
description: How ruststeve walks a route - clojurecraft's walk intent (waypoints, follow, replanning) plus the progress invariant that fails a walk which stops getting closer, the deadline, and the failures as data.
type: reference
tags: [plan, walk, progress, watchdog, invariants]
aliases: [Walk, walk task, follow, waypoint, replan, NoProgress, NoPath, Deadline, patience, longest stall, TaskStatus, Failure]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@a9a6129 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/walk.rs#pub fn step(&mut self, w: &World, tick: u64) -> (Controls, TaskStatus) {"
  - "ruststeve:crates/plan/src/walk.rs#pub const PATIENCE_TICKS: u64 = 200;"
  - "ruststeve:crates/plan/src/walk.rs#fn follow(body: physics::Body, (wx, wy, wz): Cell, on_soul_sand: bool) -> Controls {"
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
- **The bridge's executor** (`Walk::bridge`): at a waypoint reached by `MoveKind::Bridge` with no floor yet, the body stays within `BRIDGE_STAND` of the centre of the block it stands on, holds a bridge block (`hold::toward`) and clicks the side of the block under its feet (`Act::UseOn`, every `PLACE_EVERY_TICKS`, `MAX_PLACE_TRIES` times); the floor seen, it walks on. It never steps over a floor not laid: a bridge it cannot lay fails the walk `NoPath`. Each block laid is a ledger note, "bridge block laid at …".
- **A walk's acts and notes** are left in `Walk::more` and taken by the planner each tick (`take_out`).
- **A far walk** (`Walk::far`) searches with `FAR_NODES` at `FAR_WEIGHT`; each search after its first is a note. On its found route (a far walk only) a waypoint reached counts as progress, so a route that leads round is not failed `NoProgress`; every other walk keeps the plain invariant (on every walk it broke the overworld's log walk: fix 6 of `docs/problems/nether-walk.md`).
- **`follow` on soul sand** (the feet's cell is soul sand, read from the block): a rise of a step or less is stepped, never jumped, and "a cell up" is read a block higher (feet at y + 0.875). Off soul sand it is clojurecraft's `follow`, line for line (fix 6 of `docs/problems/nether-walk.md`).
- **The fortress leg's order** (`fortress_task`): a found plain route; on rock (`rock_to_tunnel`: two full blocks under the feet, no lava at the door) the travel tunnel, then the far walk; with no rock to tunnel, the far walk first. The far walk's target is the nearest brick in a fortress's body (`brick_in_the_body`: 200 brick columns in the nine 16-block squares round it), before a nearer lone pillar. `Walk::far_to` serves the planner's other legs.
- **Raced** (45337580375, the Nether snapshot, 3 x 1): two bots at `fortress` at ticks 1,600 and 1,720 with 20 health, 162 and 172 blocks laid; the third killed by piglins on land before its far walk.
- **A heading's walk** (issue 17d, docs/problems/heading.md): `More::keep` is the blocks its far search keeps unspent before it bridges (`None` is `BRIDGE_RESERVE`), and `More::blocked` the cells its searches never enter (the heading's cut). Blocked cells are read by the searches only, not by the per-tick staleness check.

## Issue 19: sneak at a lava edge (docs/problems/eyes-home.md wall 21)
- **`lava_edge`**: a side of a cell whose floor neighbour is lava, or open with lava within `LAVA_DROP` 8 under it. At such an edge (the feet or the next waypoint, never on a step down, which a sneaking body is held back from) the walk and its bridge move hold sneak (`edge_sneak`): the server backs a sneaking body off an edge, its own step or a knock-back (`physics::back_off_from_edge`). Its ticks (`More::sneak_ticks`) are added to the deadline (sneaking is 0.3 of a walk); "walk: sneak at a lava edge from …" once a walk. An idle body at a lava edge sneaks too (the planner's `Task::Idle`). In the Nether a touch of lava is a death: no water puts the fire out (race 45742350492 lane 3).

## Issue 20e: the void is an edge (docs/problems/end-island.md)
In the End a side whose floor neighbour and every cell under it to the dimension's floor are loaded and clear (`end::void_edge`, `end::void_column`) is an edge as lava is: the walk sneaks there, "walk: sneak at a void edge from …" once a walk. Unloaded cells under it are not a known void. The End's bridge is its own ([[end-island]]), not the far walk's bridge move.

## Issue 56: the walk asks to sprint
- `toward` (and so `follow`) holds the sprint key; the pace gate ([[sprint]], `plan::sprint`) keeps it where the food and the ground allow. A bridge or a sneak at an edge clears it and sets `More.step`, which `Walk::pace` hands the gate for its "sprint refused" note. `Walk::left` is the route still to go.

## See also
- [[pathfinding]] - the routes.
- [[expectations-and-watchdog]] - the general mechanism.
