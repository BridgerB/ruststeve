---
title: Design
type: moc
tags: [moc, design]
status: verified
lastUpdated: 2026-11-11
related:
  - "[[ruststeve/_moc|ruststeve]]"
---

# Design

How ruststeve is shaped. Read [[sans-io-core]] first; everything else hangs off it.

## The core
- [[sans-io-core]] - the whole core API (`Brain`, `Input`, `Output`, `step`) and the fixed order inside a step.
- [[world-model-fold]] - the world model is only ever changed by folding observations.
- [[domain-types]] - newtypes, ids not pointers, kinds as enums, context types instead of `Option` soup.
- [[phases-and-task-stack]] - phase, task stack and tactic; how a failed task is recovered.
- [[expectations-and-watchdog]] - how "stuck" is detected mechanically.
- [[progress-invariants]] - loops without progress made impossible: invariants per intent, resources per progress, a CPU budget.
- [[rule-tables]] - tactics as named, prioritised rules; the rule that fires is the reason.
- [[brain-events]] - decisions as data (`BrainEvent`, `CombatFrame`).

## The shell
- [[tick-loop]] - how the shell drives the core; record before step; what runs on which thread.
- [[body-boundary]] - the trait that hides the game; in ruststeve, our own wire layer.
- [[workspace-crates]] - the crates and the one-way dependency rule.
- [[ci-on-runners]] - how code runs: the ci jobs, the cache rule, the probe for one-off questions, the boundary check.

## Truth and time
- [[determinism-rules]] - every nondeterminism source and the lint that bans it.
- [[recordings]] - the recording format, checkpoints, `verify`, growth by accretion.
- [[replay-limits]] - why replay cannot test a changed brain; which tool answers which question.
- [[incident-to-regression]] - the six steps from a stuck run to a permanent test.
- [[observability]] - recordings, events with reasons, case files, senses, batch files, the read-only snapshot stream, coverage.

## Quality
- [[error-kinds]] - domain outcomes vs boundary, infrastructure and bug errors.
- [[testing-kinds]] - the seven kinds of test and when each runs.
- [[accents]] - TypeScript, OOP and Clojure habits to catch in review; the PR checklist.

## See also
- [[ruststeve/decisions/_moc|Decisions]]
