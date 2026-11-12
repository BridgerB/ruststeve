---
title: Offline-mode identity
description: What UUID an offline-mode server assigns a bot, how the login hello carries it, and why the bots never authenticate.
type: reference
tags: [wire, login, uuid, offline]
aliases: [offline uuid, OfflinePlayer, hello, online-mode=false, auth]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:src/clojurecraft/bytes.clj#\"The UUID an online-mode=false server assigns: MD5 name-UUID (v3) of OfflinePlayer:<name>.\""
  - "clojurecraft:src/clojurecraft/packet.clj#[:login :c2s :hello] [[:username :string] [:uuid :uuid]]"
  - "ruststeve-v1:CLAUDE.md#Targets MC 26.1.2, protocol 775, offline mode only (no auth; bots must be op'd for RCON/MC_TP)."
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-connection-phases]]"
  - "[[wire-framing]]"
---

# Offline-mode identity

Every bot connects to servers with `online-mode=false`: no Mojang account, no session server, no encryption. The server derives the player's UUID from the name alone.

## Key files
- clojurecraft `src/clojurecraft/bytes.clj`: `offline-uuid`.
- clojurecraft `src/clojurecraft/packet.clj`: the login `hello` spec.

## How it works
- UUID = the MD5 name-based UUID (version 3) of the bytes of `"OfflinePlayer:" + name` (Java's `UUID.nameUUIDFromBytes`).
- The client's `hello` sends `username` (string) and that `uuid`.
- With no encryption request in offline mode, the stream stays plaintext; only compression switches on ([[wire-framing]]).
- A bot that runs RCON-backed setup (teleports, gives, gamerules in gyms) must be op'd on the server by name.

## Gotchas
- The UUID is a pure function of the name, so a renamed bot is a different player with a fresh inventory and spawn point.

## Limits
Online-mode login (Microsoft auth, AES-CFB8) is implemented in typecraft and v1 but never used by these bots; it is out of scope.

## See also
- [[wire-connection-phases]] - where the hello sits in the login.
