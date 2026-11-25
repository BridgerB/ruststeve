# Issue 18: from a ticking enderman to 12 ender pearls (one bot first)

**Status: open.** The index for issue 18 (#23). Each part has its own wall file: [`pearls-observe.md`](pearls-observe.md) (measures, stages, facts), [`pearls-travel.md`](pearls-travel.md) (to the warped forest), [`pearls-hunt.md`](pearls-hunt.md) (the forest base, the burrow, provoke, fight, collect), [`pearls-model.md`](pearls-model.md) (the sim and the parity rows).

## Needs
From the frontier `endermen` snapshot, one real bot holds 12 ender pearls and its 7 blaze rods; the race snapshots it as stage `pearls_12`, the snapshot is published and becomes the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`. Checkpoints on the way (Bridger, 2026-11-24): `forest` (7 rods, the feet in a warped forest) and `pearls_6`; the frontier moves at each.

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: pearls_12`, and the race's release carries its `snapshot-pearls_12-…` asset.

## The starting bot (race-45481633896 lane 3, the frontier)
Sealed in a 12-block pocket at about (8, 85, 81) in a soul sand valley, about 150 blocks west of the blaze spawner: health 12.5, food 17, **no food held** (cannot heal: natural healing needs food 18), **no shield**, iron sword 161 uses left, 2 stone pickaxes 144 uses, about 588 filler, 7 rods, iron helmet, chestplate and leggings, furnace, raw iron 7, **coal 1**, sticks 3, no table, no wood. Piglins (hostile: no gold worn), zombified piglins, a skeleton and a ghast near.

The other real `endermen` snapshot, lane 2 of the same race, stands at the blaze spawner with a blaze charging: health 16.3, food 20, a raw porkchop and a shield, no coal.

## What follows before any code
- **No healing**: every hit is kept. An enderman hit is about 4 through iron; 12.5 is three hits. The hunt is built so the bot is never hit.
- **The sword cannot do it**: a kill drops a pearl one time in two (loot 0 to 1), so 12 pearls are about 24 kills (about 31 for nine chances in ten); an iron sword's 6 against 40 health is 7 hits, about 170 to 215 hits, against 161 uses. More weapons need wood (a table, sticks) and the 126 cobblestone, or the 7 raw iron smelted with the one coal.
- **Where endermen are**: the warped forest spawns endermen and no other monster (groups of 4); a soul sand valley 1 pack in 71 (race 45487204794: 4 bots waited 12,000 ticks each and none had an enderman within 32). The warped forest is the target and the base: its stems are wood. This world's nearest from the spawner is about 454 blocks east-north-east behind about 300 blocks of basalt deltas (17a, development knowledge only).
- **Not used**: the overworld (the portal's place is not in the snapshot, a Nether portal cannot be cast, caves 1.75% of seconds with an enderman within 32) and bartering (about 90 gold ingots).

## Risks
- The trip at no-heal health: about 510 blocks, ghasts and magma cubes in the deltas, piglins.
- The stare-freeze (clojurecraft, 5d894db): an angry enderman stared at within 16 neither walks nor attacks; held stares parked them for minutes. Stare only until the creepy flag, then the body.
- Parked endermen that never come to the mouth; the bounce between the burrow's end and its mouth (clojurecraft: 119 to 1,241 a run).
- Endermen taking a burrow block (`#enderman_holdable`).
- The heading has never raced; basalt deltas guard this world's forest.
- One world: every frontier race is the same seed, forest and route.

## The burrow (to be proven on a real server first: parity rows e04 to e08)
A level burrow one wide and two high on the forest floor, built from held filler: the bot's cell B, the mouth cell M, the outside cell O where the enderman stands with three or more clear above it. The enderman (2.9 tall) cannot enter M and its reach (its box grown 0.83 sideways, 1.43 centre to centre) stops 0.37 short of B; the bot's 3.0 reaches its box. From B the roof cuts the line from the bot's eye to the enderman's at O, so no stare can freeze it there. Provoke from M at a calm ticking enderman 6 to 36 off until the creepy flag, then B; strike the torso on a full charge; collect from M only with no angry enderman present.

## Index of walls
(none yet)
