# Issue 18: from a ticking enderman to 12 ender pearls (one bot first)

**Status: the stage is passed (2026-11-25, 6:30 AM): a real bot holds 12 pearls.** The index for issue 18 (#23). Each part has its own wall file: [`pearls-observe.md`](pearls-observe.md) (measures, stages, facts), [`pearls-travel.md`](pearls-travel.md) (to the warped forest), [`pearls-hunt.md`](pearls-hunt.md) (the forest base, the burrow, provoke, fight, collect), [`pearls-model.md`](pearls-model.md) (the sim and the parity rows).

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

## Measured (18a)
- The bots (probe 45529738504): the frontier 12.47 health, food 17, saturation 0, sword 161 uses, pickaxes 13 + 131, coal 1, no shield; lane 2 16.3, food 20, sword 176, shield 48, a porkchop (about 4 health of healing in all). `pearls-observe.md`.
- The way (development knowledge only): the warped forest at (488, -111), 516 blocks east-north-east of the frontier; no compass axis crosses one within 640; east is basalt deltas from 160 to 640; the forest lies north of that axis about x 470 to 540, so the way is east then north through about 300 blocks of deltas.
- The jar (probe 45529798802): melee is the box grown 0.828 sideways and needs a line of sight; an enderman needs 1 x 3 x 1 cells and walks to the node nearest an unreachable target; a player's hit never teleports it; holdable blocks are dirt, gravel, nylium, fungi and roots; no usable mushrooms in the warped forest (on the way only).
- **The burrow on a real server** (parity 45531046147, P4, `pearls-model.md`): e04 the stare from M angered it in about 10 ticks, it walked to O in about 30 and stood there 400 ticks, the bot never hit; e05 seven iron hits from B, no teleport, a kill in about 150 ticks (each hit knocks it 1 to 1.7 back); e06 the roof cuts the stare from B; e07 a pearl at O is picked up from M, not from B; e08 even in a roofed M the bot is not hit (the melee needs a line of sight eye to eye). The bot drifted 0.14 toward M when its forward key was let go: the hunt holds it at B's centre or back.

## The checkpoints reached
- **forest** (2026-11-24, 8:15 PM): rs-l00-b00 of race 45537545167 (commit 1c17850), resumed from the real `endermen` snapshot of race 45481633896 lane 2: out of its pocket, east by the travel points through about 300 blocks of basalt deltas, feet in a warped forest at tick 4,020 at (476.3, 41.0, 152.5), health 18.3, 7 rods, a shield and a porkchop. `config/frontier.toml` moved (6f33cc4). The travel's walls are in `pearls-travel.md`.
- The forest base on a real server (race 45536739217, dev-forest, development only): met 2 of 2 in 2,620 to 3,080 ticks, no hits (`pearls-base.md`).

- **pearls_6** (race 45588764709 lane 0, resumed from the real stuck 5-pearl snapshot of race 45578610032 lane 6), **pearls_9** (race 45615781100 lane 6), **pearls_12** (race 45621953332 lane 1, tick 11,360, from the pearls_9 frontier). Every link real; the frontier moved at each (24c47dc, 129f879 sideways to the healthier pearls_6 bot, a414553, ca7e981).
- What carried it: the burrow proven on a real server (a solid 4 x 5 shell, the bot two cells in), the hunt point in the main forest (Bridger's ruling: 8 kills by tick 4,800 against 1 to 3 a race at the forest's edge), no fight with magma cubes or piglins (a sealed pocket instead), reclaimed and restocked filler, and each lane its own enderman loot rolls (race.yml: every lane restored from one world had rolled the same pearl at its nth kill).

## Index of walls
- **1. The burrow is a tie** (parity 45535279839, P4, `pearls-model.md`): on the second real run the enderman held O about 200 ticks, then walked round to the cell beside B and behind its back wall, out of strike reach; for a mob 2.9 tall O, the two cells beside B and the one behind it are all 2 from B, and the partial path takes any. The bot in M was hit once (7); in B never (0 hits in 4 rows). Fix under test: a solid shell 5 wide and 3 to 4 long round the tunnel, so O is the one nearest standing cell (every other at the square root of 5 or more), the roof's top unreachable; fight and wait only from B; M only to collect with none angry.
