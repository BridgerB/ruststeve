---
title: Client physics
description: How clojurecraft moves its own body (vanilla land and liquid movement as a pure function of the player, controls and a solidity oracle), and the water facts every bot learned.
type: reference
tags: [oracle, physics, movement, water]
aliases: [physics.clj, physics/step, out-of-liquid impulse, no buoyancy, tick-ms, movement]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 steve@46c1264
sourceRefs:
  - "clojurecraft:src/clojurecraft/physics.clj#\"Vanilla land and liquid movement as a pure function over the world's :player/* attributes. Full-cube"
  - "clojurecraft:src/clojurecraft/physics.clj#siblings' ports of the client): no buoyancy to speak of, a swim-up of 0.04 a tick while jumping,"
  - "clojurecraft:src/clojurecraft/path.clj#Liquids are walls, never floors (the physics has no fluid model), lava is a wall and no cell"
  - "steve:CLAUDE.md#typecraft water physics"
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[body-boundary]]"
  - "[[mc-water]]"
---

# Client physics

The server does not move the player; the client simulates its own movement every tick and reports positions. clojurecraft does this as a pure function.

## Key files
- `src/clojurecraft/physics.clj`: `step`.
- `src/clojurecraft/path.clj`: what the route search assumes about the physics.

## How it works
- `(step solid? fluid-at world controls) → world'`. `solid?` maps a block cell to nil, a full cube, or a box height under 1 (a carpet). `fluid-at` gives a fluid kind and level. `controls` are forward, jump, sneak and yaw.
- Full-cube collision plus flat boxes the body stands on; a one-block ledge needs a jump, as in vanilla.
- In liquid, vanilla's liquid rules: no buoyancy to speak of, a swim-up of 0.04 a tick while jumping, and the one real lift: an impulse of 0.3 when pressing into a bank whose top is free.
- Each tick also writes `:player/in-water?`, `:player/in-lava?`, `:player/eye-in-water?` and `:player/fall-distance`.
- The route search treats liquids as walls and never enters a cell beside or over lava; blocks the physics cannot simulate (slabs, stairs, fences, doors) are awkward, routed around.

## For ruststeve
Physics is deterministic given the world and controls, so it belongs in the core where replay covers it ([[body-boundary]]). It is the most numerically sensitive port: differential-test it against clojurecraft step by step.

## Gotchas
- Water traps every bot: escape is by pressing into a bank or digging a notch, never by jumping ([[mc-water]]).

## See also
- [[mc-water]] - the water facts.
- [[cc-goals-and-intents]] - the escape rows.
