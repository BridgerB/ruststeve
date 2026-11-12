# Cycle 6 status, hour 20 (2026-10-09 ~23:40 UTC; cycle start 02:50 UTC)

The hour-16 status was missed: the Mac slept from about 15:57 to 23:18 UTC (on battery; caffeinate pid 7864 was alive, so most likely the lid). Race i12 finished at 17:52 on its own; the box then sat idle about 5.5 h. Race i13 started 23:21 UTC on c6-bc575e3.

## Races since hour 8

| race | build | portal step | in the Nether (own portal) | minutes to the Nether | bot-hours past budget | largest past-budget step |
|---|---|---|---|---|---|---|
| i9 | tree + ASTAR_SYNC + stale fix | 5/5 | 0 | — | 10.75 | enter_nether 6.52 (line of sight) |
| i10 | c6-8e9cf47 | 0/5 | 0 | — | 5.64 in 157 min (stopped) | craft_iron_pickaxe 4.07 (mixed planks) |
| i11 | c6-7ad4c89 | 4/5 | 2 | 78, 231 | 4.58 | find_fortress 2.45 (arrival-portal pocket) |
| i12 | c6-acc95c5 | 4/5 | 3 | 132, 141, 219 | 4.15 | find_fortress 3.02 (out of cobble over the lava sea) |
| i13 | c6-bc575e3 | running | | | | |

i11 rust-race-004 at 78 min is the fastest natural Nether entry this project has (cycle 3 i6: 177 min). Every entry above was through the bot's own lit portal (log: `cast & lit at …` then `enter_nether: own portal at …`). No bot has reached a fortress yet.

The race funnel stopped at "Enter Nether" and showed i11 rust-race-004 as furthest = Smelt Iron; it now lists the post-portal steps and each bot's first Nether tick (scripts/ml/race-funnel.ts).

## Fixes since hour 8, each with its slug

| fix | slug | old | new |
|---|---|---|---|
| ingredient search only in the inventory section (mixed plank types) | mixed_planks_craft | 0/6 | 6/6, 8 s |
| arms run against arm C: craft fixes + decision 9 on the portal | portal, landing set A, paired (11) | arm H 4/12 | arm C 4/11; B−A 0.00 [−0.36, 0.36] |
| Nether tunnel reads the feet row, not the floor (y = 89.9999) | nether_portal_exit (pocket, no fortress within 200) | 6/12 | 12/12, 57 s |
| portal blocks have no collision (vanilla); generated data had a full cube | enter_portal_front / enter_own_portal | 12/12 / 12/12 14 s | 12/12 / 12/12 8 s |
| Nether bridging uses netherrack when cobble runs out | nether_far (fortress 160 blocks along X, 16 cobble) | running | running |

Not shipped: two attempts to walk onto the portal cell centre after the approach dropped enter_own_portal to 8/12 and 10/12; reverted.

## Late game (local, Normal)

dragon 0/3 (died before any detonation each time: dragon breath, dragon melee, endermen); crystals cage v2 0/3 (death loops before the cage); blaze_rod 0/30 across three builds; pearls 1/6.

## Deviations

1. The Mac slept ~7 h; the hour-16 status and the hour-14 dragon slot slipped (dragon ran at hour 12).
2. Races run back to back as soon as a fix measures non-worse, not at hours 12/24.

## Decisions still open (from hour 8)

1. Server B is Peaceful; races i7–i13 had no hostile mobs. Switch to Normal?
2. Safety reads of unexposed lava outside site selection: delete or keep?
