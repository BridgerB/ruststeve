---
title: The goal table and the needs planner
description: How ruststeve decides what to make - config/goals.toml's goal and producer rows, one generated row per 2x2 recipe, plan::make (a port of clojurecraft's make.clj item needs) re-derived from the inventory every time the planner is idle, and the differential test against clojurecraft's own answers.
type: reference
tags: [plan, goals, needs, make, crafting]
aliases: [block needs, holds, place-table, mine-stone, stone-tools, goals.toml, goal table, needs planner, plan::make, make, producer rows, craft rows, kit, wood, Aim::Table, --goal table, producer order, craft-now, make differential]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@7e42e00 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/make.rs#pub fn make<'a>("
  - "ruststeve:crates/plan/src/make.rs#fn producer_order(counts: &Counts, row: &Row) -> (i32, i32) {"
  - "ruststeve:crates/plan/src/make.rs#fn craft_now("
  - "ruststeve:config/goals.toml#id = \"kit\""
  - "ruststeve:crates/plan/tests/make_differential.rs#fn the_needs_planner_chooses_what_clojurecraft_chooses() {"
  - "clojurecraft:src/clojurecraft/make.clj#(defn resolve-item-need"
  - "clojurecraft:src/clojurecraft/make.clj#(defn producer-order"
  - "clojurecraft:resources/clojurecraft/goals.edn#:goal/doc \"four sticks, then a crafting table, from logs\""
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[planner]]"
  - "[[crafting-window]]"
  - "[[digging]]"
  - "[[mc-recipes]]"
---

# The goal table and the needs planner

Issue 10c. Goals are data; planning is a function. A goal row says what it provides, a producer row how a need is met, and `plan::make` walks back from the goal to the first act whose needs are held.

## Key files
- `config/goals.toml`: the rows, read by the shell (`bot::goals_table`) into `plan::make::Table`.
- `crates/plan/src/make.rs`: `Need`, `Row`, `Table`, `make`.
- `crates/plan/tests/make_differential.rs` and `tests/fixtures/make-expected.txt`.

## How it works
- **Rows**: `wood` (`tag:logs 1`, `--goal log`'s row in the table), `kit` (`item:stick 4`, then `item:crafting_table 1`; sticks first, clojurecraft's issue #104), and the producer `gather` (`tag:logs 1`, act `gather`). `Table::new` adds one producer per recipe that fits the 2x2 grid (`craft:<recipe id>`), its needs the recipe's ingredient sets counted, its provide the result; a need is named as clojurecraft names it (the shortest item tag with exactly that set, else the item).
- **`make(table, counts, goal)`** → `Done`, `Act(row)` or `Stuck`, from what is held (window 0 from slot 1 and the cursor, `plan::held`): each provide in order is resolved against one netted working inventory (`consume` takes items in name order, as `recipe/consume`); a need not held is resolved through its producers, tried in clojurecraft's `producer-order` (most of their own needs already covered, then larger yield; ties keep table then recipe order) for as many runs as the rest needs, recursively, to depth 12 with cycles cut. `craft-now`: a craft asked for several runs that would first send the bot out, but one run of which is held, is crafted first.
- **Nothing stored**: the planner calls `make` each time it is idle; a log consumed or lost is simply planned for again.
- **`Aim::Table`** (`--goal table`): the planner's idle choice is `make` toward the row whose `until` is `table`. `Act(gather)` chooses the nearest walkable log in view (or explores), dug from beside it ([[digging]]); `Act(craft:…)` runs one craft ([[crafting-window]]); `Done` sets `met`.

## Issue 11b: block needs and holds
- **Rows**: `pickaxe`, `cobblestone`, `stone-pickaxe` (clojurecraft's) and `stone-tools` (a stone pickaxe, then a sword: issue 11's scope); producers `place-table` (needs a held table, provides `block:crafting_table`) and `mine-stone` (holds a wooden pickaxe, provides cobblestone; act `mine`, [[stairs-down]]).
- **Block needs** (`resolve-block-need`): `block:<name>` is met when one is remembered near (a table within 24 of the eye, from memory, refused only by its own failures), else by the first producer that provides it, once. A recipe that does not fit the 2x2 grid needs `block:crafting_table`, after its ingredients.
- **Holds** (`resolve-holds`): what a row keeps (a tool) is resolved once and handed back unconsumed, so three cobblestone need one pickaxe.
- The planner turns a 3x3 craft into a craft at the remembered table ([[crafting-window]]), `place` into the place task ([[placing]]), and before a fetching act (mine, gather) digs a table in reach that is not held, to carry it on (`take-table`).

## Always-rows (issue 12b)
`[[always]]` rows (`id`, `priority`, `doc`, `trap`: water, lava or fire) are kept apart from goals and producers in `Table::always`, by priority: they are not planned for, they preempt ([[escapes]]). Issue 12c's rows name an `act` (`eat`, `hunt`) instead of a `trap`, and `[survival]` holds their numbers ([[food-and-hunting]]).

## Issue 13b: fuel, smelting and spares
Smelt rows, the `fuel:ticks` need and `spares` are in [[fuel-and-smelt-needs]], with their own differential (12,288 of 12,288).

## Checked against clojurecraft
`make-expected.txt` is clojurecraft's own `make/next-row` (at be85a92, a one-off oracle in its flake shell, outside this repo) toward the kit with a table of the kit and gather-log rows, for all 480 inventories of oak logs 0-3, planks 0-4, sticks 0-5 and tables 0-1: **480 of 480** choices equal. `make-expected-stone.txt` (issue 11b): toward `stone-pickaxe` with the stone-pickaxe, gather-log, place-table and mine-stone rows, 1,920 inventories of logs, planks, sticks, tables, cobblestone and wooden pickaxes, with and without a remembered table: **1,920 of 1,920** equal.

## Limits
- Not ported yet: spares, fuel and smelting; their issues add them.
- Ties among producers follow the generated recipe order, where clojurecraft follows its set order; the differential covers the inventories the kit meets.

## See also
- [[planner]] - where `make` is called.
- [[mc-recipes]] - the recipes it generates rows from.
