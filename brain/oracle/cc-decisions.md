---
title: clojurecraft's decisions
description: The decisions clojurecraft recorded in its own brain, one line each with whether ruststeve inherits it, so none is re-litigated by accident.
type: reference
tags: [oracle, decisions, index]
aliases: [clojurecraft decision notes, inherited decisions]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:brain/bot/decisions/predict-nothing.md#Why every container click claims no changed slots and an empty cursor, so the server answers each click with authoritative slots and the bot never models a predicted window."
  - "clojurecraft:brain/bot/decisions/placement-judged-by-server.md#Why :place never writes the placed block locally and counts as done only when the server's block-update shows it, failing :rejected when the ack arrives without it."
  - "clojurecraft:brain/bot/decisions/sets-over-arrays.md#Why block predicates are plain sets of state ids and not primitive boolean arrays, with the measurement over a recorded run, and the rule for type hints that came with it."
  - "clojurecraft:brain/bot/decisions/natural-tree-not-fixture.md#Why CI and local runs chop a real tree in a normal world instead of a setblock log on a flat world."
related:
  - "[[oracle/_moc|Oracle]]"
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[cc-lessons]]"
---

# clojurecraft's decisions

clojurecraft keeps its decisions in `brain/bot/decisions/`. Each is a constraint a reasonable porter would otherwise rediscover. Read the original before overriding one.

| Decision | One line | ruststeve |
|---|---|---|
| `predict-nothing` | clicks claim no changed slots and an empty cursor | inherit ([[wire-container-click]]) |
| `placement-judged-by-server` | a placed block is real only when the server's block update shows it | inherit (an expectation) |
| `manual-clicks-not-place-recipe` | crafts are laid by computed clicks, not the recipe-book packet | inherit |
| `one-item-per-cell` | one item per grid cell, craft once, one verified shift-click | inherit |
| `one-stack-per-ingredient` | each ingredient from one chosen stack, several only when none covers it | inherit |
| `grid-in-its-own-attribute` | window-0 grid slots kept apart from the inventory | inherit (a separate field) |
| `any-log-species` | the graph asks for any log; planks follow the log held | inherit |
| `one-planner` | one needs planner, the recipe-only walk removed | inherit ([[cc-plan-and-make]]) |
| `goals-in-play-from-go` | the goals a run pursues arrive on the `:go` event | adapt (the scenario or run config) |
| `pearls-rule-table` | the pearl hunt is rows over named facts; history in data | the shape only; the table is not ported ([[cc-pearls-rule-table]]) |
| `threats-as-data` | per-kind threat facts and a cover ladder in data | inherit |
| `randomness-on-the-tick` | `rand` is a field of the tick event | inherit as a seeded RNG ([[cc-randomness-on-the-tick]]) |
| `facts-in-datascript` | memory is append-only facts queried by Datalog | adapt (facts, no Datalog engine) ([[cc-world-memory]]) |
| `remembered-by-identity` | costly queries memoised by the identity of the world's parts | adapt (explicit caches) |
| `sets-over-arrays` | block predicates are sets of state ids, by measurement | revisit (Rust bitsets are cheap) |
| `spot-two-blocks-away` | stand spots searched in rings 1 to 3 with a body check | inherit |
| `natural-tree-not-fixture` | tests chop a real tree in a normal world | inherit |
| `from-scratch-protocol` | protocol as data in-repo, no Java library | inherit ([[own-protocol-not-azalea]]) |
| `public-by-default` | functions public so the REPL can reach them | not applicable (Rust visibility; test via `testkit`) |

## See also
- [[ruststeve/decisions/_moc|ruststeve decisions]] - the ones made here.
- [[cc-lessons]] - the lessons behind them.
