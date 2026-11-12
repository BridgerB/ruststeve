---
title: Master index
type: index
tags: [index]
status: verified
lastUpdated: 2026-11-11
---

# ruststeve brain: master index

Read this first, pick a pillar, follow the links. New here? [[operating-manual]], [[conventions]], [[rules]], [[glossary]]. The brain's test suite is [[questions]]. Check it with `cargo run -q -p brain-check`.

## Pillars
- [[ruststeve/_moc|ruststeve]] - what we are building: the sans-IO core, the loop, determinism, recordings, the twenty-issue plan, and the decisions. Pinned to this repo.
- [[wire/_moc|Wire]] - Minecraft 26.1.2 / protocol 775 on the wire, as ruststeve reads and writes it itself. Pinned to the bots that already did it.
- [[oracle/_moc|Oracle]] - clojurecraft, the Clojure bot being ported: its closing handoff (start there), architecture, harness, data, namespaces and lessons.
- [[game/_moc|Game]] - vanilla facts: dimensions, digging, water, recipes, the portal cast, blazes, endermen.
- [[history/_moc|History]] - steve and ruststeve v1: why v1 was reset, what it learned, the walls every bot hits.

## Sources (`brain/sources`)
- `ruststeve`: this repo's working tree (today `FUNCTIONAL.md`, the porting guide; later the crates).
- `clojurecraft` at be85a92 (main 177d858be plus the closing handoff, PR #871), `steve` at 46c1264, `ruststeve-v1` at 9e1567f (this repo's main before the reset).

## Elsewhere
- `FUNCTIONAL.md` - the porting guide, written blind from a basic prompt; clojurecraft's `docs/HANDOFF-RUSTSTEVE.md` outranks it ([[cc-handoff]]).
- clojurecraft's own `brain/` - its internals in depth.
