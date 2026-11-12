---
title: Memory and sightings
description: How ruststeve remembers watched blocks (logs, tables, furnaces, nether bricks, spawners, obsidian) as append-only sightings with time, only when seen with an exposed face, indexed by position and chunk column; how the world decides what to look at.
type: reference
tags: [memory, sightings, no-xray, facts, world]
aliases: [Memory, Sighting, Seen, watched kinds, exposed face, memory crate, facts with time]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@951a21a clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/memory/src/lib.rs#pub fn check("
  - "ruststeve:crates/memory/src/lib.rs#pub fn record(&mut self, seen: Seen, at_tick: u64) -> bool {"
  - "ruststeve:crates/world/src/lib.rs#fn observe_column(&mut self, at: ChunkPos) {"
  - "ruststeve:crates/world/tests/properties.rs#fn a_buried_log_is_seen_only_once_uncovered() {"
  - "clojurecraft:src/clojurecraft/memory.clj#defn watched?"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[cc-world-memory]]"
  - "[[hist-no-xray]]"
  - "[[world-value]]"
---

# Memory and sightings

The `memory` crate (core, no I/O) is clojurecraft's sighting store in Rust ([[cc-world-memory]]): facts with time, never retracted.

## How it works
- A `Sighting { dimension, pos, state, at_tick }` is appended only when the state differs from the latest sighting at that position; a dug log becomes a newer fact of air, and the history stays.
- Indexes: the latest sighting by position, and positions by dimension and chunk column. `refold_latest` rebuilds the index from the log; the property test holds them equal.
- **Only watched kinds start a record**: the `logs` block tag, crafting table, furnace, nether bricks, spawner, obsidian. A position on record is then followed through every change, whatever it becomes.
- **No X-ray by construction**: a `Sighting` needs a `Seen`, and only `Seen::check` makes one, when a neighbour is loaded and its collision (generated from the game, [[datagen]]) is not a full cube. An unloaded neighbour does not count.
- **What the world looks at** (`World::observe_column`, `observe_around`): when a column loads, its sections whose palette may hold a watched kind, the positions already on record in it, and the facing edge cells of loaded neighbouring columns; after a block change, the cell and its six neighbours.

## Gotchas
- Water and lava are not watched as cells: clojurecraft found a fact per fluid cell froze a forest run (600,000 water cells). Fluids become per-chunk pools when an issue needs them.

## Limits
- Pools, intentions and server answers (clojurecraft's other facts) are not here yet.
- A live 2-minute session recorded 339 sightings in 329 columns; no query uses them yet.

## See also
- [[hist-no-xray]] - why the rule.
- [[entities-and-inventory]] - the other half of 04b.
