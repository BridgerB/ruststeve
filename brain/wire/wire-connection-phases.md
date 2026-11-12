---
title: Connection phases
description: The protocol states from handshake to play, which client packet advances each, and what the configuration and play-login packets carry.
type: reference
tags: [wire, states, login, configuration]
aliases: [handshake, login, configuration, play, protocol state, intention, finish-configuration, start-configuration]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#\"Protocol state after the client sends a packet: {[state name] next-state}.\""
  - "clojurecraft:src/clojurecraft/packet.clj#[:configuration :c2s :finish-configuration] []"
  - "clojurecraft:src/clojurecraft/packet.clj#CommonPlayerSpawnInfo as respawn has it, so a bot that joins in the Nether knows it; the death"
  - "ruststeve-v1:src/protocol/states.rs#pub enum ProtocolState {"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-framing]]"
  - "[[wire-offline-login]]"
  - "[[wire-packet-ids]]"
---

# Connection phases

The wire is always in one protocol state, and the same packet id means different packets in different states. The client advances the state by *sending* a transition packet.

## Key files
- clojurecraft `src/clojurecraft/packet.clj`: `transitions`, `next-state`, the handshake, login and configuration specs.
- ruststeve v1 `src/protocol/states.rs`: `Handshaking, Status, Login, Configuration, Play`.

## How it works

| From | Client sends | To |
|---|---|---|
| handshake | `intention` (protocol version, host, port, next-state: 1 status, 2 login, 3 transfer) | login |
| login | `login-acknowledged` (after the server's `login-finished`) | configuration |
| configuration | `finish-configuration` (after the server's) | play |
| play | `configuration-acknowledged` (after the server's `start-configuration`) | configuration |

- **Login**: client `hello` (username, uuid) → server may send `login-compression` (threshold) → server `login-finished` (uuid, username) → client `login-acknowledged`.
- **Configuration**: server sends registries and tags; the bot answers `select-known-packs`, echoes `keep-alive` (i64) and `ping`→`pong` (i32), and answers `finish-configuration` with its own.
- **Play login** (`login`): entity id, hardcore, the dimension list, max players, view and simulation distance, then `CommonPlayerSpawnInfo` (dimension type, dimension name, seed, game type ...), so a bot joining in the Nether knows it. `respawn` carries the same spawn info.
- Keep-alive in play: echo the server's i64 id, or be kicked.

## Gotchas
- The server can send the client back to configuration from play (`start-configuration`). A bot that ignores it decodes every later frame against the wrong table.
- The play `login` and `respawn` frames have tails (death location and more) that clojurecraft does not model; its decoder simply ignores a frame's unread tail ([[wire-packets-as-data]]).

## Limits
Which configuration packets a 26.1.2 server sends, and in what order, is not recorded here; clojurecraft answers only what its specs list.

## See also
- [[wire-offline-login]] - the identity the hello carries.
- [[wire-packet-ids]] - per-state id tables.
