---
title: Memory as facts with time
description: How clojurecraft remembers what it has seen (an append-only DataScript store of sightings, intentions and server answers, per dimension), what it watches, and the no-X-ray rule it enforces.
type: reference
tags: [oracle, memory, facts, datascript]
aliases: [memory.clj, sightings, :world/facts, terrain overlay, pools, landmarks, no x-ray]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/memory.clj#\"What the bot has seen, as facts with time. The store is a DataScript database held as a value in"
  - "clojurecraft:src/clojurecraft/memory.clj#overworld's (ruststeve read Nether brick 64 blocks off through overworld columns). Pools likewise"
  - "clojurecraft:src/clojurecraft/memory.clj#Water and lava are remembered as pools, one fact per chunk and fluid kind:"
  - "clojurecraft:src/clojurecraft/terrain.clj#\"The blocks around the bot as values: chunk columns kept as they arrived, an overlay of block"
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-world-value]]"
  - "[[hist-no-xray]]"
---

# Memory as facts with time

## Key files
- `src/clojurecraft/memory.clj`: the store, `watched?`, `history`, `intention-as-of`, pools.
- `src/clojurecraft/terrain.clj`: chunk columns, the overlay of block updates and the bot's own breaks, and the questions physics and the planner ask.

## How it works
- **Sightings**: each observation is one fact `{:sight/pos :sight/state :sight/at :sight/dimension}`, appended only when what is seen at a position differs from the last one there. Nothing is retracted; a dug log is a newer fact of air, and history stays queryable.
- **Only watched kinds start a record**: logs, tables, furnaces, a fortress's bricks, a Nether spawner; a position on record is then followed through every change.
- **Per dimension**: every question is asked in the bot's current dimension. ruststeve v1 once read Nether brick 64 blocks off through overworld columns.
- **Pools**: water and lava as one fact per chunk and fluid kind (the highest surface source nearest the chunk centre). A fact per cell was the "stone freeze" (a forest recording held 600,000 water cells).
- **Intentions** are facts too (`:started :done :failed :abandoned`, with target or reason), so "what was I doing at time t" is a query.
- **Server answers** (acks with their sequence, pickups, deaths) are facts.
- Where water or lava is beside the body *now* is the terrain's question, never memory's.

## For ruststeve
The guide's `WorldModel` is a fold of observations; clojurecraft adds a second, append-only layer of facts with time on top. In Rust: an ordered `Vec` of facts plus `BTreeMap` indexes (latest by position, by kind), rebuilt by the same fold. No Datalog engine is needed for the handful of queries used; the queries become functions.

## Gotchas
- Memory only holds what was seen with an exposed face; that is the no-X-ray rule ([[hist-no-xray]]).

## See also
- [[cc-world-value]] - where `:world/facts` sits.
- [[world-model-fold]] - the Rust fold.
