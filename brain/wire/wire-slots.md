---
title: Item slots and components
description: How an item stack is laid out on the wire (count, item, the data-component patch), which components the bot can step over, and why an unknown component truncates the frame.
type: reference
tags: [wire, items, slots, components]
aliases: [read-slot, ItemStack, data components, DataComponentPatch, damage, truncated frame]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#\"{:item :count} (plus :components {type-name value} when the slot carries any) or nil for an"
  - "clojurecraft:src/clojurecraft/packet.clj#and ends what can be read of the frame.\""
  - "clojurecraft:src/clojurecraft/packet.clj#or :unit, nothing at all. A worn tool carries damage, and the game answers every use-item with"
  - "clojurecraft:src/clojurecraft/packet.clj#\"A slot as the server expects in a click: present flag, item, count, and no"
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

## See also
- [[entities-and-inventory]] - ruststeve's reader (`protocol::slot`): clojurecraft's component codecs, `Slot::Truncated` for the rest.
- [[wire-container-click]] - where hashed slots are sent.
- [[wire-entity-metadata]] - dropped items carry a slot in metadata.
