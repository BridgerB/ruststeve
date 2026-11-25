# Issue 18d: the forest base (agent P5)

The orchestrator's index is `docs/problems/pearls.md`; this file is P5's: the kit made in the warped forest before the hunt (`crates/plan/src/base.rs`, the `base-*` rows and `[base]` in `config/goals.toml`, brain note `forest-base`).

## Needs
On reaching a warped forest, with no angry enderman present and before the first provoke, the bot holds swords worth 30 kills (iron 7 hits, stone 8; a sword under 10 uses counts for nothing), pickaxe uses at or over the reserve 40 plus 60, and a shield for issue 19, keeping its 7 rods and 4 free main slots. It arrives (the frontier, race-45481633896 lane 3) with no table and no wood, an iron sword of 161 uses (23 kills), stone pickaxes of 13 and 131, a furnace, raw iron 7, coal 1, sticks 3 and 126 cobblestone; lane 2 has no coal and a shield.

## Done (world terms)
A bot's ledger in the forest shows "base: <step> done / skipped" for each of the seven steps and "base over: the kit is held", its pack holds the kit, its rods 7, and no enderman was stared at or angered while it worked.

## Risks
- Calm endermen all round: every look goes through `look::keep_off` (P3).
- A sword in hand while digging stems spends 2 uses a block: the hand is emptied first.
- Fuel: warped planks do not burn; with no coal the planner's fuel need would reach for sticks or count warped planks: the smelt runs with coal only, and any other smelt act gives its step up.
- Stems over reach: only stems within 2 of the feet's height are walked to.

## Walls

### 1. The first real base race: the dev-forest start stands in a fortress (race 45535350776, 53f5f8b, goal `base`, dev-forest on the frontier lane-3 snapshot, 2 x 1, cap 6,000; development only)
- **What happened**: 0 of 2 met the base. Both died to blaze fire on nether bricks, at ticks 760 and 240: lane 0 at (491, 70, -121), lane 1 at (493, 68, -107). They started at health 12.5 with no shield. Each ledger has "base: wood begun" and one "drop Granite", the 4-free-slots rule firing. Lane 0 also has "base waits: wood: no stem seen within 24 with a way to it" twice. No enderman was angered or struck, and there was no near miss. Every hit came from a blaze: lane 0 took 2 fireballs and 7 burns; lane 1 took a touch, a fireball and burns.
- **Read**: the start's place, (488.5, 70, -110.5), is the brick deck of a fortress with blazes and wither skeletons. Lane 1's "warped forest known" notes name chunk columns 29 to 33 at chunk z -1 to 2, so the forest's floor starts about 100 blocks south (z -16 or more). No stem was within 24, so the base had nothing to do. `Aim::Base` also runs no cover: `take_cover` is part of the race's `nether_task`. A bot at 12.5 among blazes had only the shield to answer with, and it holds none.
- **Cause**: the development start, not the base. The base has to start on the forest's own floor.
- **Next**: the orchestrator moves `dev-forest` onto warped nylium inside the "warped forest known" columns (about x 470 to 540, z 0 to 40), away from the fortress, and the race runs again. Until then the base's steps are proven only in the model (`crates/bot/tests/base.rs`), not on a real server.

## Result: the base proven on a real server (race 45536739217, 054aac6, goal `base`, dev-forest at (490.5, 45, 5.5) on the frontier lane-3 snapshot, 2 x 1, cap 6,000; development only, never evidence)
- **Base met 2 of 2**, both alive to the end at health 12.5 (the start's), no hits, no swings, no enderman angered, no near miss. Each ends "base over: the kit is held (swords worth 58 kills of 30, pickaxe uses 144 of 100, 1 shield; given up: none)", and the shield goes to the off hand.
- **Steps and ticks** (lane 0 / lane 1):
  - wood: 1,600 / 1,140 ticks, 8 stems by hand, 10 / 9 walks, 2 / 1 digs `Uncollected`.
  - table: 40 / 40. Planks, sticks (3 to 7), a table placed.
  - smelt: 1,400 / 1,400. 7 raw iron with the one coal in the held furnace, placed where the bot stood; no stick or plank burned.
  - iron sword: 20 / 20. 23 kills to 58.
  - stone swords and pickaxe: skipped (58 kills, 144 uses).
  - shield: 20 / 20.
  - In all 3,080 / 2,620 ticks from arrival, every step done within its 2,400.
- **Inventory**: the 4-free-slots rule fired. Granite, tuff and leather were dropped; leather was the existing junk rule's. "junk picked up again" fired once each: a dropped stack lay at the feet, granite in lane 0 and tuff in lane 1. That is the existing equip junk drop's known pick-up, counted by its note.
- **Two things for the hunt**: the smelt is the longest step, 1,400 ticks standing at the furnace with endermen facing the bot from 24 to 35. Every look was kept off. It ends with 4 ingots spare, the crafting table and furnace left standing, and 5 warped stems.
