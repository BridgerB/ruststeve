---
title: One owner, no shared mutation
description: Why all bot state has one owner mutated through &mut for one step at a time, and why Rc<RefCell> and Arc<Mutex> are banned from the core.
type: decision
tags: [decision, ownership, state, core]
aliases: [no Arc<Mutex>, no Rc<RefCell>, aliasing XOR mutability, single owner, epochal time]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Mutation is fine. Shared mutation is not."
  - "ruststeve:FUNCTIONAL.md#Treat borrow-checker friction as a design signal: it means two parts of the program are trying to own the same changing thing."
  - "ruststeve:FUNCTIONAL.md#The one-line rule: in Rust, mutation is how you build the next value, and ownership is how you guarantee nobody sees it half-built."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[sans-io-core]]"
  - "[[hist-why-v1-failed]]"
---

# One owner, no shared mutation

## The choice
The tick loop owns the `Brain`; `step(&mut self, &Input)` is the only way it changes; readers get published `Arc` snapshots. `&mut` is used freely inside that one owner. `Rc<RefCell<_>>` and `Arc<Mutex<_>>` never appear in the core (and `Rc`, `RefCell` are lint-banned there).

## What was rejected
- **The object graph of mutually referencing, mutating objects** (TypeScript's shape). Ported literally, the borrow checker refuses it, and the usual escape is `Rc<RefCell<_>>` or `Arc<Mutex<_>>` around everything: hidden shared state, no single timeline, plus lock contention and runtime borrow panics. ruststeve v1 ended exactly there ([[hist-why-v1-failed]]).
- **Clojure's persistent values everywhere.** Unnecessary: while `&mut brain` is held nobody can observe it, so in-place mutation is indistinguishable from returning a new value, without the copies.

Hickey and Rust diagnose the same disease, shared mutable state. Clojure removes mutation; Rust removes sharing *while* mutating. Port the diagnosis, use Rust's cure.

## What would change the answer
Nothing short of needing several live versions of a large structure at once (a planner exploring hypothetical worlds), and even then the answer is `im`/`rpds` or cloning a small slice, not shared mutation.

## See also
- [[sans-io-core]] - the API this shapes.
- [[accents]] - the review list that catches violations.
