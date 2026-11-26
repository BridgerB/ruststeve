# Issue 19: checkpoints on the way home (agent Q10)

The orchestrator's index is [`eyes.md`](eyes.md); the trip's code and walls are Q8's ([`eyes-home.md`](eyes-home.md)). This file is agent Q10's: real mid-trip snapshots (`snapshot_at`, kind "at") so the trip's later stretches (the fortress at x 60 to 120, the portal at (22, 34, -20)) get many attempts per race instead of one replay of the whole trip.

## Needs
Each hard stretch 4 to 6 real tries a race, from a real snapshot taken just before it (never a development start). The moment a lane reaches stage `overworld`, its `snapshot-overworld-…` asset goes to the orchestrator.

## Timing from the resume (race 45753006452 lane 1, b8dc00e, from `race-45722775712/snapshot-at-16000-…-lane-1`)
The resume stands at about (311, 36, 194) on the old bridge's east span. A lane that does well: x 210 at tick 1,200 (the west span), x 180 to 185 at 2,000 to 3,200 (stairs at y 51), x 140 at 3,700, x 95 to 105 at 4,400 to 7,000 (soul sand valley, the fortress's east side), x 76 to 85 at 8,000 to 10,000 (the fortress at y 61); that lane was struck to 5.6 by wither skeletons at 10,200 and held a pocket to the end.

## Races (Q8's commit, resume point, snapshot_at, cap, lanes; result)
| race | commit | from | snapshot_at | cap | lanes | result |
|---|---|---|---|---|---|---|
| 45761710316 | 43297ed | resume at-16000 (45722775712 lane 1) | 3600 | 14000 | 4 | cancelled: 43297ed's guard clicked one cell forever at x 309 to 313 (race B11) |
| 45761721070 | 43297ed | resume at-16000 (45722775712 lane 1) | 7600 | 20000 | 4 | cancelled, the same |
| 45764230971 | 1543fcb | resume at-16000 (45722775712 lane 1) | 3600 | 14000 | 4 | cancelled at ~4,800: all 4 lanes at x 309 to 315 by the join, restocking for the spans' guard (152 wanted) on the resume's island; "restock over (-10 blocks gained in 46 digs, 4738 ticks)": the guards laid while restocking eat the filler; then every leg "no way" (far walk 48 to bridge, 19 spare), 3 to 5 "guard given up" each |
| 45764234506 | 1543fcb | resume at-16000 (45722775712 lane 1) | 7600 | 20000 | 4 | cancelled, the same (lanes 0, 1 at x 311, restock 56 to 59 of 152) |
| 45765424576 | 1543fcb | stuck snapshot 45742350492 lane 1 (49, 33, 206), hp 19.8, food 7, west of the fortress | 6000 | 20000 | 5 | 5 caps, 0 deaths, none south: at 16,800 lanes at (81, 87, 151), (101, 43, 205), (56, 49, 197), (171, 74, 196), (78, 32, 206). The only walk south refused "for cut (paced)" at tick 280 in all 5; restocks negative from guards (-10 in 21 digs); owed restocks walked east to x 95 |
| 45765427829 | 1543fcb | stuck snapshot 45742350492 lane 0 (74, 33, 201), hp 15.6, food 9 | 6000 | 20000 | 3 | cancelled at ~tick 10,000 for 694f582: x 58 to 75, no progress |
| 45768703328 | 694f582 | stuck 45742350492 lane 1 (49, 33, 206) | 6000 | 20000 | 5 | lane 1 ghast fireball at 7,520; at 16,800 the rest at (103, 38, 163), (88, 54, 201), (167, 75, 206), (56, 59, 122); lane 2 a spawner snapshot |
| 45768707240 | 694f582 | stuck 45742350492 lane 0 (74, 33, 201) | 6000 | 20000 | 3 | **lane 2 to (22, 52, -8), 17 from the portal on the plane**, cap, no pickaxe from 18,180 (the last leg, a 71-step tunnel down from y 59, spent 156 uses; wood gone; 464 netherrack held); lane 1 ghast fireball at 7,520; lane 0 cap, "no far walk" at (64, 63, 210) |
| 45771610530 | 5ea5a66 | at-6000 of 45768707240 lane 2 | 5000 | 16000 | 6 | running |
| 45771663969 | 5ea5a66 | frontier (pearls_12, the forest) | 12000 | 24000 | 2 | running |

## Resume points (real snapshots past the join, from earlier races' case files)
| snapshot | where | health | note |
|---|---|---|---|
| race-45742350492/snapshot-stuck-rs-l01-b00-…-lane-1 | (49, 33, 206) | 19.8 | furthest west, past the fortress's x; 48b21ef's world |
| race-45742350492/snapshot-stuck-rs-l00-b00-…-lane-0 | (74, 33, 201) | 15.6 | under the fortress |
| race-45746355032/snapshot-stuck-rs-l01-b00-…-lane-1 | (163, 40, 216) | 7.6 | at the danger line: not used |
| race-45753006452/snapshot-stuck-rs-l01-b00-…-lane-1 | (78, 61, 159) on the fortress | 5.6 | at the danger line: not used |
| race-45768707240/snapshot-at-6000-…-lane-2 | about (70, 50, 110 to 150), tick 6,000 of the lane that reached 17 from the portal | 15.6 | the portal stretch's resume |
