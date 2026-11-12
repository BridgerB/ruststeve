---
title: Wire pillar
type: moc
tags: [pillar, hub, wire, protocol]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f steve@46c1264
related:
  - "[[_index]]"
---

# Wire pillar

Minecraft 26.1.2, protocol 775, as ruststeve must read and write it. Facts come from the three protocol layers already written (clojurecraft, typecraft in steve, ruststeve v1); each note cites the one it was read from.

## Bytes and frames
- [[wire-primitives]] - varints, packed positions, strings, UUIDs, the LpVec3 velocity.
- [[wire-framing]] - frames, the compression threshold, state flips on send.
- [[wire-connection-phases]] - handshake → login → configuration → play, and back.
- [[wire-offline-login]] - the offline UUID and the hello.
- [[wire-packet-ids]] - ids generated from the server's reports; the version pin.

## Packets
- [[wire-packets-as-data]] - clojurecraft's spec table, tails ignored, unknowns counted; the typed Rust form.
- [[wire-slots]] - item stacks and data components; why one unknown component truncates a frame.
- [[wire-entity-metadata]] - serializer ids, the load-bearing order, skimming.
- [[wire-chunk-sections]] - paletted containers and the 26.1 changes.
- [[wire-container-click]] - state ids, the empty prediction, the one-click-behind trap.
- [[wire-actions]] - dig, place, use, and the 775 attack packet.
- [[wire-nbt-text]] - text components as network NBT; the death message and the damage event.

## ruststeve's layer
- [[wire-boundary-parsing]] - packets become `Observation`s; boundary errors are values and never vanish.
- [[wire-prior-implementations]] - typecraft vs v1 vs clojurecraft, and what ruststeve keeps.

## See also
- [[own-protocol-not-azalea]] - why this pillar exists.
- [[game/_moc|Game]] - the game rules behind the packets.
