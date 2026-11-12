---
title: Ordered collections in the core
description: Why the core uses BTreeMap, BTreeSet and IndexMap and never HashMap or HashSet.
type: decision
tags: [decision, determinism, collections]
aliases: [no HashMap, BTreeMap, IndexMap, iteration order]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 ruststeve-v1@9e1567f
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#HashMap/HashSet iteration"
  - "ruststeve-v1:src/protocol/codec.rs#use std::collections::HashMap;"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[determinism-rules]]"
---

# Ordered collections in the core

## The choice
Every map and set in `bot-core` and `bot-path` is `BTreeMap`, `BTreeSet` or `IndexMap`/`IndexSet`. `HashMap` and `HashSet` are `disallowed-types` there.

## What was rejected
`std::collections::HashMap`: its hasher is seeded randomly per process, so iteration order differs run to run. Any decision that iterates a map (the nearest of several equidistant entities, the first matching rule) then differs on replay, and `verify` fails for no visible reason. ruststeve v1 used `HashMap` freely (its codec's type registry is one), but v1 never replayed anything, so it never paid for it.

## What would change the answer
A measured hot path where `BTreeMap` lookups cost real tick budget. The fix then is a deterministic hasher (a fixed-seed `HashMap` with no iteration in decisions) behind a reviewed exception, not the default `HashMap`.

## See also
- [[determinism-rules]] - the whole determinism checklist.
