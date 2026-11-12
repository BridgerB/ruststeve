---
title: Endermen and pearls
description: The enderman rules the pearl hunt relies on (neutral until stared at or hit, teleport rules, reach, the warped forest, one pearl in two kills).
type: reference
tags: [game, endermen, pearls, nether]
aliases: [enderman, ender pearl, warped forest, stare, provoke, burrow]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/pearls.clj#The game's loot table (data/minecraft/loot_table/entities/ enderman.json) gives one roll of ender_pearl, its count uniform 0 to 1 (plus 0 to 1 a level of looting): a kill drops a pearl one time in two, so twelve pearls are about twenty-four kills."
  - "clojurecraft:src/clojurecraft/pearls.clj#A warped forest spawns endermen and nothing else hostile (its biome file's monster spawner), so that is where the Nether leg hunts."
  - "clojurecraft:src/clojurecraft/pearls.clj#An enderman is neutral (entity/neutral): it attacks only once looked in the face or hit."
related:
  - "[[game/_moc|Game]]"
  - "[[cc-pearls-rule-table]]"
---

# Endermen and pearls

## How it works
- **Loot**: one roll of ender pearl, count uniform 0 to 1 (plus 0 to 1 per Looting level): a pearl one kill in two, so twelve pearls are about 24 kills. Bartering needs a stack of gold the run does not have.
- **Where**: a warped forest spawns endermen and nothing else hostile.
- **Neutral**: it attacks only once looked in the face or hit. Stared at, it turns on the player; an angry one stared at within 4 blocks teleports away; one farther than 16 teleports toward the player. Hit by a living entity it never teleports (only projectiles and non-living sources move it).
- **Numbers**: 40 health; 7 per hit on Normal (5.74 through iron), against which a sword needs seven hits: in the open the bot loses.
- **The burrow**: an enderman is 2.9 tall; a tunnel two high stops it at the mouth, two cells from the bot, where its reach (its box grown 0.83 sideways, never up or down) cannot touch the bot and the sword can touch it.
- Water hurts it and it teleports out of rain and water.

## See also
- [[mc-mob-facts]] - anger, teleports, armour arithmetic and frozen mobs, learned the hard way.
- [[cc-pearls-rule-table]] - the hunt built on these rules.
