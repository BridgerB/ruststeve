---
title: The planner
description: What the bot decides and why - the plan crate's aims (a log, or a goal row such as the kit), tasks (walks, digs, crafts) and choices (a held target from memory's column index, explore on a held heading), failure memory, the churn invariant, giving up, the decisions recorded as notes, and how the shell drives it.
type: reference
tags: [plan, planner, tasks, decisions, churn]
aliases: [Aim::Table, take_acts, met, Plan, Aim, Task, ToLog, Explore, PlanEvent, churn invariant, refused, area refused, stuck, max unwalkable, --goal, plan notes]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@3fa1b05 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/lib.rs#pub fn step(&mut self, w: &World) -> Controls {"
  - "ruststeve:crates/plan/src/lib.rs#fn choose(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {"
  - "ruststeve:crates/plan/src/lib.rs#fn nearest_log(&self, w: &World, origin: [f64; 3], min: f64, tick: u64) -> Option<Cell> {"
  - "ruststeve:crates/bot/tests/planner.rs#fn over_generated_worlds_it_never_churns_and_never_stalls_past_its_patience() {"
  - "ruststeve:crates/bot/src/landings.rs#pub fn walkable_log(w: &world::World, start: [f64; 3], min: f64) -> bool {"
  - "ruststeve:config/landings-grid.toml#on the race seed, 192 blocks apart."
  - "clojurecraft:src/clojurecraft/plan.clj#(def max-unwalkable 12)"
  - "ruststeve:crates/plan/src/lib.rs#fn decide(&mut self, w: &World, origin: [f64; 3], tick: u64) -> Task {"
  - "ruststeve:crates/plan/src/lib.rs#pub fn take_acts(&mut self) -> Vec<world::Act> {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[phases-and-task-stack]]"
  - "[[walk-and-watchdog]]"
  - "[[progress-invariants]]"
---

# The planner

Issue 09a begins the `plan` crate (core): `Plan::step(&World) -> Controls`, the world read, never changed.

## How it works
- **Aims**: `ReachLog { min_distance }` (`--goal log` or `log:<min>`), issue 09's measure; goal rows decided each idle tick by the needs planner ([[goal-table-and-needs]]): `Table` (`--goal table`, issue 10's kit), and issue 11's `pickaxe`, `cobblestone`, `stone-pickaxe`, `stone-tools`.
- **Tasks** (`Task`, an enum): `Idle`, `ToLog(Walk)`, `Explore(Walk)`, since issue 10 `Dig` ([[digging]]) and `Craft` ([[crafting-window]]), since issue 11 `Place` ([[placing]]) and `Stairs` ([[stairs-down]]). Under a goal aim a walk to a block (a log, exposed stone) ends beside it and becomes a dig.
- **Acts**: a dig or a craft makes `world::Act`s; the shell takes them with `take_acts` after each step and sends them as events (recorded, replayed), in `bot::run`, lockstep and the landing measure. A target is held until the walk ends, never re-chosen per tick.
- **Choosing**: the nearest log *sighting* (memory: seen with an exposed face, so no X-ray) at least `min_distance` from where the planner began, found by a ring query over memory's chunk-column index, never a scan; else explore 48 blocks out on a held heading, turned a quarter after each failure.
- **The churn invariant** (clojurecraft's `churn_guard`): every candidate is routed before it is chosen, so a chosen task always has a way and is not over after one tick; at most 2 candidates a tick. A test checks it over generated worlds.
- **Failure memory**: a failed target is refused 600 ticks, for good after 3 failures; a `NoPath` refuses every target within 6 blocks (one unreachable tree is not tried log by log). After a failure the planner waits 20 ticks.
- **Giving up**: 12 failed walks or choices in a row with nothing done → `stuck`, said so; the run ends there (clojurecraft's `max-unwalkable`).
- **Decisions as data**: every start, end and failure is a `PlanEvent` with its reason. The shell writes each into the recording as a note, `why` prints them up to the moment ([[replay-and-why]]), and the case file carries the summary: aim, reached, stuck, walks, failures by kind, the longest stall, expansions, the brain's time per tick ([[race-and-results]]).
- **Driving it**: `bot::run` steps the planner each tick once the start has settled and sends `Event::Controls` when they change; `bot::lockstep` takes the same planner as its controls.

- **Choosing a target needs a walkable route** (clojurecraft's `walkable?`): found, or partial only toward a column not loaded yet; an explore takes any route with waypoints. With no log walkable, explore is the fallback in the same choice.

## The measure (issue 09, the `walk` job in ci.yml)
- **Landings**: `config/landings-grid.toml`, a 20 × 16 grid on the race seed, 192 blocks apart, captured byte for byte (radius 4) from a fresh server; cached in CI by the list's hash.
- **Placed as vanilla places a player**: on the ground under leaves, with two free cells, within 8 blocks of the landing (`ModelWorld::ground`, `placeable`); a landing whose own spot is water is excluded.
- **Qualifying** (Bridger, 2026-11-12): a log 30+ blocks from the start is in view once the bot has joined, **and some walking route to one exists**, pre-checked with a 200,000-node search over the 40 nearest (`walkable_log`), as clojurecraft pre-checked its landing sets. Landings with a log in view but none walkable are counted every run and wait for issues 11 and 12 ([docs/problems/walk.md](../../../docs/problems/walk.md)).
- **Done-conditions**, checked by `simrace --check true`: at least 95% of the first 200 qualifying landings reach a log (within reach, eye to centre ≤ 4.0); no walk 1,200 ticks without progress; the planner's step per tick within `config/race.toml`'s `[budget]` (mean 500 µs, p99 5 ms).

## Evidence
- Local measure (2026-11-12): **200/200** qualifying landings reached a log; **0** walks past 1,200 ticks without progress (longest 201); planner CPU **mean 22 µs, p99 74 µs**. Raw, every landing with any log in view: 216/231 (93.5%); the 15 misses had no walking route to any log in view.
- Getting there, each cause named from traces: starts on a tree canopy (14 of the first 22 misses), starts with leaves at head height, targets chosen on a partial route down a cave or up a cliff, and no explore once every log was refused. First full measure 176/200; then 178, 183, 187, 200 as each was fixed.

## Limits
- Two aims, no phases yet: stone and tools (issue 11) and everything after add rows.

## See also
- [[phases-and-task-stack]] - the design this begins.
- [[walk-and-watchdog]] - the task it runs.
