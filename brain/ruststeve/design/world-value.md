---
title: The world value
description: What ruststeve's World holds (the session, the dimension, chunk columns as received, the block overlay, the player's body) and how World::step folds each server packet into it; how block_at reads it; how the chunk decoder is checked against clojurecraft.
type: reference
tags: [world, chunks, overlay, fold, state]
aliases: [World, World::step, block_at, state_at, overlay, chunk columns, Event, Effect, world crate]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@34b2252 clojurecraft@be85a92
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
  - `player`: entity id, position and look from teleports (relative flags applied), health, food, saturation;
  - `entities`, `inventory` (window 0) and `memory` (sightings) ([[entities-and-inventory]], [[memory-sightings]]);
  - `held`, the dig `sequence_sent`/`sequence_acked`, the pending `click` and `pickups` (issue 10a, [[entities-and-inventory]], [[wire-actions]]);
  - `body`, `controls` and what was last sent: the physics body and what it presses ([[physics]]);
  - `hits`, `death` and `stage_reached`: the hits on the bot in the last minute, the server's death message, the highest stage held ([[entities-and-inventory]], [[race-and-results]]);
  - `now_ms`, `seed`.
- **Acts**: `Event::Act` is what the bot does (dig, hold, click, close, swing); the world turns it into the packet and the state it waits on, so an act is an input like a tick, recorded and replayed ([[wire-actions]]).
- **Chunks** are decoded on the net reader thread into `Column`s (and, since issue 08a, encoded back by `Column::encode` for the server model's generated worlds, [[server-model]]) whose sections stay packed paletted containers (no 4096-cell expansion); a cell is read on demand. A column's height comes from its section count: 24 starts at y −64, 16 at y 0.
- **Block changes** (`block_update`, `section_blocks_update`) go to the overlay; a column resent or forgotten drops its overlay. `state_at` reads the overlay first, then the column; `block_at` maps the state through `data::block_of_state`.
- **A dimension change** (respawn into another level) clears every column and the overlay: chunks belong to one dimension.

## Checked against clojurecraft
`tests/fixtures/expected.txt` holds 100 overworld and 100 Nether positions and the state clojurecraft's own `chunk/decode` and `block-at` (at be85a92) read there, from race recording `Clj_r12` (run 43143461648); the 40 sampled columns' raw chunk bytes sit beside it. The test loads them through the same packets the server sends and compares every position.

## Limits
- Other windows (tables, furnaces), fluid pools and intentions in memory are later issues.
- Collision shapes per state and prediction of the bot's own digs are later issues.

## See also
- [[world-model-fold]] - the design this implements.
- [[wire-chunk-sections]] - the wire format.
