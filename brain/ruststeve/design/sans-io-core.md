---
title: The sans-IO core
description: What the core's whole public API is (Brain, Input, Output, step), and the fixed order of work inside one step.
type: reference
tags: [core, architecture, brain]
aliases: [Brain::step, functional core, pure core, sans-io]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#pub fn step(&mut self, input: &Input) -> Output;   // the only way state advances"
  - "ruststeve:FUNCTIONAL.md#Inside step, the order is fixed and always the same:"
  - "ruststeve:FUNCTIONAL.md#The core has no I/O and is deterministic."
  - "ruststeve:FUNCTIONAL.md#pub tick: Tick,                        // game time: the only clock the core sees"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[tick-loop]]"
  - "[[world-model-fold]]"
  - "[[determinism-rules]]"
---

# The sans-IO core

The bot is a deterministic core with no I/O wrapped by a thin shell. Everything the core learns arrives as one `Input` per tick and everything it decides leaves as one `Output`; `Brain::step` is the only way state advances. This is "functional core, imperative shell", called sans-IO in Rust networking.

## Key files
- `FUNCTIONAL.md`, section 4, "The core's whole public API": `Input`, `Output`, `Brain`.
- The core crates (`world`, `physics`, `memory`, `plan`; not written yet): see [[workspace-crates]]. `FUNCTIONAL.md` calls them `bot-core`.

## How it works
- `Input { tick, observations, completions }`. `tick` carries the client clock and a random seed (the only clock and randomness the core sees); game time is counted from the server's ticks. `observations` are facts since the last tick, in arrival order. `completions` are results of expensive work requested earlier.
- `Output { commands, requests, events }`. Each command carries a `Reason`; `requests` are pure work for the shell to run elsewhere; `events` are the structured "why" ([[brain-events]]).
- `Brain { world, plan, expectations, config }`. Time and randomness arrive on the input (the clock and a random seed on every tick, as clojurecraft did), never from inside the core ([[sync-core-and-time]]).
- Inside `step`, always in this order:
  1. fold `observations` into `world` ([[world-model-fold]]);
  2. fold `completions` into the plan;
  3. check expectations, emit `ExpectationMissed` for any past its deadline ([[expectations-and-watchdog]]);
  4. run the planner: keep, advance or replace the current task ([[phases-and-task-stack]]);
  5. run the current task's tactic to produce commands, registering an expectation for each world-changing command;
  6. return `Output`.
- `step(&mut self, &Input)` and `step_owned(Brain, &Input) -> (Brain, Output)` are the same program: while `&mut brain` is held nobody else can see it, so mutation in place cannot be observed half-done. The tick loop *is* Clojure's atom.

## Gotchas
- A clock read, `env::var`, a `HashMap` iteration or `thread_rng` anywhere in the core silently breaks replay. These are banned by lint, not by habit ([[determinism-rules]]).
- Splitting borrows: destructure `let Brain { world, plan, .. } = self;` to read `world` while mutating `plan`.

## Limits
The guide's shapes are illustrative; the real fields of `Config`, `Phase` and `Task` come from clojurecraft during the inventory ([[port-phases]]).

## See also
- [[tick-loop]] - the shell side that calls `step`.
- [[one-owner-no-shared-mutation]] - why `&mut` under one owner, never `Arc<Mutex<_>>`.
- [[cc-reducer-and-effects]] - the Clojure shape this replaces.
