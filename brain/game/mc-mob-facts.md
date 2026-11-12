---
title: Mob facts learned the hard way
description: Vanilla 26.1.2 mob and server facts that each cost clojurecraft a day or more (simulation distance freezes, enderman anger and teleports, knockback, magma cube hops, fireball damage events, armour arithmetic, slow server ticks).
type: reference
tags: [game, mobs, simulation-distance, endermen, combat]
aliases: [simulation distance, frozen mobs, knockback, magma cube, fireball damage type, armour damage, server tick rate]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#An enderman with a target keeps its anger forever: the timer restarts every tick, and it doesn't need to see the target."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A teleport lands only on a floor with three free cells, so a bot in a 2-high rock tunnel cannot be reached."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A melee hit knocks a mob back about 3 blocks over 11 ticks. The bot ignores its own knockback packets, so the real bot is never knocked back."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A fireball's damage event carries no entity (the projectile is gone), so \"was I shot\" must read the damage type, not the attacker."
  - "clojurecraft:brain/bot/tooling/race.md#A server ticks entities only within its simulation distance of a player (3 chunks on the race): one farther out is still sent but frozen."
related:
  - "[[game/_moc|Game]]"
  - "[[mc-endermen]]"
  - "[[cc-stage18-lessons]]"
---

# Mob facts learned the hard way

Each of these came from the server jar or real logs and cost clojurecraft at least a day.

## Server
- **Simulation distance freezes mobs.** A server ticks entities only within its simulation distance of a player; one farther out is still sent but frozen: no movement, attack or teleport. The race servers ran 3 chunks (about 48 blocks), which beat 4 in an A/B. Real bots watched frozen endermen for hours; two frozen ghasts 61 blocks off held one bot in a heal-under-fire loop for two hours. The bot reads the distance from the login packet and leaves frozen mobs out of its threats; the model must freeze them too.
- **Servers tick slowly under load**: 150–450 ms a tick with 10–14 bots instead of 50. Anything counted in wall time (caps, stares, digs) must be counted in server ticks ([[mc-dig-timing]]).

## Endermen
- An enderman with a target keeps its anger forever (the timer restarts every tick) and does not need to see the target.
- More than 16 blocks from its target it teleports toward it after 30 ticks; stared at within 4 it teleports away. A teleport lands only on a floor with three free cells, so a bot in a 2-high rock tunnel cannot be reached.
- Through iron armour one hit takes about 4 health, so any "I was hit" threshold below 16 never fires after a single hit.
- A stare turns an enderman only after several of its goal passes, which take server ticks: 1.7 to 5.1 s on a loaded server ([[mc-endermen]]).

## Combat
- A melee hit knocks a mob back about 3 blocks over 11 ticks. The bot ignores knockback packets for itself, so the real bot is never knocked back.
- Magma cubes come in sizes 1, 2 and 4 and hop; the hop stops at a ceiling and the strike needs line of sight.
- A fireball's damage event carries no entity (the projectile is gone): "was I shot" reads the damage type, not the attacker.
- 26.x gamerules are snake_case (`keep_inventory`). Protocol 775 split `attack` into its own packet ([[wire-actions]]).

## See also
- [[mc-endermen]] - the pearl hunt's rules.
- [[cc-stage18-lessons]] - the sim-real gaps these caused.
