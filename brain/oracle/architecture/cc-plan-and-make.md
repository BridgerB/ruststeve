---
title: The needs planner
description: How clojurecraft gets from a target to the next concrete act (walking back through producer rows and generated recipe rows from the inventory, every tick), so a lost item is simply planned for again.
type: reference
tags: [oracle, planner, crafting, needs]
aliases: [make.clj, needs planner, producer rows, recipe rows, provided?, netting]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/make.clj#n is fuel that burns n game ticks (a smelt's), counted from what the inventory would burn."
  - "clojurecraft:src/clojurecraft/make.clj#producer rows - the table's own (gather a log, place a table) and one generated row per"
  - "clojurecraft:src/clojurecraft/make.clj#inventory and the world, so a lost or consumed item is simply planned for again; needs are"
  - "clojurecraft:brain/bot/decisions/one-planner.md#Why the recipe-only graph walk (recipe/next-action) was removed once the needs planner in make covered everything it did, and what each was."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-goals-and-intents]]"
  - "[[mc-recipes]]"
---

# The needs planner

`make.clj` turns "hold a stone pickaxe" into "dig this log now". It is re-derived every tick from the inventory and the world, so nothing about progress is stored.

## Key files
- `src/clojurecraft/make.clj`: the `:needs` planner, the `:provided?` predicate, the `:gather`, `:place`, `:craft`, `:smelt` acts, `craft-rows`.
- `resources/clojurecraft/recipes.edn`, `smelting.edn`, `fuel.edn`: generated from the jar.

## How it works
- A **need** is a key and an amount: `:item/<name> n` and `:tag/<tag> n` counted in the inventory; `:block/<name> :near` is a remembered block a walk reaches; `:fuel/ticks n` is fuel that burns that many game ticks, counted from what the inventory would burn.
- A goal is met when its provides are held. When it is not, the planner walks back through producer rows (the table's own: gather a log, place a table, mine iron) and one **generated row per recipe and per furnace recipe**, until it reaches a row whose needs are all met, and acts on it.
- Needs are netted against one working inventory, so two needs never count the same item twice.
- `:goal/holds` are needs a row keeps (a tool), planned once, not spent per run.

## For ruststeve
This is a pure function `(world, table) -> act` and ports as one: a recipe graph built once from generated data, a walk over it per tick. It is a prime differential-test target: same inventory in, same chosen act out ([[testing-kinds]]).

## Gotchas
- An earlier recipe-only graph walk was removed once the needs planner covered everything; clojurecraft has one planner, not two.
- A loaded furnace batch is not in the inventory, so the planner would mine for iron already cooking. clojurecraft remembers a loaded batch (`:furnace/batch`) for exactly this ([[cc-roadmap-stages]], iron kit).

## See also
- [[cc-goals-and-intents]] - the table this walks.
- [[mc-recipes]] - recipe facts.
