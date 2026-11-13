---
title: Fuel and smelt needs
description: How the needs planner smelts (issue 13b, a port of clojurecraft's make.clj fuel and smelt parts) - a smelt row per furnace recipe, the fuel:ticks need burned in the smelt's own order and planned from coal or planks, spares, the furnace as a block need, batches at the furnace and the batch the planner goes back for, and the differential against clojurecraft over 12,288 inventories.
type: reference
tags: [plan, make, smelting, fuel, needs, spares]
aliases: [fuel:ticks, smelt rows, burn order, wood covers, log reserve, spares, make_with, gather_now, batch, collect, iron-kit, ingots, furnace row, make-expected-iron]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@7e42e00 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/make.rs#fn resolve_fuel(&self, counts: Counts, n: i32, path: &[Vec<ItemKind>]) -> Resolved<'a> {"
  - "ruststeve:crates/plan/src/make.rs#pub fn burn_order() -> Vec<ItemKind> {"
  - "ruststeve:crates/plan/src/lib.rs#fn keep_batch(&mut self) {"
  - "ruststeve:config/goals.toml#id = \"iron-kit\""
  - "ruststeve:crates/plan/tests/make_differential.rs#fn fuel_smelting_and_spares_choose_what_clojurecraft_chooses() {"
  - "ruststeve:crates/bot/tests/iron.rs#fn from_the_stone_kit_to_the_iron_kit() {"
  - "clojurecraft:src/clojurecraft/make.clj#(defn resolve-fuel-need"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[goal-table-and-needs]]"
  - "[[smelting]]"
  - "[[veins-and-flint]]"
---

# Fuel and smelt needs

Issue 13b. Iron is the first item the planner gets from a furnace: its needs reach through fuel and a placed furnace as well as through items.

## Key files
- `crates/plan/src/make.rs`: `Need::fuel`, `Act::Smelt`, the generated smelt rows, `burn`, `wood_covers`, `resolve_fuel`, spares, `make_with`.
- `config/goals.toml`: `furnace`, `ingots`, `iron-pickaxe`, `bucket`, `flint-and-steel`, `iron-kit`; `place-furnace`, `mine-coal`, `mine-iron`, `dig-gravel`, `flint-from-gravel`.
- `crates/plan/src/lib.rs`: `decide`'s smelt, the batch (`Plan::batch`, `keep_batch`).
- Tests: `make_differential.rs` (`make-expected-iron.txt`), `crates/bot/tests/iron.rs`.

## How it works
- **Smelt rows** (`smelt-row`): one producer per furnace recipe: its input, then `fuel:ticks` for one cook (after the input, so the input is never its own fuel), then `block:furnace`; it provides the result. A furnace near comes from memory (a placed furnace is watched); else `place-furnace`, and the furnace is crafted at a table from 8 cobblestone.
- **The fuel need** (`resolve-fuel-need`): met when the fuel held burns the ticks, taken out in the smelt's own burn order (the `coals` tag, then the `planks` tag, each by name and only what burns; never logs, sticks or tools); else the rest is planned from coal (mine-coal), then planks, planks first when the logs held, less a reserve of one (a new pickaxe's wood), make them (`wood-covers?`).
- **Spares** (`resolve-spares`): a row keeps one more of each `spares` item than it holds, made only when the producer is worth it now: never a mine, a gather only when a log can be started (`make_with`'s `gather_now`, the world's nearest log). `mine-iron` spares a stone pickaxe and 24 sticks; `ingots` 24 sticks.
- **Batching**: a smelt act cooks all of the input held (to 64): one trip to the furnace for the ore. A smelt cut with its batch loaded (an always-row) leaves `Plan::batch`, and the next decision goes back to collect it before anything else, so the planner never mines for iron already cooking (clojurecraft's iron-kit wall).
- `iron-kit` (issue 13's measure) provides an iron pickaxe, a bucket and flint and steel; the water bucket is the portal's (issue 14).

## Checked against clojurecraft
`make-expected-iron.txt` is clojurecraft's own `make/next-row` (at be85a92, a one-off oracle in its flake shell, outside this repository) toward the kit, with clojurecraft's eight producer rows, for 12,288 inventories (logs, planks, sticks, cobblestone, coal, raw iron, ingots, gravel, flint, tables, always a stone pickaxe; with and without a table and a furnace near; no log in view): **12,288 of 12,288** equal.

## Evidence
- Model: from the `stone` start's kit on a flat world with ore walls, the kit in 4,753 ticks (table, three iron ore as a vein, coal, stairs down for stone, a furnace crafted and placed, three batches, flint from gravel).

## Limits
- Worn tools are not kept out of spares (the world does not read tool damage yet).
- A smelt batches every input held, whether all of it is wanted or not.

## See also
- [[smelting]] - the furnace and the smelt task.
- [[veins-and-flint]] - mining ore and flint.
