---
title: Why v1 failed
description: What actually made ruststeve v1 hard to observe and fix, read from its code (a god-object Bot, async tasks that decide and act, string dispatch, dynamic protocol values, clock and env reads), and where the guide's account differs.
type: explanation
tags: [history, ruststeve-v1, anti-patterns]
aliases: [TypeScript in Rust syntax, v1 failure, god object, why the reset]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve-v1@9e1567f ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#The Rust port failed because it was TypeScript written in Rust syntax."
  - "ruststeve-v1:src/bot/mod.rs#pub struct Bot<'a> {"
  - "ruststeve-v1:src/bot/mod.rs#pub static DEATH_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);"
  - "ruststeve-v1:src/steps.rs#\"gather_wood\" => tasks::gather_wood::gather_wood(bot, 6, mem).await,"
  - "ruststeve-v1:src/protocol/value.rs#/// Any number-typed field (i8..u32, f32, f64, varint). Mirrors JS number."
related:
  - "[[history/_moc|History]]"
  - "[[accents]]"
  - "[[one-owner-no-shared-mutation]]"
  - "[[hist-ruststeve-v1]]"
---

# Why v1 failed

The guide's one-line verdict is that v1 was TypeScript written in Rust syntax. The code at 9e1567f shows what that meant concretely.

## What the code shows
- **A god object.** `pub struct Bot<'a>` (in `src/bot/mod.rs`, 2,358 lines) holds the socket client, the registry, the entity, health, food, inventory, windows, world, movement and more. Every task takes `&mut Bot` and both decides and sends packets inside `async fn`s. Decision and action are the same line, so no decision is a value that can be logged or replayed.
- **String dispatch.** `execute_step(bot, id: &str, ...)` matches step ids as strings (`"gather_wood" => ...`): typos compile, nothing is exhaustive.
- **Dynamic protocol values.** Every packet field is a `PValue` (`Num(f64)` for any number, `Compound(Vec<(String, PValue)>)`), interpreted from a JSON schema at runtime and looked up by string name ([[wire-prior-implementations]]).
- **Hidden inputs.** About 120 wall-clock reads and about 70 `env::var` calls across `src/`, and `HashMap` throughout. A run could not be replayed and nobody tried.
- **A process-wide static** (`DEATH_COUNT`) to read state while a task held `&mut Bot`: a sign the single owner was not enough because the state was not a value.

## Where the guide's account differs
The guide describes v1 ending as `Arc<Mutex<Bot>>`, callbacks and borrow-checker fights. At 9e1567f, `Arc<Mutex>` and `RefCell` barely appear (a handful of uses in the pathfinder and viewer). v1 avoided shared mutation by threading one `&mut Bot` through `async` code instead. The opacity came from the god object and effects-inside-decisions, not from locks. The lesson is the same; the mechanism to watch for is "decide and act in one function", not only `Arc<Mutex>`.

## What it did well (kept as knowledge)
Strong measurement (paired gyms, Wilson intervals, attempt rows) and hard-won protocol and game facts ([[hist-v1-load-bearing-fixes]]).

## See also
- [[accents]] - the review list built from this.
- [[sans-io-core]] - decisions as values, the fix.
