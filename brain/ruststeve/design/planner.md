---
title: The planner
description: What the bot decides and why - the plan crate's aim, tasks and choices (a held target from memory's column index, explore on a held heading), failure memory, the churn invariant, giving up, the decisions recorded as notes, and how the shell drives it.
type: reference
tags: [plan, planner, tasks, decisions, churn]
aliases: [Plan, Aim, Task, ToLog, Explore, PlanEvent, churn invariant, refused, area refused, stuck, max unwalkable, --goal, plan notes]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@e2f14d7 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/lib.rs#pub fn step(&mut self, w: &World) -> Controls {"
  - "ruststeve:crates/plan/src/lib.rs#fn choose(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {"
  - "ruststeve:crates/plan/src/lib.rs#fn nearest_log(&self, w: &World, origin: [f64; 3], min: f64, tick: u64) -> Option<Cell> {"
  - "ruststeve:crates/bot/tests/planner.rs#fn over_generated_worlds_it_never_churns_and_never_stalls_past_its_patience() {"
  - "clojurecraft:src/clojurecraft/plan.clj#(def max-unwalkable 12)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[phases-and-task-stack]]"
  - "[[walk-and-watchdog]]"
  - "[[progress-invariants]]"
---

# The planner

Issue 09a begins the `plan` crate (core): `Plan::step(&World) -> Controls`, the world read, never changed.

## How it works
- **One aim so far**: `ReachLog { min_distance }` (`--goal log` or `log:<min>`), the checkpoint start's measure for issue 09.
- **Tasks** (`Task`, an enum): `Idle`, `ToLog(Walk)`, `Explore(Walk)`. A target is held until the walk ends, never re-chosen per tick.
- **Choosing**: the nearest log *sighting* (memory: seen with an exposed face, so no X-ray) at least `min_distance` from where the planner began, found by a ring query over memory's chunk-column index, never a scan; else explore 48 blocks out on a held heading, turned a quarter after each failure.
- **The churn invariant** (clojurecraft's `churn_guard`): every candidate is routed before it is chosen, so a chosen task always has a way and is not over after one tick; at most 2 candidates a tick. A test checks it over generated worlds.
- **Failure memory**: a failed target is refused 600 ticks, for good after 3 failures; a `NoPath` refuses every target within 6 blocks (one unreachable tree is not tried log by log). After a failure the planner waits 20 ticks.
- **Giving up**: 12 failed walks or choices in a row with nothing done → `stuck`, said so; the run ends there (clojurecraft's `max-unwalkable`).
- **Decisions as data**: every start, end and failure is a `PlanEvent` with its reason. The shell writes each into the recording as a note, `why` prints them up to the moment ([[replay-and-why]]), and the case file carries the summary: aim, reached, stuck, walks, failures by kind, the longest stall, expansions, the brain's time per tick ([[race-and-results]]).
- **Driving it**: `bot::run` steps the planner each tick once the start has settled and sends `Event::Controls` when they change; `bot::lockstep` takes the same planner as its controls.

## Evidence
- Captured parity landings, 3,000 ticks each: every dry landing with a log 30+ blocks away in view but one reached it; the others started in or across water, where routes see liquids as walls (issue 12 adds swimming).

## Limits
- One aim, no phases yet: digging the log (issue 10) and everything after add rows.

## See also
- [[phases-and-task-stack]] - the design this begins.
- [[walk-and-watchdog]] - the task it runs.
