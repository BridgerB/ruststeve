---
title: Entities and the inventory
description: How the world folds entities (adds, moves, syncs, teleports, metadata, removals) and window 0, the player's inventory (content, slot and player-inventory packets, menu slots, truncated slots); what is counted rather than invented.
type: reference
tags: [world, entities, metadata, inventory, slots]
aliases: [Entity, entities map, Inventory, window 0, menu slots, set_player_inventory, MetaValue, Slot, truncated slot, health index 9]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@5a0f8e2 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/world/src/lib.rs#pub fn menu_slot(index: i32) -> Option<i16> {"
  - "ruststeve:crates/world/src/lib.rs#fn place_entity(&mut self, id: i32, [x, y, z]: [f64; 3], flags: u32) {"
  - "ruststeve:crates/protocol/src/metadata.rs#pub fn read_metadata(r: &mut Reader<'_>) -> Result<Vec<(u8, MetaValue)>, ParseError> {"
  - "ruststeve:crates/protocol/src/slot.rs#pub fn read_slot(r: &mut Reader<'_>) -> Result<Slot, ParseError> {"
  - "clojurecraft:src/clojurecraft/packet.clj#defn read-slot"
  - "clojurecraft:src/clojurecraft/packet.clj#[:play :s2c :move-entity-pos] [[:entity-id :varint] [:dx :i16] [:dy :i16] [:dz :i16]"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[world-value]]"
  - "[[wire-entity-metadata]]"
  - "[[wire-slots]]"
---

# Entities and the inventory

## Entities
- `World::entities: BTreeMap<i32, Entity>`: one entry per id by construction, never the player's own id. `Entity { kind, pos, metadata, first_seen, last_seen }`, times in server ticks.
- `add_entity` inserts (id, uuid, type, position; the velocity tail is ignored), `remove_entities` drops, `move_entity_pos` and `move_entity_pos_rot` add deltas of 1/4096 block, `entity_position_sync` sets the position, `teleport_entity` sets it with relative flags as `player_position`'s. A move or data for an id the world does not hold is counted in `unknown_entity_updates`, never invented.
- `set_entity_data` merges skimmed metadata ([[wire-entity-metadata]]); `Entity::health` reads index 9, `flags` index 0.
- Non-finite coordinates are a `ParseError` at the boundary, and the fold ignores any that a built packet slips in.
- A change of dimension drops every entity.

## The inventory (window 0)
- `World::inventory: Inventory { slots: BTreeMap<menu slot, (ItemKind, count)>, state_id, carried, truncated }`: only slots holding at least one item are present, so no count is ever zero or negative.
- `container_set_content` (window 0) replaces it, `container_set_slot` sets one menu slot, `set_player_inventory` sets one by inventory index, mapped by `Inventory::menu_slot`: 0-8 → 36-44, 9-35 stay, 36-39 armour (feet to head) → 8-5, 40 → 45.
- Slots read the components clojurecraft reads (`max_stack_size`, `max_damage`, `damage`, `repair_cost`, `unbreakable`); any other ends the frame as `Slot::Truncated` (the item and count still right) and is counted in `truncated` ([[wire-slots]]).

## Tests
`crates/world/tests/properties.rs`: any sequence of adds, moves, syncs, teleports, data, removals, inventory packets, chunks and block updates folds without panic; no entity holds the player's id; every position is finite; every count is at least 1.

## Limits
- A live session so far ran with an empty inventory (a fresh player); inventory folding is shown by the tests.
- Other windows (tables, furnaces) and clicks are later issues.

## See also
- [[world-value]] - the world these live in.
- [[memory-sightings]] - the other half of 04b.
