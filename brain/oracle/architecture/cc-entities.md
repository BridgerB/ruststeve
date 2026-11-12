---
title: Entities and what the bot hears
description: How clojurecraft tracks entities from packets (positions, velocities, health and flags from metadata, deaths stated by the server) and the extra senses it added (hurt flashes, swings, sounds, effects).
type: reference
tags: [oracle, entities, senses, observability]
aliases: [:world/entities, entity.clj, hear.clj, ear.clj, eye.clj, damage-event, entity-event]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/entity.clj#\"The entities the server shows the bot, as values: every one it adds is kept in :world/entities by"
  - "clojurecraft:src/clojurecraft/hear.clj#\"What the server says about a fight beyond the health and the damage-event, kept as facts in the"
  - "clojurecraft:src/clojurecraft/ear.clj#Vanilla plays a blaze's shot as level event 1018 and an enderman's stare, scream and teleport as sounds,"
  - "clojurecraft:src/clojurecraft/derived.clj#Why: 14,156 of 19,018 hits over 450 runs named no attacker, 62% of all hits were burning, and what lit the bot was never said."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[wire-entity-metadata]]"
  - "[[world-model-fold]]"
---

# Entities and what the bot hears

## Key files
- `src/clojurecraft/entity.clj`: `:world/entities` and the questions a fight asks (hostile, nearest, gone).
- `src/clojurecraft/hear.clj`, `ear.clj`, `eye.clj`, `chat.clj`: extra senses, each a `defmethod` of `game/on-packet`.
- `src/clojurecraft/derived.clj`: facts from two successive worlds.

## How it works
- Every added entity is kept by id: `:entity/type`, `:entity/pos`, `:entity/seen-at`, `:entity/vel` once given, for a living one `:entity/health` and `:entity/flags` from metadata, for a dropped item `:entity/item`. It stays until the server removes it.
- A death is a fact the server states (entity-event status 3, `:entity/died-at`) before removing the body. A kill is the target gone or dead by the server's word.
- Extra senses, added for observability: an entity's hurt flash (a swing that landed vs missed), a mob's arm swing (a strike coming), the server's combat bracket, effects on the bot (wither, fire resistance), attributes (armour, max health), sounds and level events (a blaze's shot heard as level event 1018 when not seen), head yaw, equipment, chat and the bot's own death message.
- `derived.clj` diffs successive worlds into ledger events: what set the bot alight, items gained and lost, server-forced motion, biome, breath. It exists because most hits named no attacker: 14,156 of 19,018 over 450 runs, and 62% of all hits were burning.

## For ruststeve
These are `Observation` variants, each folded by `WorldModel::apply`. The derived facts are pure functions of (before, after) and belong in the core as derived events. Add the senses from the start; clojurecraft added them late, after deaths it could not explain.

## See also
- [[wire-entity-metadata]] - where health and flags come from.
- [[cc-blaze-hunt]] - the fight that needed them.
