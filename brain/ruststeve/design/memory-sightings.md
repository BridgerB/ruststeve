---
title: Memory and sightings
description: How ruststeve remembers watched blocks (logs, tables, furnaces, nether bricks, spawners, obsidian, nether portals, a stronghold's frames, bricks and infested blocks) as append-only sightings with time, only when seen with an exposed face, indexed by position and chunk column; how the world decides what to look at.
type: reference
tags: [memory, sightings, no-xray, facts, world]
aliases: [Memory, Sighting, Seen, watched kinds, exposed face, memory crate, facts with time]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@bd9b55c clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/memory/src/lib.rs#/// A block seen with an exposed face. Only `Seen::check` makes one."
  - "ruststeve:crates/memory/src/lib.rs#pub fn record_source(&mut self, seen: SeenSource) -> bool {"
  - "ruststeve:crates/memory/src/lib.rs#pub fn record(&mut self, seen: Seen, at_tick: u64) -> bool {"
  - "ruststeve:crates/memory/src/lib.rs#pub fn record_sign(&mut self, seen: &Seen) -> bool {"
  - "ruststeve:crates/memory/src/lib.rs#pub struct ChestSeen {"
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
- Indexes: the latest sighting by position, and positions by dimension and chunk column. Since issue 07a also every block kind ever sighted (`has_seen`), so the stage ladder asks "seen nether bricks?" without a scan. `refold_latest` rebuilds the index from the log; the property test holds them equal.
- **Only watched kinds start a record**: the `logs` block tag, crafting table, furnace, nether bricks, spawner, obsidian, and (issue 15c) nether portal, a lit portal to walk back to; (issue 19b) end portal frame and end portal (the portal room's frames face its air, so chunk data shows them within the view distance), stone bricks and their mossy, cracked and chiseled kinds (a stronghold's walls), and the seven infested kinds (`memory::INFESTED`: never dug); (issue 20b) in the End only (`memory::watched_in`, `END_WATCHED`), end stone (the island's ground and the dragon-immune pillar block), iron bars (two spikes' cages) and bedrock (the spikes' tops, the exit portal). Bedrock floors every Overworld and Nether chunk and roofs the Nether: watched there it put every column on record, and the heading's seen-columns count stopped meaning anything (CI 45868182237, `a_way_only_back_over_the_cut_is_refused_and_said`). Frames, the End portal, stone bricks, the infested kinds, iron bars and bedrock are in the kind index (`INDEXED`) for the stage ladder's look every second; end stone is not (the island's whole surface). `memory::frame_has_eye(state)` reads a frame's `eye` property from the generated table: the frontier's 12 frames on record read as RCON did (probe 45863427332: one eye). A position on record is then followed through every change, whatever it becomes.
- **Chests** (issue 77): `chest` is a watched kind. When a cell's recorded kind becomes a chest, `Memory::chests` gets a `ChestSeen` for it, and when the kind changes away from a chest the entry is dropped. That covers a chest the bot placed (the block update after the place) and one seen exposed. Its `contents` are `None` until the chest's window shows them, and `chest_contents` takes window contents only for a chest already on record ([[chests]]).
- **No X-ray by construction**: a `Sighting` needs a `Seen`, and only `Seen::check` makes one, when a neighbour is loaded and its collision (generated from the game, [[datagen]]) is not a full cube. An unloaded neighbour does not count.
- **What the world looks at** (`World::observe_column`, `observe_around`): when a column loads, its sections whose palette may hold a watched kind, the positions already on record in it, and the facing edge cells of loaded neighbouring columns; after a block change, the cell and its six neighbours.

## Gotchas
- Water and lava are not watched as cells: clojurecraft found a fact per fluid cell froze a forest run (600,000 water cells). Fluids are per-chunk pools (below).

## Limits
- Intentions and server answers (clojurecraft's other facts) are not here yet.
- A live 2-minute session recorded 339 sightings in 329 columns.

## Issue 14b: fluid pools
- Water and lava are kept as **pools**, not cells: per dimension, chunk column and fluid, at most `POOL_CELLS` (32) exposed sources, the first seen. One fact per fluid cell froze clojurecraft's forest run (600,000 water cells).
- Only `SeenSource::check` makes one: **water** with air straight over it (a surface a bucket reaches; water under a ceiling is the aquifer trap), **lava** with an air neighbour. A source buried in rock never enters memory (strict no X-ray, Bridger 2026-11-15).
- The world scans a column's fluid sections top first on arrival, stopping once a pool is full, and re-checks a cell and its neighbours on every block change; a cell that holds no source now is forgotten.
- `sources_near` answers the scoop task ([[buckets-and-fluids]]) and, in 14c, the cast's site choice.

## Issue 17b: biome signs and seen columns
- **Biome signs** (`Memory::record_sign`, `Sign`, `sign_of`): an exposed face of warped nylium, warped wart block, warped stem or hyphae (warped forest), crimson nylium or nether wart block (crimson forest), soul sand or soul soil (soul sand valley), basalt or blackstone (basalt deltas) is kept by dimension, chunk column and sign, at most `SIGNS_KEPT` (64) cells each, so a soul sand valley stays a small record. Signs are not sightings: they do not enter the log, and a sign is never retracted (evidence of where the biome grew).
- **A known forest** (`forest_known`, `forests`): a Nether column with `FOREST_SIGNS` (12, clojurecraft's nylium threshold) warped signs, kept in the order known and after the column unloads. `biome_signs_in_column(cx, cz)` counts a Nether column's signs.
- **A known soul sand valley** (`soul_valley_known`, `soul_valleys`): a Nether column with `SOUL_SIGNS` (12, the forest's threshold by analogy) soul sand or soul soil seen; the first target of issue 17 (the valley 32 blocks west of the frontier's spawner). Soul soil the bot placed itself counts too (it carries 50): a known gap.
- **Sign kinds through the kind index**: `indexed_in_column` answers a sign kind (soul sand, soul soil, warped nylium and the rest of `SIGN_KINDS`) from the sign record (exposed cells, at most 64 a kind and column), so `stage::kind_near` gets exact cells with no scan. A cell seen to hold something else leaves the record (`forget_sign`, from `World::observe`); a column known as a forest or a valley stays known.
- **The world's scan** (`World::observe_column_signs`): a column's sections tested by palette (`is_sign`), each sign cell's six faces read inside the section where it can, a sign whose record is full skipped. A changed block is checked by `observe`. Faces on the edge towards an unloaded column are not re-checked when it loads (the watched kinds' edge pass does not cover signs).
- **Seen columns** (`seen_columns(dimension)`): the column keys of `by_column` for one dimension, by a range of the index (issue 17d's heading counts seen columns).
- Bridger, 2026-11-24 ([[strict-no-xray]]): the planner may know biomes only from its feet and from these signs, never from an unseen column's biome.

## Issue 20 part 2: crystals destroyed

- `crystals_destroyed` (`record_crystal_destroyed`): the End's spikes (centre columns, `world::stage::SPIKES`) whose crystal was seen destroyed, a removal with its blast at its spot (the world decides, `World::crystal_removed` and `crystal_blast`), or a spike seen bare for `CRYSTAL_ABSENT_TICKS` (400) running with the bot within 60 of the fountain (`crystals_absent_tick`: a development start's RCON kill). It lives in memory so a snapshot's resume keeps the count; older stores read it as empty. The stages `crystals_5`, `crystals_1` and `crystals` count from it, never from the crystals tracked (race 46044026128: a resume's re-send read as "5 left" on tick 40 in every lane).

## See also
- [[hist-no-xray]] - why the rule.
- [[entities-and-inventory]] - the other half of 04b.
