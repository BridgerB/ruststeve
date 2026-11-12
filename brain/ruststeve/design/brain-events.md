---
title: Brain events
description: How the core explains itself as data (BrainEvent, CombatFrame) instead of log strings, and what each event records.
type: reference
tags: [observability, events, why]
aliases: [BrainEvent, CombatFrame, RuleFired, decisions as data, Reason]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#The core never explains itself through log strings."
  - "ruststeve:FUNCTIONAL.md#Using tracing inside the core is allowed only at debug/trace level for breadcrumbs. It must never be the only record of a decision."
  - "ruststeve:FUNCTIONAL.md#pub target_charging: bool,             // the blaze's \"on fire\" metadata flag: volley wind-up"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[observability]]"
  - "[[rule-tables]]"
---

# Brain events

The core never explains itself through log strings. It returns structured `BrainEvent`s in `Output.events`; they are recorded, snapshot-testable and renderable anywhere. Every command also carries a `Reason`.

## Key files
- `FUNCTIONAL.md`, sections 3.6, 8.1 and 16.2.

## How it works
`BrainEvent` variants: `PhaseChanged { from, to }`, `TaskStarted { id, task, reason }`, `TaskEnded { id, status, ticks }`, `RuleFired { tactic, rule, priority }`, `ExpectationMissed { cmd, expect, deadline }`, `WatchdogTripped { task, metric, best, since }`, `Recovered { failure, chosen }`, `CombatFrame(..)`, and (section 10) `InvariantViolated`.

`CombatFrame` is emitted every tick of a fight with what the rules saw: tick, target id and position, `target_dy` (blazes hover above reach), distance, visibility, `target_charging` (the volley wind-up: the guide says the on-fire flag; clojurecraft reads the blaze's metadata index 16, see [[mc-blaze]]), incoming projectiles, own health, on fire, attack ready, the chosen intent and the rule that chose it. Every blaze hypothesis is confirmed or ruled out from this stream ([[blaze-debugging-method]]).

`Command { id, action, reason }`; `Action` is plain data (`LookAt`, `WalkTo`, `Jump`, `Attack`, `UseItem`, `PlaceBlock`, `Mine`, `SelectSlot`, `StopMoving`). The core never calls the network.

## Gotchas
- `tracing` in the core is breadcrumbs at debug/trace only, never the sole record of a decision.
- New variants are added, never renamed: old recordings are the regression suite ([[recordings]]).

## See also
- [[observability]] - how the shell logs and renders these.
- [[rule-tables]] - where `RuleFired` comes from.
