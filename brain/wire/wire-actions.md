---
title: Acting on the world
description: The client packets that dig, place, use items and attack in 775, their fields and enum values, and the attack-packet trap that dealt zero damage.
type: reference
tags: [wire, actions, attack, dig, use-item]
aliases: [attack packet, interact, player-action, use-item, use-item-on, swing, 0x01, melee does nothing]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f ruststeve@3042214
sourceRefs:
  - "ruststeve:crates/world/src/lib.rs#pub enum Act {"
  - "ruststeve:crates/protocol/tests/encode.rs#fn a_dig_carries_its_sequence() {"
  - "clojurecraft:src/clojurecraft/packet.clj#;; an attack on an entity, its id alone: 775 split attacking out of interact, which is a"
  - "clojurecraft:src/clojurecraft/packet.clj#[:play :c2s :player-action] [[:status :varint] [:pos :position] [:face :i8] [:sequence :varint]]"
  - "clojurecraft:src/clojurecraft/packet.clj#;; right-click with the held item, aimed by look: a bucket fills or pours here, not through"
  - "ruststeve-v1:src/bot/mod.rs#// Sync our current position+look to the server BEFORE attacking. send_position otherwise"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-packets-as-data]]"
  - "[[mc-blaze]]"
---

# Acting on the world

Everything the bot does to the world is one of a few client packets. Several carry a **sequence** number the server acknowledges with `block-changed-ack`.

## Key files
- clojurecraft `src/clojurecraft/packet.clj`: the play c2s specs and `values`.
- ruststeve v1 `src/bot/mod.rs`: `attack`.
- ruststeve `crates/world/src/lib.rs`: `Act` (`Dig { step, pos, face }`, `Hold`, `Click`, `CloseWindow`, `Swing`) is an input event; the world turns it into the packet, counting the dig sequence (`sequence_sent`) and folding `block_changed_ack` into `sequence_acked` (the highest seen). Live on a local server (2026-11-12): START then FINISH 3 s later on a grass block drew the block update to air, `block_changed_ack 2` and the pickup into hotbar slot 36.

## How it works

| Packet | Fields | Use |
|---|---|---|
| `player-action` | status (varint), position, face (i8), sequence | dig: status 0 start, 1 abort, 2 finish; 5 release the use key (lower a shield) |
| `use-item-on` | hand, position, face, cursor x/y/z (f32), inside-block, world-border-hit, sequence | right-click a block face: place the held block or open a container |
| `use-item` | hand, sequence, yaw, pitch | right-click aimed by look: a bucket fills or pours here, not through `use-item-on` |
| `attack` | entity id (varint) | melee |
| `swing` | hand | arm animation |
| `set-carried-item` | slot (i16) | select a hotbar slot |
| `client-command` | action (0 respawn) | respawn after death |

Faces: down 0, up 1, north 2, south 3, west 4, east 5. Hands: main 0, off 1.

## Gotchas
- **In 775 melee is the dedicated `attack` packet, entity id only.** `interact` became right-click only and requires a trailing hand field. ruststeve v1's "melee deals zero damage" wall was an interact with mouse 1: it decoded and did nothing; without the hand field it got the bot kicked.
- v1 also had to send its position and look *before* the attack, or the server judged reach and angle against a stale pose, and to pace swings by wall clock ([[hist-v1-load-bearing-fixes]]).
- A dig FINISH sent too early aborts the break; the timing rule is in [[mc-dig-timing]].

## See also
- [[mc-dig-timing]] - when to send FINISH.
- [[wire-container-click]] - acting inside windows.
