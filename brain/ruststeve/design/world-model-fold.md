---
title: The world model is a fold
description: Why the world model is only ever changed by applying observations, and what shape it has.
type: reference
tags: [world-model, observations, core]
aliases: [WorldModel, apply, rebuild, observation fold, derived index]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#The event log is append-only. The WorldModel is a derived index: always rebuildable by folding observations."
  - "ruststeve:FUNCTIONAL.md#pub entities: BTreeMap<EntityId, EntityRecord>,     // ordered map: deterministic iteration"
  - "ruststeve:FUNCTIONAL.md#pub blocks: [BlockKind; 4096],                      // 16x16x16, flat array, index = (y*16 + z)*16 + x"
  - "ruststeve:FUNCTIONAL.md#Smell. The world model gets modified anywhere other than apply. State that cannot be reconstructed from the log."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[sans-io-core]]"
  - "[[domain-types]]"
  - "[[cc-world-value]]"
---

# The world model is a fold

Facts accrete; they are never edited. The log of observations is the source of truth and `WorldModel` is a derived index, always rebuildable by folding the observations in order. Any cache inside it must be rebuildable the same way.

## Key files
- `FUNCTIONAL.md`, section 3.4 (`WorldModel::apply`, `rebuild`) and section 5.2 (the shape).

## How it works
- `Observation` is an enum, one variant per kind of fact the shell can report (`EntityMoved`, `EntityRemoved`, `HealthChanged`, `BlockChanged`, `InventoryChanged`, ...).
- `WorldModel::apply(&mut self, &Observation)` matches exhaustively; a new variant does not compile until it is handled.
- `rebuild(log)` is `fold(WorldModel::default(), apply)`.
- Shape: `me: SelfState`, `entities: BTreeMap<EntityId, EntityRecord>`, `blocks: ChunkStore` (sparse: only chunks the server sent; `Chunk { sections: Vec<Option<Box<Section>>> }`, `None` = all air; `Section { blocks: [BlockKind; 4096] }`, index `(y*16 + z)*16 + x`), `dimension`, `last_tick`.
- Readers on other threads see chunks as `BTreeMap<ChunkPos, Arc<Chunk>>`; the core writes through `Arc::make_mut`, which copies a chunk only while a reader still holds it.

## Gotchas
- The world model being changed anywhere other than `apply`, or holding state the log cannot rebuild, is the smell to catch in review.
- Our own wire layer decodes chunk sections from paletted containers ([[wire-chunk-sections]]); the core's flat array is the decoded form, not the wire form.

## Limits
`SelfState`, `EntityRecord` and the exact observation set are not fixed by the guide; they come from what clojurecraft folds from packets ([[cc-world-value]]).

## See also
- [[domain-types]] - the newtypes and context types the model is built from.
- [[determinism-rules]] - why `BTreeMap`.
