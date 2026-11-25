# Issue 19: from 12 pearls in the Nether to standing in the stronghold's portal room (one bot first)

**Status: started (2026-11-25).** The index for issue 19 (#24). Each part has its own wall file: [`eyes-observe.md`](eyes-observe.md) (measures, stages, facts), [`eyes-home.md`](eyes-home.md) (the Nether forest to the overworld surface), [`eyes-throw.md`](eyes-throw.md) (craft, throw, read, triangulate), [`eyes-stronghold.md`](eyes-stronghold.md) (overland, the dig, the portal room), [`eyes-model.md`](eyes-model.md) (the sim and the parity rows).

## Needs
From the frontier `pearls_12` snapshot, one real bot stands in the stronghold's portal room; the race snapshots it as stage `portal_room`, the snapshot is published and becomes the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`. Checkpoints on the way (Bridger, 2026-11-25): `overworld` (back through its own portal with the eyes) and `stronghold` (an end portal frame seen within 128); the frontier moves at each. Before leaving the forest the bot tops its pearls up to 14 (Bridger, 2026-11-25), bounded at 6,000 ticks: 7 rods make 14 eyes, and every throw spends issue 20's margin.

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: portal_room`, and the race's release carries its `snapshot-portal_room-…` asset.

## The starting bots (race 45621953332, all four in the main warped forest)
| lane | health | food | sword uses | free slots |
|---|---|---|---|---|
| 1 (the frontier) | 10.0 | 10 | 37 | 5 |
| 5 | 19.8 | 10 | 44 | 6 |
| 6 | 19.8 | 10 | 24 | 6 |
| 7 | 19.8 | 11 | 47 | 5 |

All hold 7 rods, 12 pearls, a shield, iron armour, stone and wooden tools, and no food: none can heal (natural healing needs food 18). A sideways frontier move to a 19.8 bot is chosen from their `snap.toml` items (19a).

## What follows before any code
- **Eyes do not fly in the Nether**: the bot goes back to the overworld.
- **No portal can be made in the Nether** (water evaporates; obsidian needs a diamond pickaxe): the way home is its own entry portal, obsidian x 21 to 24, y 34 to 37, z -20 (`nether-walk.md`), on a ledge over the lava sea with lava in front of it (`nether-tunnel.md`), about 475 blocks west through the basalt deltas.
- **The overworld side is deep** (cast near the lava sea, about y -50; `overworld-leg.md`): the cave band above it killed 7 of 16 bots in issue 16.
- **Strongholds of the first ring lie 1,280 to 2,816 blocks from the origin**: a long overland trip, by day and night, with no food until the bot hunts.
- **The portal room is found by its frames**: `end_portal_frame` blocks face the room's air, so memory sees them from chunk data within the view distance; the eyes only need to bring the bot within about 100 blocks.

## Measured (19a)
- **The eye** (Q5, probe 45685968956, the 26.1.2 jar): it spawns at the feet +0.9 and flies through terrain toward a point 12 blocks along the exact bearing to the stronghold and 8 up (within 12, to the stronghold itself, descending). It ends after 81 ticks: 4 in 5 as a dropped `ender_eye` item, 1 in 5 shattered. In the Nether, or with the look on an end portal frame, there is no throw and no eye is spent. The heading is the bearing with no curve: a line's error is only the position's rounding.
- **The stronghold** (development knowledge, RCON `locate`): (1648, ~, -16), 1,537 blocks almost due east of the overworld portal near (113, -105) and nearest from the bot's old stairs at (55, -133).
- **The portal room** (`StrongholdPieces$PortalRoom`, 11 x 8 x 16): frames at the floor +3 round a lava pool at floor +1 under the ring; side lava pools by the door end; the silverfish spawner at the top of the stair, facing the ring.
- Dev starts `dev-overworld-eyes` and `dev-stronghold` (1355826): the frontier bot on the surface with 14 eyes and food, development only.

## Risks
- The lava ledge at the portal; the cave band at the overworld exit.
- No healing until food is eaten.
- Night and creepers on a long walk; oceans on the bearing.
- Eyes lost to throws (1 in 5 breaks).
- One world: one seed, one stronghold, one route.

## Index of walls
(none yet)
