---
title: Chunk sections
description: How level-chunk-with-light's chunk data decodes into sections of paletted containers, the 26.1 layout changes, and how a block is found by index.
type: reference
tags: [wire, chunks, palette, blocks]
aliases: [paletted container, level-chunk-with-light, section, bits per entry, block-index, biomes]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/chunk.clj#with palette nil above max-bits (direct ids). 26.1 sends no long count; it is"
  - "clojurecraft:src/clojurecraft/chunk.clj#\"One section: block count, fluid count (26.1+), block container, biome container; returns"
  - "clojurecraft:src/clojurecraft/chunk.clj#\"Index of local cell (lx ly lz), each 0-15, inside a section: y, then z, then x.\""
  - "clojurecraft:src/clojurecraft/chunk.clj#\"The id at index idx (0-4095 for blocks) of a section, through the palette when"
  - "clojurecraft:src/clojurecraft/packet.clj#[[:x :i32] [:z :i32] [:heightmaps [:vec [[:type :varint] [:data [:vec :i64]]]]] [:data :bytes]]"
related:
  - "[[wire/_moc|Wire]]"
  - "[[world-model-fold]]"
  - "[[mc-dimensions]]"
---

# Chunk sections

A column arrives in `level-chunk-with-light`: chunk x and z (i32), heightmaps, then a varint-length byte array holding every section bottom to top (light data follows and is ignored). Each section is two paletted containers.

## Key files
- clojurecraft `src/clojurecraft/chunk.clj`: `decode`, `read-section-with-biomes`, `read-container`, `section-get`, `block-index`, `section-find`.

## How it works
- **Section** (26.1+): block count (i16), **fluid count (i16, new in 26.1)**, the block container (4096 entries, palette up to 8 bits), the biome container (64 entries, palette up to 3 bits).
- **Paletted container**: `bits u8`. Bits 0 → a single value (varint) for the whole section. Otherwise, at or under the max bits a palette (varint count, varint ids) follows; above it ids are direct. Then the packed longs. **26.1 sends no long count**: it is `ceil(entries / floor(64 / bits))`.
- Entries never straddle longs: entry `i` sits in long `i / (64/bits)` at bit `bits * (i % (64/bits))`.
- **Index** of local cell `(x, y, z)`: `y << 8 | z << 4 | x`. Biomes: cells of 4×4×4, same y-z-x order over 64 entries.
- **How many sections**: the server writes exactly its dimension's count: 24 from y −64 in the overworld, 16 from y 0 in the Nether and the End. clojurecraft derives min-y from the count ([[mc-dimensions]]).
- clojurecraft keeps sections packed (no expansion to 4096) and tests a palette before scanning; scanning every cell lazily was a tenth of a race's CPU.

## Gotchas
- Forgetting the 26.1 fluid count, or reading a long count that is no longer sent, shifts every later byte.
- `section-blocks-update` packs a section position in an i64 and each change as a varlong; `block-update` carries a packed position and a state id.

## See also
- [[world-model-fold]] - the core's decoded form.
- [[cc-world-memory]] - what clojurecraft remembers from these.
