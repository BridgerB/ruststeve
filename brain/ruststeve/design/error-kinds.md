---
title: Four kinds of failure
description: How a domain outcome, a boundary error, an infrastructure error and a bug are each represented and answered, and why game outcomes are never Err.
type: reference
tags: [errors, failure, panics]
aliases: [thiserror, anyhow, Failure vs Err, ParseError, BodyError, InvariantViolated]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Domain failures are values, not Err."
  - "ruststeve:FUNCTIONAL.md#Typed errors in libraries, anyhow in the application."
  - "ruststeve:FUNCTIONAL.md#Let it crash. The Input that caused the panic is already on disk (the loop records before stepping), so the panic is perfectly reproducible with bot-replay."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[phases-and-task-stack]]"
  - "[[tick-loop]]"
---

# Four kinds of failure

Most of what the bot experiences as failure is not an error: it is a normal game outcome and belongs in the domain model.

## Key files
- `FUNCTIONAL.md`, section 10.

## How it works

| Kind | Examples | Representation | Response |
|---|---|---|---|
| Domain outcome | no path, target lost, died, missing item, no progress | `TaskStatus::Failed(Failure)` | planner recovery, logged as `Recovered` |
| Boundary error | malformed packet, NaN position, unknown registry id | `Result<Observation, ParseError>` (`thiserror`) in the adapter | map to `Unknown` or drop, log, count |
| Infrastructure | disconnect, server restart, disk full | `BodyError` (`thiserror`), `anyhow` at the top | reconnect with backoff, or stop with the recording flushed |
| Bug | negative count, entity in two chunks, empty stack mid-phase | `debug_assert!` plus `InvariantViolated` | panics in tests; live, an event and an incident dump |

Rules: typed `thiserror` enums in `bot-types`, `bot-core`, `bot-path` and the adapter; `anyhow` only in `bot-shell`'s `main` and the replay CLI. Error enums that reach recordings derive `Serialize`. No `unwrap`, `expect` or `panic!` in the core; `?`, `let ... else`, saturating `Tick` arithmetic. Never swallow: every dropped `Err` leaves a log line or counter, because a silently dropped boundary error is how the bot comes to believe something false.

## Gotchas
- If the core panics anyway, let it crash: the input is already on disk, so the panic replays exactly. Limping on with corrupted state is worse.

## See also
- [[phases-and-task-stack]] - `Failure` and `Recovery`.
- [[wire-boundary-parsing]] - where boundary errors are born in our own protocol layer.
