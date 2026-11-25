# Issue 18c: from the endermen pocket to a warped forest (agent P2)

The orchestrator's index is `docs/problems/pearls.md`; this file is agent P2's (the hand-over, the travel, the heading's rules past the endermen stage).

## Needs
A real bot resumed from the `endermen` frontier (race-45481633896 lane 3: sealed in a 12-block pocket at about (8, 85, 81) in a soul sand valley, health 12.5, food 17, no food held so it cannot heal, no shield, 144 pickaxe uses, about 588 filler, 7 rods) leaves its pocket and reaches a warped forest alive with its 7 rods: the feet's biome `warped_forest`, stage `forest`.

## Done (world terms)
A case file from a race resumed at the frontier shows `stage reached: forest`, and the race's release carries its `snapshot-forest-…` asset.

## Risks
- No healing: every hit on the way is kept (12.5 is about three enderman hits, two piglin brute hits).
- This world's only warped forest within 600 of the spawner lies about 454 blocks east-north-east behind about 300 blocks of basalt deltas (17a, development knowledge: never in code). The heading has never raced, and today it turns away from basalt signs.
- Ghasts in the valley and the deltas, magma cubes in the deltas, piglins (hostile: no gold worn), wither skeletons near the fortress.
- One world: every frontier race is the same seed, forest and route.

## What is built (branch `issue-18-p2-travel`)
- `EnderState.phase: EnderPhase { Reach, Travel, Hunt }` (serde default `Reach`).
- The hand-over: under `Aim::Race` a pocket closed (or one that cannot be sealed) at the stage notes "endermen reached: on to the forest" and moves to `Travel`; `met` is no longer set there. The stage seen by the world's ladder (`w.stage_reached >= endermen`) also counts as reached, for a resumed bot's first look.
- The way out: `endermen::exit_side` picks a side of the pocket (no lava beside the cells it opens; first a side not facing a ghast, blaze, skeleton, wither skeleton, piglin, brute, hoglin or magma cube within 48; then the longest standing run past the wall; then toward a known forest), and `exit_step` digs its head and feet cells ("pocket opened at", "out of the pocket to"), at most 4 digs.
- In `Travel`: no approach of endermen, no valley (the heading's valley signs are dropped too), no forest wait. A known forest is walked to by cell (`endermen::forest_target`: seen warped nylium in a known forest column with room for a body over it, within 12 of the feet's height first, then the nearest), ending within 1 of the cell; else the heading. A "to the forest" note every 1,200 ticks and every heading leg's note carry health, food, pickaxe uses and filler.
- The feet's biome `warped_forest` moves to `Hunt` ("warped forest reached: on to the hunt"); `Plan::pearls_task` (P3) runs there (a stub returning `None` until it merges).

## Walls
(none yet)
