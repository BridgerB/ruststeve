---
title: The world value
description: What ruststeve's World holds (the session, the dimension, chunk columns as received, the block overlay, the player's body) and how World::step folds each server packet into it; how block_at reads it; how the chunk decoder is checked against clojurecraft.
type: reference
tags: [world, chunks, overlay, fold, state]
aliases: [World, World::step, block_at, state_at, overlay, chunk columns, Event, Effect, world crate]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/world/src/lib.rs#pub fn step(&mut self, e: &Event) -> Vec<Effect> {"
  - "ruststeve:crates/world/src/lib.rs#pub fn state_at(&self, pos: BlockPos) -> Option<u32> {"
  - "ruststeve:crates/protocol/src/chunk.rs#pub fn decode(data: &[u8]) -> Result<Self, ParseError> {"
  - "ruststeve:crates/world/tests/oracle.rs#fn overworld_matches_clojurecraft() {"
  - "clojurecraft:src/clojurecraft/chunk.clj#defn block-at"
  - "clojurecraft:src/clojurecraft/terrain.clj#defn section-update"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[world-model-fold]]"
  - "[[wire-chunk-sections]]"
  - "[[cc-world-value]]"
---

# The world value

`World` (crate `world`, no I/O) is everything the bot knows, and `World::step(&mut self, &Event) -> Vec<Effect>` is the only way it changes. An `Event` is a server packet or a tick of the loop's clock (`now_ms`, `seed`); an `Effect` is a packet to send or the compression switch.

## Key files
- `crates/world/src/lib.rs`: `World`, `Event`, `Effect`, `BlockPos`, `ChunkPos`, `Dimension`, `Player`.
- `crates/protocol/src/chunk.rs`: `Column::decode`, `Paletted`, `state_at`, a port of clojurecraft's `chunk.clj`.
- `crates/world/tests/oracle.rs` and `tests/fixtures/`: the decoder checked against clojurecraft.

## How it works
- **What it holds**, each field documented in the code as `model.clj` documents clojurecraft's keys ([[cc-world-value]]):
  - `session`: protocol upkeep; its replies become the step's effects ([[connection]]);
  - `dimension`: type id, name, `min_y` and height from the generated dimension types;
  - `columns`: `BTreeMap<ChunkPos, Arc<Column>>`, as received;
  - `overlay`: block changes by column;
  - `player`: entity id, position and look from teleports (relative flags applied), health, food, saturation; air, on fire, effects, attributes, in combat, experience;
  - `entities`, `inventory` (window 0) and `memory` (sightings) ([[entities-and-inventory]], [[memory-sightings]]);
  - `held`, the dig `sequence_sent`/`sequence_acked`, the pending `click` and `pickups` (issue 10a, [[entities-and-inventory]], [[wire-actions]]);
  - `body`, `controls` and what was last sent (position, look, and `sent_input`, the keys): the physics body and what it presses ([[physics]]);
  - `hits`, `death` and `stage_reached`: the hits on the bot in the last minute, the server's death message, the highest stage held ([[entities-and-inventory]], [[race-and-results]]);
  - `window` and `closed_window`: the window open besides the inventory and the one the bot last closed ([[table-window]], [[wire-container-click]]); `kills` ([[entities-and-inventory]]);
  - what the bot hears and is told (issue 16e), each list the newest 64 (`SOUNDS_KEPT`): `sounds`, `level_events`, `explosions`, `game_events`, `chat`; and `bosses`, `spawner_mobs` (each spawner's mob from its block-entity data), `cracks`, `cooldowns`, `hurt_from`, `won` (the `win_game` game event), `dragon_dead` (issue 20b: the server tick of the dragon's entity event 3, or its boss bar removed in the End), `attacks_sent` and `last_attack` (the strikes the ledger names), `crystals_removed` and `crystals_absent` (issue 20 part 2: a crystal's removal held for its blast, a spike seen bare; a destruction goes to memory, [[entities-and-inventory]]), `chunk_center`, `view_radius`, `simulation_distance`, `biome_updates`;
  - `game_time` and `day_clock`: the level's game time and the overworld clock from the newest `set_time` (issue 16, `docs/problems/day-clock.md`). `World::day_time()` is the time of day, 0 to 23,999, run on from the last overworld clock by the game ticks since; `World::is_night()` and the pure `world::time` helpers (`is_night`, `ticks_to_dusk`, `ticks_to_dawn`, `DUSK` 12542 and `DAWN` 23460 from the jar's `timeline/day.json` `monsters_burn` track) are what the planner asks;
  - `now_ms`, `seed`.
- **Acts**: `Event::Act` is what the bot does (dig, hold, click, close, swing, resync, use on a block, attack, use item, use the off hand, release use, respawn); the world turns it into the packet and the state it waits on, so an act is an input like a tick, recorded and replayed ([[wire-actions]]).
- **Chunks** are decoded on the net reader thread into `Column`s (and, since issue 08a, encoded back by `Column::encode` for the server model's generated worlds, [[server-model]]) whose sections stay packed paletted containers (no 4096-cell expansion); a cell is read on demand. A column's height comes from its section count: 24 starts at y −64, 16 at y 0.
- **Block changes** (`block_update`, `section_blocks_update`) go to the overlay; a column resent or forgotten drops its overlay. `state_at` reads the overlay first, then the column; `block_at` maps the state through `data::block_of_state`.
- **A dimension change** (respawn into another level) clears every column, the overlay, every entity and the body: chunks belong to one dimension. Memory keeps everything, by dimension.

## Checked against clojurecraft
`tests/fixtures/expected.txt` holds 100 overworld and 100 Nether positions and the state clojurecraft's own `chunk/decode` and `block-at` (at be85a92) read there, from race recording `Clj_r12` (run 43143461648); the 40 sampled columns' raw chunk bytes sit beside it. The test loads them through the same packets the server sends and compares every position.

## Limits
- Intentions in memory are a later issue.
- Collision shapes per state and prediction of the bot's own digs are later issues.

## Issue 17b: biomes and ticking
- `World::feet_biome()` reads the biome container of the section at the feet (index `((y>>2)&3)<<4 | ((z>>2)&3)<<2 | ((x>>2)&3)`), named by `biome_registry` (the `registry_data` packet for `minecraft:worldgen/biome`, entry names in the server's order) or, before it, the generated `BIOME` table (sorted by name, the order the server syncs). `biome_at` is private to the crate: the planner reads only its feet (Bridger, 2026-11-24). Tested on clojurecraft's real Nether columns (`biome::tests`).
- `world::ticks(w, pos)`: the entity's chunk within `simulation_distance` of the body's chunk on both axes (the jar: entity-ticking chunks are a square, probe 45472254333); false with no body or no distance told. At distance 3 anything within 32 blocks ticks.
- Chunk biome updates (`chunks_biomes`) are still only counted (`biome_updates`), not applied.

## See also
- [[world-model-fold]] - the design this implements.
- [[wire-chunk-sections]] - the wire format.
