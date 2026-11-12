---
title: Phases, the task stack and recovery
description: The three layers of the decision core (phase, task stack, tactic), how a failed task is recovered, and why every match is exhaustive.
type: reference
tags: [planner, phases, tasks, recovery]
aliases: [Phase, Task, Plan, Recovery, Failure, planner, TaskStatus]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#A phase is complete when a predicate on the world model is true, not when some code path says so."
  - "ruststeve:FUNCTIONAL.md#The phase list above is a placeholder. Take the real sequence, and the real completion conditions, from the Clojure bot."
  - "ruststeve:FUNCTIONAL.md#When the top task returns Failed(f), the planner calls one pure function, recover(&f, &plan, &world) -> Recovery, and logs the failure and the recovery chosen:"
  - "ruststeve:FUNCTIONAL.md#In bot-core, deny clippy::wildcard_enum_match_arm."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[expectations-and-watchdog]]"
  - "[[rule-tables]]"
  - "[[cc-goals-and-intents]]"
---

# Phases, the task stack and recovery

The brain decides in three layers of printable data: a **phase** (where we are in the speedrun), a **task stack** (what we are doing now), and a **tactic** (what to do this tick, often a [[rule-tables|rule table]]). Every transition is a logged event with a reason.

## Key files
- `FUNCTIONAL.md`, section 6.

## How it works
- `Phase` is an enum (the guide's placeholder: `EarlyGame`, `BuildPortal`, `EnterNether`, `FindFortress`, `CollectBlazeRods`, `GetEnderPearls`, `CraftEyes`, `FindStronghold`, `EnterEnd`, `KillDragon`, `Done`). `Phase::is_complete(w, cfg)` is a predicate on the world model, so progress is checkable from any snapshot.
- `Task` is an enum carrying exactly the state it needs (`Goto { goal, path, started }`, `Mine { target, started }`, `FightBlaze(BlazeFight)`, ...). `step_task(&mut Task, &TickCtx, &mut Emitter) -> TaskStatus`; the emitter assigns `CommandId`s and registers expectations.
- `TaskStatus` is `Running | Done | Failed(Failure)`; `Failure` is data: `NoPath`, `TargetLost`, `NoProgress`, `MissingItem`, `TookDamageTooFast`, `Died`.
- `Plan { phase, stack: Vec<Task>, watchdog }`; the top of the stack is current. On `Failed(f)` one pure `recover(&f, &plan, &world) -> Recovery` chooses `Retry`, `PushSubtask`, `ReplaceWith`, `PopAndReplan` or `Escalate`, and logs both. All recovery policy sits in one table.
- Runtime state machines are enums; the typestate pattern is only for shell APIs where misuse is a programmer error (a `Recorder<NoHeader>` that becomes `Recorder<Ready>`).
- `clippy::wildcard_enum_match_arm` is denied, so adding `Task::Bridge` makes the compiler list every place that must decide. The only exception is the porting-time `_ => todo!()`, which must be gone by the end of phase 4.

## In ruststeve so far (issue 09a)
`plan::Task` is `Idle | ToLog(Walk) | Explore(Walk)`, `walk::TaskStatus` is `Running | Done | Failed(Failure)` and `Failure` is `NoPath | NoProgress | Deadline | TargetLost`; one task is held, not yet a stack, and recovery is refuse-and-choose-again ([[planner]]). Phases arrive with the issues that need them.

## Gotchas
- The phase list is a placeholder. clojurecraft has no phase enum; its progress lives in goal rows ([[cc-goals-and-intents]]). Deriving the real sequence is inventory work ([[port-phases]]).
- Moving owned state between variants: `std::mem::take(task)` then match, listing unchanged variants explicitly.

## See also
- [[expectations-and-watchdog]] - how "stuck" becomes a `Failure`.
- [[cc-plan-and-make]] - clojurecraft's planner, the behaviour to port.
