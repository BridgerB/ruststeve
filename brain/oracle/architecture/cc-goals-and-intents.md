---
title: Goals and intents
description: How clojurecraft decides what to do (a goal table as data, a planner re-deriving the goal every tick, intents as values advanced one tick at a time), and the goal list in run order.
type: reference
tags: [oracle, planner, goals, intents]
aliases: [goals.edn, goal table, intent, next-intent, :goal/priority, always rows, superseded goals]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/plan.clj#registers the needs planner. Every tick the planner re-derives the current goal from the"
  - "clojurecraft:resources/clojurecraft/goals.edn#;; A row is never edited into a different meaning or deleted: to fix one, add the fixed row under"
  - "clojurecraft:src/clojurecraft/intent.clj#\"Intents are values: {:intent/kind k :intent/status :active|:done|:failed ...}. The planner"
  - "clojurecraft:resources/clojurecraft/goals.edn#:goal/doc \"out of water onto dry land, before anything else\""
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-plan-and-make]]"
  - "[[phases-and-task-stack]]"
---

# Goals and intents

clojurecraft has no phase enum and no step objects. What it wants is a **goal table** of data rows; what it is doing is one **intent**, a value advanced by one tick at a time.

## Key files
- `resources/clojurecraft/goals.edn`: the table, hand-written.
- `src/clojurecraft/plan.clj`: `goals`, the registries `done-by`, `act`, `next-intent`, `interrupts?`, and `plan-tick`.
- `src/clojurecraft/intent.clj`: `run` (a multimethod on `:intent/kind`), `still-wanted?`, child intents.

## How it works
- **A goal row**: `:goal/id`, `:goal/priority`, `:goal/needs` and `:goal/provides` (`:item/<name> n`, `:tag/<tag> n`, `:block/<name> :near`), `:goal/done?` (the name of a registered predicate), `:goal/act` or `:goal/plan` (the name of a registered act or planner), plus watchdog knobs. `:goal/target? true` marks what a run is for; `--until <name>` puts one in play.
- **Always rows** (priority 0, `:goal/always? true`) preempt everything: `:escape-water`, `:escape-lava`, `:escape-fire`, `:defend`, then eating, healing, hunting, armour.
- **The run's targets**, in table order: `:wood`, `:kit`, `:pickaxe`, `:cobblestone`, `:stone-pickaxe`, `:furnace`, `:ingots`, `:bucket`, `:water-bucket`, `:flint-and-steel`, `:iron-kit`, `:portal-kit`, `:gear`, `:cook`, `:cast-one`, `:portal`, `:nether`, `:fortress`, `:blaze-rods`, `:spare-picks`, `:ender-pearls`, `:eyes-of-ender`, `:stronghold`, `:portal-room`, `:end-portal`, `:enter-end`, `:crystals`, `:dragon`, plus producer rows (gather a log, mine stone, coal, iron, gravel, fill a bucket, place a table or furnace).
- **Every tick** `plan-tick` re-derives the goal in play from the world: no target left → done; a running intent that serves another target than the one chosen now is preempted; a row that now outranks it or whose `interrupts?` fires cuts it; otherwise the intent runs, or the chosen goal's next intent starts.
- **Intents** are values (`{:intent/kind :dig :intent/target [x y z] ...}`); every deadline is an absolute ms value; a new kind is a `defmethod` of `run` in a new namespace. Intents compose: a staircase runs child `:dig` intents.
- **Accretion**: a row is never edited into a different meaning or deleted; the fix is a new id and `:goal/superseded-by`, so old recordings still replay.

## For ruststeve
Goal rows map to the guide's phases and task stack loosely: always rows are the recovery and survival layer, target rows are phases, intents are tasks ([[phases-and-task-stack]]). The table should stay data (a `const` table of rows or a data file) with exhaustive enums for act and predicate names, not trait objects.

## See also
- [[cc-plan-and-make]] - the needs planner that walks from a target to the next act.
- [[cc-roadmap-stages]] - each stage's problem statement.
