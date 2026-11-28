---
title: Item slots and components
description: How an item stack is laid out on the wire (count, item, the data-component patch), which components the bot can step over, and why an unknown component truncates the frame.
type: reference
tags: [wire, items, slots, components]
aliases: [read-slot, ItemStack, data components, DataComponentPatch, damage, truncated frame]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: clojurecraft@be85a92 ruststeve@81480c0
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#\"{:item :count} (plus :components {type-name value} when the slot carries any) or nil for an"
  - "clojurecraft:src/clojurecraft/packet.clj#and ends what can be read of the frame.\""
  - "clojurecraft:src/clojurecraft/packet.clj#or :unit, nothing at all. A worn tool carries damage, and the game answers every use-item with"
  - "clojurecraft:src/clojurecraft/packet.clj#\"A slot as the server expects in a click: present flag, item, count, and no"
  - "ruststeve:crates/protocol/src/slot.rs#fn skip_potion_contents(r: &mut Reader<'_>) -> Result<bool, ParseError> {"
  - "ruststeve:crates/replay/src/bin/cuts.rs#fn cut_slots(p: &S2c) -> Vec<(&'static str, Slot)> {"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-container-click]]"
  - "[[wire-entity-metadata]]"
---

# Item slots and components

An item stack on the wire is a varint count (0 = empty), then the item id, then a data-component patch. Components are the hard part: each has its own codec, and one the reader cannot step over leaves the rest of the frame unlocatable.

## Key files
- clojurecraft `src/clojurecraft/packet.clj`: `read-slot`, `component-codecs`, `component-ids` (from `components.edn`), `read-hashed-slot`, `write-hashed-slot`, `truncating?`.

## How it works
- **Slot** (server → client): `count varint`; if positive, `item varint`, `added varint`, `removed varint`, then each added component as `type-id varint` + its value, then each removed type id (the game's `DataComponentPatch.STREAM_CODEC`).
- clojurecraft knows five component codecs: `max_stack_size`, `max_damage`, `damage` and `repair_cost` are varints, `unbreakable` is unit (no bytes). A worn tool carries `damage`, and the server answers every `use-item` with the whole inventory, so a slot after a worn tool must still be readable.
- A component type outside that table cannot be skipped: the slot is marked `:components?` and **ends what can be read of the frame**. A vector stops at that element; a struct is marked `:truncated`.
- **Hashed slot** (client → server, inside `container-click`): `present bool`, then `item`, `count`, and two empty lists (no component hashes). clojurecraft always writes nil or a bare item.

## Gotchas
- A container's full content arrives in one packet; one enchanted or named item early in the list hides every later slot until the codec learns that component.
- typecraft has a `component-hash.ts` and v1 a `component.rs` that compute real hashes; clojurecraft avoids needing them by never predicting ([[wire-container-click]]).

## Limits
Which other components appear in a speedrun inventory (enchantments, custom names, potion contents) is not surveyed here.

## Issue 16e
- `protocol::slot` keeps `damage` and `max_damage` in a `Wear` on the slot instead of stepping over them, and `Slot::Truncated` names the component it stopped at (`stopped_at`), so coverage lists the next reader to write. `write_slot` writes the wear back, for the server model.

## Issue 77: potion_contents, and the tool that finds a cut
- **`replay cuts <rsr> ...`** (`crates/replay/src/bin/cuts.rs`, read-only) reads a recording's raw packet bytes with today's decoders. It counts, for each file, every slot cut short (entity data, windows, the cursor, the player's inventory, equipment) by where it was and the component it stopped at. It also counts every entity-data read that stopped at a serializer (the world's `metadata_unread`, the coverage row) and every packet that would not decode. The first packets of each are printed in hex. Recordings are in a race's lane artifacts, not its batches asset; the lane that holds a coverage count is the one whose `batch.rsb` names it. This tool is what proves a fix: before a fix it names the component and gives the real bytes; after it, a re-read of the same lane shows the count gone.
- **`potion_contents`** (probe 46150486488: 389 witch-held potions in `set_equipment` in race 46140744043 lane 2) is `PotionContents.STREAM_CODEC` from the 26.1.2 jar (javap, probe 46151140850), a composite of four fields:
  - an optional potion, `Potion.STREAM_CODEC` (a registry id, varint);
  - an optional custom colour, `ByteBufCodecs.INT` (four bytes);
  - the custom effects, `MobEffectInstance.STREAM_CODEC` as a list (varint length);
  - an optional custom name, `STRING_UTF8`.
  Each optional is a bool, then the value. A witch's potion is `01 0d 00 00 00`: potion 13, no colour, no effects, no name.
- `protocol::slot` steps over the potion, colour and name. A list of custom effects that is not empty is left unread: a `MobEffectInstance` is its effect then a recursive `Details`, whose codec has not been read from the jar here. The slot then ends as `Truncated` at `potion_contents`, counted and never guessed.
- Tests built from real packets: `a_witchs_potion_reads_whole` (equipment) and `a_tipped_arrow_in_a_window_slot_reads_whole` (an inventory slot), with `a_thrown_potions_stack_reads_whole` in metadata. The cuts rerun (probe 46152583531) matched the coverage counts exactly: 131 ITEM_STACK in race 46140744043 lane 2; 44 ITEM_STACK and 8 window slots in race 46124595679 lane 0. Every one of them was `potion_contents`.

## See also
- [[entities-and-inventory]] - ruststeve's reader (`protocol::slot`): clojurecraft's component codecs, `Slot::Truncated` for the rest.
- [[wire-container-click]] - where hashed slots are sent.
- [[wire-entity-metadata]] - dropped items carry a slot in metadata.
