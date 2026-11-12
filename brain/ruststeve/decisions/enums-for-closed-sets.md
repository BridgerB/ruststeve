---
title: Enums for closed sets, traits for open ones
description: Why tasks, phases, commands, events and failures are exhaustive enums, and the only places a trait is the right tool.
type: decision
tags: [decision, types, enums, traits]
aliases: [exhaustive match, no wildcard arms, polymorphism a la carte, when to use a trait]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Closed sets are enums. Tasks, goals, commands, events and failure reasons are enums, matched exhaustively."
  - "ruststeve:FUNCTIONAL.md#That is the visibility the Clojure version lacked, where an unhandled dispatch value silently did nothing."
  - "ruststeve:FUNCTIONAL.md#A trait with a single implementation \"for flexibility.\" Box<dyn Task> hierarchies that recreate TypeScript classes. String-keyed dispatch maps."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[phases-and-task-stack]]"
  - "[[domain-types]]"
---

# Enums for closed sets, traits for open ones

## The choice
A set we own and can list (tasks, phases, actions, observations, events, failure reasons, block and item kinds) is an `enum`, matched with no `_ =>` arm (`clippy::wildcard_enum_match_arm` denied in the core). A trait is used only where the set is genuinely open: the `Body`, a swappable pathfinder, test doubles. `Box<dyn Trait>` only at the shell edge where static dispatch is impossible.

## What was rejected
- **Dynamic dispatch on keywords** (Clojure multimethods, a `HashMap<&str, fn>`): an unhandled dispatch value silently does nothing. That is the gap the blaze debugging kept falling into.
- **Trait-object task hierarchies** (`Box<dyn Task>`): TypeScript classes again, and no compiler list of what a new case touches.

With exhaustive enums, adding `Task::Bridge` or `Failure::Burning` makes the compiler list every place that must decide what to do.

## What would change the answer
A set that must be extended without editing the crate that owns it (third-party plugins). None exists.

## See also
- [[phases-and-task-stack]] - the enums the planner is built from.
- [[domain-types]] - `Unknown(u32)` variants for server registries.
