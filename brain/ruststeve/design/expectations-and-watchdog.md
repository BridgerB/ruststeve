---
title: Expectations and the watchdog
description: How the core detects "stuck" mechanically: commands register expectations with deadlines, and a generic watchdog fails a task whose progress metric stops improving.
type: reference
tags: [stuck, expectations, watchdog, progress]
aliases: [Expectation, Expect, deadline, NoProgress, stuck detection]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#A missed expectation is a logged event and an input to replanning. This is the main mechanism for detecting \"stuck.\""
  - "ruststeve:FUNCTIONAL.md#Every task kind defines a numeric progress metric and a patience budget. The watchdog is generic and doesn't know anything about blazes."
  - "ruststeve:FUNCTIONAL.md#The thresholds are placeholders: take real values from the Clojure version where they exist, otherwise measure them from recordings of successful runs. Put them in Config, not in code."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[phases-and-task-stack]]"
  - "[[incident-to-regression]]"
---

# Expectations and the watchdog

A Minecraft server is a remote system that can ignore you. So every command that should change the world registers an `Expectation` with a deadline in ticks, and the core checks it against later observations. A miss is a logged `ExpectationMissed` event and an input to replanning. This, plus a generic progress watchdog, turns "stuck" from something a human notices into a replayable event.

## Key files
- `FUNCTIONAL.md`, sections 3.20, 6.4 and 8.6.

## How it works
- `Expectation { cmd, expect, deadline: Tick }`; `Expect` is `EntityHealthDrops(id)`, `BlockBecomes(pos, kind)`, `ReachWithin { pos, radius }`, `ItemCountAtLeast { item, count }`.
- Checked in step 3 of every `step` ([[sans-io-core]]).
- Each task kind has a progress metric (higher is better) and a patience in ticks: `Goto` negative remaining path length, `Mine` break progress then pickup, `FightBlaze` damage dealt plus rods minus damage taken, `Explore` new chunks seen. No improvement within patience → `Failure::NoProgress` → recovery, and the flight recorder dumps.
- Flight-recorder triggers: no progress, an expectation-miss streak, oscillation (net movement under a radius during `Goto`), damage rate, a recovery loop (same failure and recovery repeating), death, disconnect, panic.

## Gotchas
- Fire-and-forget commands are the smell; every world-changing command needs an expectation.
- Thresholds go in `Config`, taken from clojurecraft where it has them ([[cc-stall-and-budgets]]) or measured from successful recordings.

## Limits
The patience values are not set. clojurecraft's own window rule (a third unanswered click after 3 s) is one real example of an expectation with a deadline.

## See also
- [[progress-invariants]] - the specific invariants the handoff asks for.
- [[brain-events]] - the events these emit.
- [[incident-to-regression]] - what happens after a trip.
- [[cc-window-clicks]] - clojurecraft's click deadline.
