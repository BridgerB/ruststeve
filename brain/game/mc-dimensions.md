---
title: Dimensions
description: The height, lowest y, section count and coordinate scale of each dimension in 26.1.2, and what changes on a dimension switch.
type: reference
tags: [game, dimensions, nether, end]
aliases: [overworld, nether, the end, min-y, sections, logical height, coordinate scale, respawn]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:resources/clojurecraft/dimensions.edn##:dimension{:name \"minecraft:the_nether\""
  - "clojurecraft:src/clojurecraft/dimension.clj#\"The dimensions as data (resources/clojurecraft/dimensions.edn, generated from the server jar's"
  - "ruststeve-v1:CLAUDE.md#Dimension change clears loaded columns, entities and lava_seen, and resets world height (overworld −64/384, Nether/End 0/256)."
related:
  - "[[game/_moc|Game]]"
  - "[[wire-chunk-sections]]"
  - "[[cc-world-memory]]"
---

# Dimensions

## How it works

| Dimension | registry id | min y | height | sections | logical height | coordinate scale |
|---|---|---|---|---|---|---|
| overworld | 0 | −64 | 384 | 24 | 384 | 1.0 |
| overworld_caves | 1 | −64 | 384 | 24 | 384 | 1.0 |
| the_end | 2 | 0 | 256 | 16 | 256 | 1.0 |
| the_nether | 3 | 0 | 256 | 16 | 128 | 8.0 |

- A chunk packet carries exactly the dimension's section count, so decoding with a fixed 24 reads past a Nether column ([[wire-chunk-sections]]).
- The Nether column is 256 tall but its logical height is 128; blocks above y 127 still arrive.
- Coordinate scale 8: a Nether block is eight overworld blocks.
- `login` and `respawn` name the dimension type by registry id and the dimension by name.

## Gotchas
- **A dimension change must clear what was loaded**: columns, entities and anything derived from them (v1's `lava_seen`), and reset the world height. Remembered facts must carry their dimension; v1 once read Nether brick through overworld columns ([[cc-world-memory]]).

## See also
- [[wire-chunk-sections]] - section decoding.
- [[wire-connection-phases]] - where login and respawn carry it.
