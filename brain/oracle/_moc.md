---
title: Oracle pillar
type: moc
tags: [pillar, hub, oracle, clojurecraft]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
related:
  - "[[_index]]"
---

# Oracle pillar

clojurecraft, the Clojure bot ruststeve ports, read at be85a92 (main 177d858be plus its closing handoff). It was shut down on 2026-11-12 (workflows disabled, not deleted); its code, data and recordings stay the oracle until ruststeve passes it ([[clojure-is-the-oracle]]). Its own `brain/` describes its internals in depth; these notes are what a porter needs.

## Start here
- [[cc-namespaces]] - every namespace in one table; the seed of `INVENTORY.md`.
- [[cc-to-rust-map]] - each clojurecraft mechanism and its Rust form.
- [[cc-lessons]] - the lessons register; the seed of `LESSONS.md`.
- [[cc-decisions]] - clojurecraft's recorded decisions and which ruststeve inherits.

## Areas
- [[oracle/handoff/_moc|Handoff]] - **read first**: where clojurecraft got, why stage 18 stalled, the unmerged leads, the operating loop. Outranks `FUNCTIONAL.md`.
- [[oracle/architecture/_moc|Architecture]] - the world value, the reducer, the loop, goals and intents, crafting, combat, memory, physics.
- [[oracle/harness/_moc|Harness]] - recording, the server model and sim race, tests, datagen, gyms and races, the stage list.
- [[oracle/data/_moc|Data]] - the generated and hand-written tables.

## See also
- [[_index]]
- [[history/_moc|History]] - the bots before clojurecraft.
