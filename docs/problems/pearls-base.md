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
(none yet)
