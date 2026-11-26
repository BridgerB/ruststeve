# Issue 20: from the portal room to the dead dragon and the credits (one bot first)

**Status: started (2026-11-26).** The index for issue 20 (#25). Each part has its own wall file:
- [`end-observe.md`](end-observe.md): measures, stages, facts.
- [`end-portal.md`](end-portal.md): armed in the overworld, the frames, the entry.
- [`end-island.md`](end-island.md): the bridge, the island, gaze, the bunker.
- [`end-crystals.md`](end-crystals.md): pillar and strike.
- [`end-dragon.md`](end-dragon.md): the fight and the credits.
- [`end-model.md`](end-model.md): the sim and the parity rows.

## Needs
From the frontier `portal_room` snapshot, one real bot kills the dragon and receives the credits. The race snapshots it as stage `win`; the snapshot is published and becomes the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`.

Bridger's answers (2026-11-26):
- Crystals by pillar and strike, clojurecraft's design.
- The win is the dragon dead **and** the credits (`win_game` after the exit portal).
- Checkpoints `armed`, `end`, `island`, `crystals`, with the frontier moved at each.

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: win`, and the race's release carries its `snapshot-win-…` asset.

## The starting bot (race 45800967549 lane 4, `rs-l04-b00`)
- **Where:** on the portal room's floor at (1624.4, 17, -35.7). The ring centre is (1628, 19, -34).
- **Armour and weapons:** 13 eyes, iron helmet, chestplate and leggings (no boots), a shield, 2 stone swords.
- **Missing:** **no pickaxe, table or wood**; 3 sticks.
- **Food:** 30 raw meat (chicken 7, mutton 12, porkchop 11), no cooked food.
- **Other:** 7 raw iron, a furnace, 2 water buckets, a bucket, flint and steel.
- **Blocks:** 116 cobblestone, 64 cobbled deepslate, 150 netherrack, 20 soul soil, 14 gravel, 14 andesite.

## What follows before any code
- **The prep is in the overworld:** wood, pickaxes, an iron sword and boots from the 7 iron, and the meat cooked. This happens up the bot's own stairs, so `armed` is a checkpoint.
- **The End is fixed by the seed:**
  - the platform at (100, 48, 0);
  - ten spikes on a ring of 42 round the fountain at 0, 0, two caged.
  Their places are geography (Bridger's ruling); the crystals and the dragon are entities the server sends.
- **clojurecraft's numbers:**
  - a perched dragon's head blow takes 6.8 health on Normal;
  - three blows in 0.6 s killed a bot at 18.5.
  - So the bot is in the head's reach only above the perch line (20a measures it through our armour).
- **Every crystal goes first:** the dragon heals 2 a second within 32 of one.

## Walls
(Each wall in its agent's file; the index lists them here as they are found.)
