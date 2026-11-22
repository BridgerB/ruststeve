---
title: The planner
description: What the bot decides and why - the plan crate's aims (a log, or a goal row such as the kit), tasks (walks, digs, crafts) and choices (a held target from memory's column index, explore on a held heading), failure memory, the churn invariant, giving up, the decisions recorded as notes, and how the shell drives it.
type: reference
tags: [plan, planner, tasks, decisions, churn]
aliases: [Aim::Table, take_acts, met, Plan, Aim, Task, ToLog, Explore, PlanEvent, churn invariant, refused, area refused, stuck, max unwalkable, --goal, plan notes]
status: verified
lastUpdated: 2026-11-22
verifiedAgainst: ruststeve@2acc083 clojurecraft@be85a92
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
- **Giving up**: 12 failed walks or choices in a row with nothing done → `stuck`, said so; the run ends there (clojurecraft's `max-unwalkable`). A goal aim relocates instead (issue 13b, below).
- **Decisions as data**: every start, end and failure is a `PlanEvent` with its reason. The shell writes each into the recording as a note, `why` prints them up to the moment ([[replay-and-why]]), and the case file carries the summary: aim, reached, stuck, walks, failures by kind, the longest stall, expansions, the brain's time per tick ([[race-and-results]]).
- **Driving it**: `bot::run` steps the planner each tick once the start has settled and sends `Event::Controls` when they change; `bot::lockstep` takes the same planner as its controls.

- **Choosing a target needs a walkable route** (clojurecraft's `walkable?`): found, or partial only toward a column not loaded yet; an explore takes any route with waypoints. With no log walkable, explore is the fallback in the same choice.

## Always-rows (issue 12b)
Every tick, before its task and before checking whether the aim is met, the planner asks `Plan::always` whether a trap the table answers is in force: one cuts the task in hand and runs an escape until the trap clears ([[escapes]]). `Aim::Hold` has no goal: only the always-rows run. Issue 12d adds `defend` after the escapes ([[combat-and-pacing]]); issue 12c adds `eat` and `hunt` after it ([[food-and-hunting]]): each runs to its own end, an escape cuts either, and a running hunt is not cut by the meal.

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

## Issue 13b: a goal aim never stops
The iron-kit race's walls (docs/problems/iron-kit.md) are answered here; each is a structure, not a threshold.
- **Relocate**: a goal aim's 12 failed walks relocate it (at most 6): failures and refused areas are forgotten and it explores from elsewhere. After the sixth it rests 600 ticks and starts again; only the log aim stops for good (wall 19).
- **Explore fallbacks**: 48, then 16, then 8 blocks on each of four headings (a bot on a ledge or a shore has only a short way off, wall 7); then back along the **trail**, a mark every 200 ticks (60 kept), the newest at least 10 blocks off that a route reaches (wall 18).
- **Swim** (walls 24, 26): giving up with water beside the feet, or an explore that finds no walk and no trail mark beside water, swims to dry footing 10 to 32 blocks across (`Task::Swim`, the escape's own swimming, [[escapes]]), done on that shore or after 1,200 ticks.
- **Stalled areas** (wall 25): a walk that fails `NoProgress` refuses its target's area (6 blocks), for good after 3 stalls there until a relocation; `NoPath` refuses the area only a while (another log of the tree may still have a way).
- **Dangers** (wall 29): when a retreat from a `never_fight` mob ends, targets within `flee_clear` of where it was are refused 600 ticks ([[combat-and-pacing]]).
- **Opportunity**: a block a goal's provide (or a `wants` item) will need dug, exposed within 6 with a route, is dug now rather than walked back for (wall 9).
- **Dig toward**: a mine row whose nearest exposed block has no route digs toward it, with a 3-cut stall invariant (wall 11).
- **Watchdog**: no task done in 1,200 ticks ends the task in hand and plans again (wall 14).
- **Table search**: a remembered table or furnace counts as near within 64, so a deep miner walks back to its own rather than planning a new one from surface wood (wall 22).

## Issue 14
- `Aim::WaterBucket`, `Aim::PortalKit` (the scoop task, [[buckets-and-fluids]]) and `Aim::Portal`: the kit held, then `Task::Cast` at a site in hand or chosen now, else stairs toward `LAVA_DEPTH`; a failed cast gives its site up ([[portal-cast]]).

## Issue 15
- The `fortress` aim (`Aim::Fortress`, a goal aim for its table and relocation): met when the feet are within `FORTRESS_NEAR` 6 of a nether-brick cell on record (`world::stage::at_fortress`); a walk to the nearest brick seen only on a found route, else `dig_toward` it (stairs down when below, a level tunnel else, three stalls refuse it), else explore. `dig_toward` is shared with the water bank's stairs (portal wall 40). Walking on partial routes across the Nether killed half the bots (fortress wall 3).

## See also
- [[phases-and-task-stack]] - the design this begins.
- [[walk-and-watchdog]] - the task it runs.

## Blaze wall 7: worn tools and the dig budget
- A tool with fewer than `spare_uses` (40) uses left is worn and no spare (`make::without`, clojurecraft's `without-worn`).
- `dig_budget`: every held pickaxe's uses left, plus 131 for each stone pickaxe the held wood and stone make with a table held or near. Under `travel_reserve_digs` (131) a spare's gather is worth it from anywhere (`make::Wear::low_on_digs`), so a bot underground turns to the trees while it can still dig to them; the ledger notes "low on digs" once.
