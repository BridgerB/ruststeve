---
title: ruststeve v1
description: What the first Rust port was (a steve port with its own SDK, a sync-pick-execute step machine, about 31,500 lines), how far it got, and where its code can still be read.
type: reference
tags: [history, ruststeve-v1, architecture]
aliases: [v1, first Rust port, the old ruststeve, rustcraft, step machine]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve-v1@9e1567f
sourceRefs:
  - "ruststeve-v1:CLAUDE.md#An autonomous Ender-Dragon speedrun bot (random seed, any%, no human input) — a Rust port of the TypeScript steve bot"
  - "ruststeve-v1:CLAUDE.md#Critical invariant: get_next_step returns the FURTHEST runnable incomplete step (.next_back()), not the first."
  - "ruststeve-v1:docs/report-2026-10-10.md#Yes. Race i11 (race-20261009-095027-i11), rust-race-004, stood in the Nether through a portal it cast and lit itself at 78 min"
  - "ruststeve-v1:src/steps.rs#pub async fn execute_step(bot: &mut Bot<'_>, id: &str, mem: &mut WorldMemory) -> StepResult {"
related:
  - "[[history/_moc|History]]"
  - "[[hist-why-v1-failed]]"
  - "[[just-ruststeve]]"
---

# ruststeve v1

The first Rust port, a port of steve. Its last commit on main is 9e1567f; the reset branches deleted it, and the brain reads it by commit as `ruststeve-v1`.

## Key files (at 9e1567f)
- `src/app.rs` (the loop), `src/state.rs` (`GameState`), `src/steps.rs` (28 ordered steps), `src/tasks/` (one module per capability family), `src/survival.rs` (reflexes).
- The in-crate SDK: `bot`, `world`, `protocol`, `chunk`, `path`, `physics`, `registry` ... ported from typecraft.
- `src/gym.rs` (about 50 gym slugs), `src/learn.rs` (attempt rows), `src/telemetry.rs` (race.db).
- About 31,500 lines of Rust in `src/`.

## How it worked
- `app::run`: drain packets → `state::sync_from_bot(&Bot)` folds the bot into a flat `GameState` → `survival::handle_survival` (breath, lava, health; the first that fires wins) → `steps::get_next_step` picks the **furthest** runnable incomplete step → `execute_step` dispatches by **string id** into `async` task functions that both decide and send packets through `&mut Bot`.
- Measurement plumbing was strong: gyms on random or paired landings, event rows per attempt, Wilson intervals, Thompson-sampling bandits (all disabled), race telemetry in SQLite.

## How far it got
- Cycle 6 (wrapped 2026-10-10): rust-race-004 stood in the Nether through a portal it cast and lit itself at 78 minutes; seven own-portal Nether entries across races i11 to i13. No bot reached a fortress.
- Late-game gyms on Normal: blaze rods 0/33, dragon 0/3, crystals 0/3, pearls 1/6.

## See also
- [[hist-why-v1-failed]] - why it was reset.
- [[hist-v1-load-bearing-fixes]] - what it learned that ruststeve keeps.
