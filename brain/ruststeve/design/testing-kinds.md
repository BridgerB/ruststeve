---
title: Kinds of test
description: The seven kinds of test, the question each answers and when each runs, plus the property invariants worth holding.
type: reference
tags: [testing, proptest, insta, differential]
aliases: [unit, snapshot, property, differential, scenario, nextest, testkit]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Because the core is pure and its inputs are plain values, almost everything can be tested without a server, and test data is cheap to fabricate."
  - "ruststeve:FUNCTIONAL.md#Checkpoints are exact. Folding the full log equals restoring a mid-log checkpoint and folding the rest."
  - "ruststeve:FUNCTIONAL.md#The Clojure bot is the oracle until Rust passes it (ground rule 10)."
  - "ruststeve:FUNCTIONAL.md#Keep the full core test suite under about 30 seconds so it can run on every save."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[replay-limits]]"
  - "[[clojure-is-the-oracle]]"
---

# Kinds of test

The core is pure and its inputs are plain values, so almost everything is testable without a server.

## Key files
- `FUNCTIONAL.md`, section 11.

## How it works

| Kind | Question | Tooling | When |
|---|---|---|---|
| Unit | does this pure function do what it says? | `#[test]`, `cargo nextest` | every push, on a runner |
| Snapshot | did this decision or event stream change? | `insta`, `cargo insta review` | every commit |
| Property | does an invariant hold for all inputs? | `proptest`, `proptest-state-machine` | every commit |
| Determinism replay | same inputs, same outputs? | `bot-replay verify` over `recordings/golden/` | CI |
| Regression replay | is a past bad decision gone? | property assertions over `recordings/regressions/` | CI |
| Differential | does Rust decide like Clojure? | shared JSON fixtures through both | port phase 3 |
| Closed-loop scenario | does the bot succeed? | local server, success rates | nightly, before tactic merges |

Invariants worth holding: checkpoint plus suffix equals the full fold; `Brain`, `Input`, `Output`, `Record` round-trip through serde; every path step is a legal move ending in the goal; no `Attack` when the target is not visible or in reach; no entity in two places, no negative count.

Fixtures are builders over plain data (`WorldBuilder::new().dimension(Nether).me_at(..).blaze(..)`) in a `bot-core::testkit` module behind a `testkit` feature. Unit tests sit in `#[cfg(test)] mod tests`; slow scenarios go behind a feature or `#[ignore]`; the core suite stays under about 30 seconds.

**Differential tests**: generate cases in Rust, write them as fixture files, run a small Clojure script calling the original function, compare. Exact equality for math; per-rule agreement for decisions, every disagreement investigated.

## Gotchas
- Every test runs on GitHub Actions runners, never the Mac ([[runners-not-the-mac]]): the guide's "every save" loop becomes "every push". Locally only fmt, clippy and the brain checker.

## See also
- [[replay-limits]] - why scenarios exist at all.
- [[cc-test-suite]] - clojurecraft's tests, the source of differential cases.
