---
title: Generated game tables
description: Which game tables clojurecraft generates from the vanilla server jar (blocks, items, recipes, hardness, packets, registries ...), how, and why ruststeve should generate the same rather than hand-type them.
type: reference
tags: [oracle, datagen, tables, registry]
aliases: [bb datagen, resources/clojurecraft, blocks.edn, recipes.edn, hardness.edn, generated data]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f ruststeve@6907139
sourceRefs:
  - "clojurecraft:bb.edn#datagen {:doc \"Regenerate the EDN tables in resources/clojurecraft/ from the server jar\""
  - "clojurecraft:src/clojurecraft/blocks.clj#\"Block-state, item and entity-type tables generated from the vanilla reports, and the hardness,"
  - "clojurecraft:src/clojurecraft/registry.clj#\"The registries a fight is told in, generated from the vanilla 26.1.2 server (bb datagen): damage"
  - "ruststeve-v1:CLAUDE.md#cargo run --bin datagen            # regenerate data/ (registry) from the Mojang jars — required before any run"
  - "ruststeve:crates/datagen/src/emit.rs#//! Rust source for `crates/data/src/generated/`, one file per table, formatted by rustfmt so the"
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[wire-packet-ids]]"
  - "[[cc-data-files]]"
---

# Generated game tables

Every bot needs the game's registries (block states, items, recipes, packet ids). All of them generate these from the vanilla server jar; none hand-types them.

## Key files
- clojurecraft `bb datagen`: regenerates `resources/clojurecraft/*.edn` from the 26.1.2 server jar, its `--reports`, its data files and its classes.
- clojurecraft `src/clojurecraft/blocks.clj`, `registry.clj`, `dimension.clj`, `recipe.clj`: read-only lookups by numeric id over the tables.
- ruststeve v1 `cargo run --bin datagen`: the same idea in Rust, writing a gitignored `data/` that every run requires.

## How it works
- From `--reports`: block states, items, entity types, packet ids, registries.
- From the jar's data files: recipes (tags resolved), loot tables (drops), worldgen biomes, dimension types.
- From the game's classes and tags: hardness, harvest tools, tool materials; damage types, attributes, mob effects, sounds in the order the server syncs them.
- Food values are hand-written in `food.clj`, as the game's own numbers.
- Generated files carry a "do not edit" header and are committed, so a version bump is one datagen run and a diff.

## For ruststeve
ruststeve does: `datagen` writes Rust tables and enums with `Unknown(u32)` into `crates/data`, committed, regenerated and diffed on a runner ([[datagen]]). clojurecraft commits its EDN; v1 regenerated a gitignored `data/` that a fresh checkout lacked, so the bot "spawns empty" without it. Prefer committed output.

## Gotchas
- Registry order is load-bearing wherever the wire sends an index (entity metadata serializers, damage types): generate it, never hand-maintain it ([[wire-entity-metadata]]).

## See also
- [[cc-data-files]] - every table, what it holds.
- [[wire-packet-ids]] - the packet table in particular.
