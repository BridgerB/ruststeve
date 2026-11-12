---
title: Blazes
description: The blaze facts a fight depends on (health, the volley cycle, fireballs and fire, drops, the spawner), from the guide and from clojurecraft's measurements.
type: reference
tags: [game, blaze, nether, combat]
aliases: [blaze, small fireball, volley, blaze rod, spawner, hurt invulnerability]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Its ranged attack cycle: it burns for about 3 seconds, fires three small fireballs over about 0.9 seconds, then waits about 5 seconds before attacking again."
  - "ruststeve:FUNCTIONAL.md#Shields block its fireballs. Snowballs do 3 damage to blazes. Kills drop 0 or 1 rod without Looting."
  - "clojurecraft:src/clojurecraft/dodge.clj#The blaze's charge flag (metadata index 16) is set 1.4 to 3.5 s before its fireballs spawn, with no direction in it."
  - "clojurecraft:src/clojurecraft/mob.clj#A fortress's blaze spawner (the Nether's only spawning) runs by the rules at the end: awake within 16 of a player, a batch every 200 to 800 ticks, under a cap of six blazes about it."
related:
  - "[[game/_moc|Game]]"
  - "[[cc-blaze-hunt]]"
  - "[[blaze-debugging-method]]"
---

# Blazes

## How it works
- 20 health. Attacks from up to 48 blocks; touching its target it hits once a second (6 on Normal).
- **The volley cycle**: it burns for about 3 s (the wind-up), fires three small fireballs over about 0.9 s, then waits about 5 s. Broken line of sight mid-volley suspends the rest until sight returns. The safe window to close and strike is the rest after a volley.
- **The charge flag**: clojurecraft reads it as the blaze's own metadata index 16, set 1.4 to 3.5 s before the fireballs spawn, with no direction in it. The guide calls it "the on-fire metadata flag"; it is not the shared flags byte at index 0 ([[wire-entity-metadata]]).
- **Fireballs** fly straight; a hit sets the target on fire for five seconds, and burning was 45% of the health clojurecraft's blaze fights cost ([[cc-blaze-hunt]]). No water works in the Nether. Shields block fireballs.
- **Drops**: 0 or 1 rod per kill without Looting (one in two on average). Six rods make the twelve powder for twelve eyes.
- **Spawner**: the Nether's only blaze source; awake with a player within 16 blocks, a batch every 200 to 800 ticks, at most six blazes about it.
- Melee: a mob is invulnerable for 10 ticks after a hit, so swings must be paced (clojurecraft: 650 ms by the clock).

## Gotchas
- Blazes hover; a target above melee reach cannot be hit from the floor.

## See also
- [[cc-blaze-hunt]] - what clojurecraft does with these facts.
- [[blaze-debugging-method]] - the hypotheses they feed.
