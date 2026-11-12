---
title: Datagen and the data crate
description: How ruststeve's game tables are made (datagen downloads the 26.1.2 server, runs its reports, reads its inner jar, writes Rust into crates/data), how drift and unknown packet ids are caught on a runner, and how to regenerate.
type: how-to
tags: [datagen, data, tables, ci, packets]
aliases: [crates/data, generated tables, JNI probe, jar probe, hardness, fuel values, collision shapes, entity data serializers, --oracle, datagen.yml, verify-stream, regenerate the tables, BlockKind, ItemKind, packet enums]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@a02c9a0 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/datagen/src/main.rs#//! datagen: the game's tables, generated from the vanilla server itself and committed as Rust in"
  - "ruststeve:crates/datagen/src/fetch.rs#pub fn server_jar(work: &Path) -> Result<PathBuf> {"
  - "ruststeve:crates/datagen/src/model.rs#fn tags_of("
  - "ruststeve:crates/datagen/src/emit.rs#fn kind_enum(ty: &str, table: &str, names: &[String]) -> Result<String> {"
  - "ruststeve:.github/workflows/datagen.yml#git diff --exit-code --stat crates/data"
  - "ruststeve:crates/verify-stream/src/main.rs#fn offline_uuid(name: &str) -> [u8; 16] {"
  - "ruststeve:crates/data/tests/counts.rs#fn packet_counts_match() {"
  - "ruststeve:crates/datagen/src/probe.rs#//! Only `java.lang.reflect` and other JDK methods are called by JNI signature; every game method is"
  - "ruststeve:crates/datagen/src/oracle.rs#pub fn compare(dir: &Path, p: &Probe) -> Result<()> {"
  - "clojurecraft:dev/clojurecraft/datagen.clj#defn tags-of"
  - "clojurecraft:dev/clojurecraft/jar_probe.clj#\"Ask the vanilla server itself what the --reports output does not say: every block's"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[cc-datagen]]"
  - "[[wire-packet-ids]]"
  - "[[workspace-crates]]"
---

# Datagen and the data crate

Every game table ruststeve uses is generated from the vanilla 26.1.2 server and committed as Rust in `crates/data/src/generated/`. Nothing is hand-typed: ruststeve v1's placeholder hardness failed half its digs, and clojurecraft's generated tables are why its early stages came up in a week ([[cc-datagen]]).

## Key files
- `crates/datagen` (binary, runners only): `fetch.rs` (manifest, sha1-checked jar, `--reports`), `jar.rs` (the inner jar's data files), `model.rs` (each table read and checked, following clojurecraft's `datagen.clj`), `emit.rs` (Rust out, then rustfmt).
- `crates/data` (a core crate, no dependencies): `types.rs` hand-written shapes, `generated/` the tables, `tests/counts.rs` the counts against clojurecraft.
- `crates/verify-stream`: a raw offline client that logs in to a real server and fails on any packet id the tables lack.
- `.github/workflows/datagen.yml`: generate → upload → clippy and test `data` → start a server → `verify-stream` → drift check.

## How it works
1. `datagen generate` reads Mojang's version manifest, downloads `server.jar` for 26.1.2, checks its sha1 and runs `java -DbundlerMainClass=net.minecraft.data.Main -jar server.jar --reports` (Java 25). The jar and reports are cached by the workflow under a version key.
2. From the reports: packet ids per state and direction, the block, item and entity-type registries, block states (ranges per block, properties in report order) and the registries the wire sends by index (menus, attributes, mob effects, sounds, data components, particles, potions, fluids).
3. From the inner jar (`META-INF/versions/26.1.2/server-26.1.2.jar`): `version.json`, item and block tags (nested `#tag`s resolved, cycles cut), shaped and shapeless crafting recipes (patterns trimmed like vanilla), smelting, block loot (the first pool for a plain tool), dimension types, damage types and biomes (sorted by name, the order the server syncs them).
4. Every table is checked as it is read: ids run 0..n with no gaps, a block's state range equals the product of its property values, every item and block a tag, recipe or loot table names exists. A failure stops the run; nothing is defaulted.
5. `emit.rs` writes one file per table: kind enums (`BlockKind`, `ItemKind`, `EntityKind`) with `Unknown(u32)` and `from_id`/`id`/`name`/`from_name`; packet enums such as `PlayClientbound` with `ALL`, `from_id`, `id`, `name`; sorted statics with binary-search lookups (`block_of_state`, `item_tag`, `drops_of`, `harvest_of`). No `HashMap` ([[determinism-rules]]). rustfmt runs on the output, so a rerun writes the same bytes.

## The probe (the game's classes)
Hardness, needs-correct-tool, collision shapes, tool materials, fuel, mob health and category, and the entity-data serializer ids are in no data file. `probe.rs` starts a JVM from Rust (`jni` 0.21, `invocation`: libjvm is found through `JAVA_HOME` and loaded at run time, so the crate builds without Java) with the inner jar and the bundler's `libraries/` on its class path, bootstraps the game and reads them by reflection, mirroring clojurecraft's `jar_probe.clj`. Only JDK methods are called by JNI signature; each game method is found by name, argument count and argument types at run time and cached, so a renamed return type cannot break it. `--oracle <clojurecraft>/resources/clojurecraft` then compares every probe table with clojurecraft's EDN entry by entry and fails on any difference.

## Regenerating
On a runner only. Push a change under `crates/datagen`, `crates/data` or `crates/verify-stream` (or dispatch `datagen`); download the `generated-<run id>` artifact with `gh run download` and copy it over `crates/data/src/generated/`. The last step of the workflow fails while the committed files differ from the regenerated ones.

## Gotchas
- Registry order is load-bearing wherever the wire sends an index; the static registries keep protocol-id order, the data-driven ones (damage types, biomes, dimension types) are sorted by name as clojurecraft found the server syncs them.
- Block properties must keep the report's order (serde_json's `preserve_order`): a state id counts through them with the last property fastest.
- The probe needs the bundler's extracted `versions/` and `libraries/` in the work directory; they are written when the reports run and cached with them.
- Collision shapes are the default state's only (a slab's bottom half); per-state shapes are issue 06's if physics needs them.

## Limits
- The stream check answers only what keeps a session alive for 20 s; it is scaffolding, not the protocol layer (issue 03).
- Damage-type and biome order is taken from clojurecraft's finding, not yet checked against a captured `registry_data` packet.

## See also
- [[cc-datagen]] - clojurecraft's version of this.
- [[wire-packet-ids]] - the packet table in particular.
