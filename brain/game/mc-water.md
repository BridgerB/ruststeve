---
title: Water and lava
description: How a player moves in water (no buoyancy, the bank impulse is the only lift), how bots drown and escape, and the lava rules every bot learned.
type: reference
tags: [game, water, lava, physics, survival]
aliases: [drowning, swim out, out-of-liquid impulse, buoyancy, lava safety, water traps]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 steve@46c1264
sourceRefs:
  - "clojurecraft:src/clojurecraft/physics.clj#the one real lift, an impulse of 0.3 when pressing into a bank whose top is free, asked while"
  - "clojurecraft:src/clojurecraft/swim.clj#It holds forward and jump toward it; the physics' out-of-liquid impulse fires when the body"
  - "clojurecraft:src/clojurecraft/stairs.clj#and never digs the block under the feet (a blind drop is how the siblings' bots died)."
  - "steve:CLAUDE.md#typecraft water physics"
related:
  - "[[game/_moc|Game]]"
  - "[[cc-physics]]"
  - "[[physics]]"
  - "[[hist-walls]]"
---

# Water and lava

## How it works
- **No buoyancy** to speak of; jumping swims up only 0.04 a tick. The one real lift out of water is an impulse of 0.3 when the body presses into a bank whose top is free.
- **Escape**: pick one bank and keep it (re-picking every tick swims in circles), hold forward and jump toward it; when stuck, dig what is in the way (a lid overhead first, a wall at head height), never the block at feet height ahead (it is the bank the impulse needs). With no bank at all, carve a notch two high in the wall just over the float level.
- **Air** is 300 ticks (15 s); lost one a tick with the eye under water, regained four a tick otherwise.
- **Lava**: never enter a cell beside or over lava; never dig the block under the feet (blind drops into lava killed the siblings); a descent refuses any stair that exposes water, lava or an unloaded chunk.
- Nether lava flows a block every half second; water evaporates in the Nether.

## Gotchas
- Cave water is the aquifer trap the siblings drowned in; clojurecraft only scoops surface water.

## See also
- [[cc-physics]] - the pure physics step.
- [[physics]] - ruststeve's port of it.
- [[hist-walls]] - water as the second wall.
