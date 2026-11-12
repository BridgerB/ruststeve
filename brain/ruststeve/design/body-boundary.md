---
title: The Body boundary
description: The seam between the loop and the game (a real server over our own wire layer, or the server model), why it is a trait, and where physics sits.
type: reference
tags: [shell, body, boundary, trait]
aliases: [Body trait, adapter, simulator body, sim body, real server]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#/// Shell-side boundary to the game. An open set (azalea, sidecar, simulator), so a trait is right here (3.7)."
  - "ruststeve:FUNCTIONAL.md#The core doesn't care which option you pick, because it only sees Input and Output."
  - "clojurecraft:src/clojurecraft/sim.clj#Two ways to drive it: run folds bot and model together, deterministic and fast (generated worlds); connect puts the model on a thread behind a channel pair shaped like conn/open!'s, so main's own loop (its clock, its effects, its tap) runs against it with no Java process."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[own-protocol-not-azalea]]"
  - "[[wire-boundary-parsing]]"
  - "[[cc-sim]]"
---

# The Body boundary

The core only sees inputs and outputs, so what it talks to is behind one seam with two implementations: a real server reached through our own wire layer ([[own-protocol-not-azalea]]), and the server model ([[cc-sim]]). An open set, so a trait.

## How it works
- The real body is the `net` crate's reader and writer threads plus the `protocol` codec ([[wire-boundary-parsing]]).
- The model body comes in clojurecraft's two forms: folded together with the bot in one thread (deterministic and fast, for the sim race and property tests), or behind the same channel pair the socket uses, so the real loop runs against it unchanged.
- Synchronous: `FUNCTIONAL.md` sketched an `async` trait; ruststeve's loop is threads and channels ([[sync-core-and-time]]).
- The Minecraft version is pinned in config and written into every recording header.

## Physics
The server expects the client to simulate its own movement. clojurecraft does it as a pure function inside its reducer, so it is recorded and replayed ([[cc-physics]]); ruststeve keeps physics in the core (`physics` crate), not in the body.

## See also
- [[tick-loop]] - the caller.
- [[cc-sim]] - the model body.
