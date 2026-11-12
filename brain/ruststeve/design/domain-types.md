---
title: Domain types
description: The rules every domain type follows (newtypes, ids not pointers, kinds as enums, context types instead of Option soup) and how Clojure shapes map to them.
type: reference
tags: [types, data, modeling]
aliases: [newtypes, BlockPos, Tick, EntityId, context types, parse don't validate]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Public fields, no getters on plain data."
  - "ruststeve:FUNCTIONAL.md#A newtype for every unit: Tick, BlockPos (i32), Vec3 (f64), ChunkPos, EntityId, Slot, Health, Degrees."
  - "ruststeve:FUNCTIONAL.md#Kinds are enums: BlockKind, ItemKind, EntityKind, Dimension."
  - "ruststeve:FUNCTIONAL.md#Never store a reference or Rc to another entity inside a struct."
  - "ruststeve:FUNCTIONAL.md#/// Built fresh each tick for the combat tactic. If any required fact is unknown, there is no BlazeTarget."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[world-model-fold]]"
  - "[[enums-for-closed-sets]]"
---

# Domain types

Small, flat, public structs and enums. Every unit has its own newtype, other things are referred to by id, collections keyed by id live in ordered maps, and untrusted data is parsed once at the boundary.

## Key files
- `FUNCTIONAL.md`, sections 3.11, 3.12, 3.16 and 5.1 to 5.4.

## How it works
- **Public fields, no getters** on plain data. Behaviour lives in functions and `impl` blocks reading those fields.
- **Derive the inspection traits** (`Clone, Debug, PartialEq, Serialize, Deserialize`) on everything that crosses the core boundary; add `Copy, Eq, Hash, Ord` on small keys.
- **A newtype per unit**: `Tick`, `BlockPos` (i32), `Vec3` (f64), `ChunkPos`, `EntityId`, `Slot`, `Health`, `Degrees`. Conversions are named (`BlockPos::containing(Vec3)`), never scattered `as` casts. Constructors that can fail return `Result` (`Health::new` rejects values outside 0 to 20).
- **Kinds are enums**: `BlockKind`, `ItemKind`, `EntityKind`, `Dimension`, generated from the registry, each with an `Unknown(u32)` so a new server version degrades instead of crashing.
- **Ids, not pointers**: store `EntityId` and resolve it each time; "gone" is an explicit outcome (`TargetLost`). No lifetimes, `Rc`, `RefCell` or trait objects in domain types; internal graphs use `slotmap` keys.
- **Context types instead of `Option` soup**: keep the raw record sparse, then build a type where the needed fields are guaranteed, e.g. `BlazeTarget::from_world` returns `None` when a required fact is unknown.

Clojure to Rust: `{:x 1 :y 64 :z 3}` → `BlockPos`; keyword sets → enums; a map dispatched on `:type` → enum variants with data; `nil` as unknown → `Option` in the raw record; `nil` as failed → `Result`; metadata → explicit fields; EDN on disk → the binary recording format ([[recording-format]]) for runs, RON or TOML for hand-written data tables.

## Gotchas
- `f64` used for positions, health and time interchangeably is the smell; so is `.unwrap()` on a field "always set by then".

## See also
- [[world-model-fold]] - where these types are folded.
- [[enums-for-closed-sets]] - when an enum and when a trait.
- [[cc-to-rust-map]] - the full Clojure idiom table.
