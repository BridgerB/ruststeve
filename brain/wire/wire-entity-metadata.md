---
title: Entity metadata
description: How set-entity-data is laid out, which serializer ids are which, why the serializer table's order is load-bearing, and how to skim metadata safely.
type: reference
tags: [wire, entities, metadata]
aliases: [set-entity-data, entity data serializers, read-metadata, index 255, health index 9]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 steve@46c1264
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#\"How the metadata values the bot steps over are written, by serializer id: the game's"
  - "clojurecraft:src/clojurecraft/packet.clj#(def metadata-end 255)"
  - "clojurecraft:src/clojurecraft/dodge.clj#The blaze's charge flag (metadata index 16) is set 1.4 to 3.5 s before its fireballs spawn, with no direction in it."
  - "clojurecraft:src/clojurecraft/packet.clj#\"An entity's metadata as {index value}: entries of [index u8][serializer varint][value] up to"
  - "steve:CLAUDE.md#typecraft entity-metadata ordering is load-bearing"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-slots]]"
  - "[[cc-entities]]"
---

# Entity metadata

`set-entity-data` is a varint entity id followed by entries `[index u8][serializer varint][value]`, ended by index 255. The value's layout depends entirely on the serializer id.

## Key files
- clojurecraft `src/clojurecraft/packet.clj`: `metadata-readers`, `read-metadata`, `metadata-end`.

## How it works
- Serializer ids are the game's `EntityDataSerializers` **in registration order**: 0 byte, 1 int (varint), 2 long (varlong), 3 float, 4 string, 7 item stack (a slot), 8 boolean, 12 direction (varint), 15 optional block state (varint, 0 = none), 20 pose (varint). Those are the ones clojurecraft reads.
- `read-metadata` skims: it reads entries until index 255, and at the first serializer it has no reader for it stops and keeps what it has, as a frame's unread tail is ignored.
- Indices the bot uses: 0 shared flags (a bitfield: on fire, sneaking ...), 8 a dropped item's stack, 9 a living entity's health (a float), an enderman's 16 carried block and 17 creepy flag. Health is readable because everything before it (flags, air, pose, frozen ticks) is readable; only a custom name (index 2, a component) hides it.

## Gotchas
- **The serializer order is load-bearing.** One missing or extra entry shifts every later id. steve's `CLAUDE.md` records that one off entry in typecraft's index table desyncs the stream and drops the connection. A skim that stops at the first unknown serializer, as clojurecraft's does, cannot desync; a reader that guesses a width can.
- A blaze's "charging" state (its volley wind-up) is **not** the shared on-fire bit of index 0: clojurecraft reads it from the blaze's own metadata index 16, set 1.4 to 3.5 s before its fireballs spawn ([[mc-blaze]]). `CombatFrame.target_charging` ([[brain-events]]) must read that index, whatever the guide's wording.

## Limits
The full serializer list (components, particles, block positions, variants) is not here; clojurecraft never reads past them.

## See also
- [[datagen]] - ruststeve's serializer ids (`data::generated::metadata::ENTITY_DATA_SERIALIZERS`), read from `EntityDataSerializers` by the JNI probe, never hand-ordered.
- [[wire-slots]] - the item-stack serializer.
- [[cc-entities]] - what clojurecraft folds from these.
