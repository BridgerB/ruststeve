---
title: Architecture
type: moc
tags: [moc, oracle, architecture]
status: verified
lastUpdated: 2026-11-11
related:
  - "[[oracle/_moc|Oracle]]"
---

# Architecture

How clojurecraft is built, each note ending in what it means for ruststeve.

## The spine
- [[cc-world-value]] - all state is one flat, sparse map in one atom.
- [[cc-reducer-and-effects]] - one pure reducer over events; effects as data.
- [[cc-main-loop]] - the one impure loop; the 50 ms clock in ms, not ticks.
- [[cc-randomness-on-the-tick]] - randomness arrives on the tick event.

## Deciding
- [[cc-goals-and-intents]] - the goal table, re-derived every tick; intents as values; the run's goals in order.
- [[cc-plan-and-make]] - the needs planner from a target to the next act.
- [[cc-stall-and-budgets]] - the watchdog numbers and search budgets.
- [[cc-pearls-rule-table]] - 70 rules over named facts: the rule-table template.

## Doing
- [[cc-window-clicks]] - crafting and smelting by clicks; the stale-window rule.
- [[cc-blaze-hunt]] - the blaze stage and what its recordings measured.

## Perceiving
- [[cc-entities]] - entities and the extra senses added after unexplained deaths.
- [[cc-world-memory]] - facts with time, per dimension, only what was seen.
- [[cc-physics]] - client movement as a pure function; water.

## See also
- [[oracle/harness/_moc|Harness]]
