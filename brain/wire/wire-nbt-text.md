---
title: NBT text, the death message and the damage event
description: How 26.x sends text components (network NBT, an anonymous root), how ruststeve reads a death message the way clojurecraft does (translate key plus arguments, no English table), and the player_combat_kill and damage_event packets.
type: reference
tags: [wire, nbt, text, death, damage]
aliases: [network NBT, text component, death message, player_combat_kill, damage_event, killer, damage type]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@1466451 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/protocol/src/nbt.rs#pub fn read_root(r: &mut Reader<'_>) -> Result<Nbt, ParseError> {"
  - "ruststeve:crates/protocol/src/nbt.rs#pub fn death(c: &Nbt) -> Death {"
  - "ruststeve:crates/protocol/src/packets.rs#Some(PlayIn::DamageEvent) => {"
  - "clojurecraft:src/clojurecraft/text.clj#\"A death message component as {:death/key the translate key :death/victim the first argument's"
  - "clojurecraft:src/clojurecraft/packet.clj#[:play :s2c :player-combat-kill] [[:player-id :varint] [:message :text]]"
  - "clojurecraft:src/clojurecraft/registry.clj#types (the order the server syncs them: a damage-event's source type is the id), attributes"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-boundary-parsing]]"
  - "[[race-and-results]]"
---

# NBT text, the death message and the damage event

Since 1.20.3 a text component on the wire is network NBT: an anonymous root (a tag type byte, then its payload, no root name). The server's account of a death arrives this way, and it is the first thing a case file names ([[cc-observability]]).

## How it works
- `protocol::nbt::read_root` reads any tag into a plain `Nbt` value. Bounded: nesting past 64 levels and array counts the frame cannot hold are boundary errors, never a stack overflow or a huge allocation; a property test feeds it random bytes.
- **Reading a component** (`plain`, clojurecraft's `text/plain`): a string as it is; a compound as its `text`, or for a `translate` key the key then each `with` argument's text; then `extra`. No English table: `death.attack.mob rs-001 entity.minecraft.zombie` says what happened, to whom and by what. A compound whose only key is `""` is a list element NBT had to wrap.
- **`player_combat_kill`**: the player's entity id and the death message, read into `Death { text, key, victim, by }` (clojurecraft's `text/death`; `by` loses the `entity.minecraft.` prefix). The world keeps it only for the bot's own id.
- **`damage_event`**: entity id, the damage type id, the cause and direct entity ids sent **plus one** (0 = none), then an optional position (an unread tail). The type id indexes the damage-type registry in the order the server syncs it; ruststeve uses the generated `DAMAGE_TYPE` table, as clojurecraft used its generated `damage-types.edn`.

## Gotchas
- An id outside the table reads `unknown:<id>`, never a guess.
- The killer the race groups by drops the victim's name (`death.attack.mob zombie`), so one cause groups across bots; the case keeps the whole text.

## See also
- [[race-and-results]] - where the killer and hits go.
- [[entities-and-inventory]] - the world's other folds.
