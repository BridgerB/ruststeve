---
title: Design
type: moc
tags: [moc, design]
status: verified
lastUpdated: 2026-11-12
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
- [[replay-and-why]] - record a run, verify it replays exactly, export it, ask `why` at any moment.
- [[entities-and-inventory]] - how the world folds entities and window 0; what is counted rather than invented.
- [[memory-sightings]] - watched blocks remembered as sightings with time, only when seen with an exposed face.
- [[physics]] - the body moves: a bit-exact port of clojurecraft's physics step, the movement packets, zero server corrections live.
- [[race-and-results]] - the race on runners, starts as data, the stage ladder, case and batch files, coverage, k/n.
- [[insight-store]] - the SurrealDB 4 store built from the race's files, its views, the schema that is steve's contract.
- [[snapshot-stream]] - the live view: a bot pushes MessagePack frames to steve's local relay, never in the way of a run.
- [[server-model]] - the vanilla server as a pure reducer, the bot's body without Java, grown per issue.
- [[world-value]] - what `World` holds, how `step` folds each packet, `block_at`, the chunk decoder checked against clojurecraft.
- [[connection]] - how a bot connects and stays in the world: the pure protocol crate, the net threads, the bot binary, running a server locally.
- [[datagen]] - the game tables: generated from the 26.1.2 server on a runner, committed in `crates/data`, drift and unknown packet ids caught in CI.
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
