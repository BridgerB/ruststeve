# Cycle 6 status, hour 8 (2026-10-09 ~10:00 UTC; cycle start 02:50 UTC)

caffeinate pid 7864. Branch `feat/cycle6-honest-lava` at 7ad4c89, pushed. Race i11 running on Server B since 09:50 UTC (build c6-7ad4c89). Local gym server local-1 (25567/25577) up; steve's own local server is on 25569 and untouched.

## Races

| race | build | reached portal step | reached enter_nether | in the Nether | bot-hours past budget |
|---|---|---|---|---|---|
| i8 (cycle 5) | 6b-head | 3/5 | 1 (invalid) | 0 | 10.28 |
| i9 | tree + ASTAR_SYNC + stale-window fix | 5/5 | 3 | 0 | 10.75 (enter_nether 6.52) |
| i10 | c6-8e9cf47 (honest lava, craft fixes, enter fix, decision 9) | 0/5 | 0 | 0 | 5.64 in 157 min (craft_iron_pickaxe 4.07, craft_sticks 1.57) |
| i11 | c6-7ad4c89 (+ mixed-plank craft fix) | running | | | |

- i9: best portal milestone 33 min (rust-race-001), new best (was 42). No bot entered: three sat at enter_nether beside lit portals they could not see (find_block wants line of sight; portal_built does not). 001 logged "no portal found to enter" 1,190 times 5 blocks from its own lit portal (RCON: portal lit).
- i10: stopped by me at t≈9,450 s of 14,400 (race_meta `operator_stop` row): three of five bots stalled on crafting. Cause found the same hour (below); i11 started at once on the fix.

## Measured this cycle (local-1, difficulty Normal)

| change | slug / comparison | old | new |
|---|---|---|---|
| Part 3 honest lava site | portal, landing set A, paired (A11 excluded: respawn at world spawn in the X-ray arm) | X-ray 1/11 (0.02–0.38) | honest 3/11 (0.10–0.57); B−A any pass +0.18 [−0.18, 0.55], sign test p = 0.625; deaths −0.18 [−1.09, 0.73] |
| craft fix 1+2 (verbatim window packets; server cursor + settled acks) | stale_window_craft | 98 s per trial, 3 lost attempts per craft | 6/6 (0.61–1.00), 9.9 s, 0 lost |
| enter_nether finds its own lit portal | enter_own_portal (lit portal out of view) | 0/12 (0.00–0.24) | 12/12 (0.76–1.00), 14 s; sign test p ≈ 0.0005 |
| ingredient search only in the inventory section | mixed_planks_craft (1 oak + 28 cherry) | 0/6 (60 s timeouts) | 6/6 (0.61–1.00), 8 s |
| find_fortress digs down to a fortress below | blaze_rod | 0/10, 1 reached the hunt | 0/10, 10/10 reached the hunt, all died |
| kill_blaze defends against nearby hostiles | blaze_rod | 0/10 | 0/10 (6 deaths) |

Phase 1 verdict: the honest number is not lower on these 11 pairs. Time to the first lava-site decision is the same per landing (B−A +17 s [−109, 110]): both builds first target exposed band-level lava; the deleted read rarely decided first.

Audit result of Part 3: `is_exposed` treated lava as a see-through face (the registry marks lava `transparent`) and unloaded cells as air, so every source inside a buried lake counted as exposed in find_exposed_blocks / find_loaded_nearest too. Fixed in the same build. No X-RAY assertion fired in any honest trial.

Decision 7 was two bugs, not a slot map: client-only moves of grid items on resync (phantoms), then an unread server cursor and an ack wait that consumed the previous click's reply. The race then found a third, older one: ingredient search scanned the grid. Unit tests replay recorded packets / the grid case.

Decision 9 (8e9cf47): one-shot A* only; STALE_WINDOW_FIX on; REFILL_LEGACY and SAFE_FIXES branches deleted (SAFE_FIXES never won: P = 0.50). portal.rs + portal_mold.rs: 4,030 → 3,961 lines (4,009 at cycle start). 189 unit tests pass.

## Late game (local, Normal) — baselines

| slug | result | main cause |
|---|---|---|
| dragon | 0/2 | dragon breath (10 deaths), dragon melee (7), endermen (5) |
| crystals cage v2 | 0/3 | 5-death loops before the cage; v2 untested in effect |
| blaze_rod | 0/30 over three builds | wither skeletons 3, blaze 3, lava 3, hoglin 1 in the last two runs |
| pearls | 1/6 (0.03–0.56) | 4 pearls from 11 ingots once; piglin shot one bot; a gold count dropped 28 → 0 in one toss (unexplained) |

## Infrastructure

- Local server: 6-bot 10-min sample, max average tick 29.8 ms, P99 up to 618 ms (49/60 samples P99 > 30 ms). Decision 10: no second local server.
- Paired arms run in identical fresh worlds one after the other (world reset between arms; same seed 522995494 → same terrain).
- Box: old build dirs deleted, pre-cycle-5 logs tarred; disk 8.8 G → 11 G free.

## Deviations

1. i10 stopped early (above); i11 started at hour 7 instead of a race at hour 12.
2. Blind tunnel kept at y −50, not −56 (lava fills caves up to −55; a −56 tunnel opens lava at body height).
3. cluster_min arm 60/100/150 now means 6/10/15 seen exposed sources within 6 of the target.
4. Blaze work parked after 0/30; the race gate is the overworld and the portal.

## Decisions needed

1. **Server B is `difficulty=peaceful`** (server.properties, confirmed by RCON). No October journal line shows a mob death, so i7–i11 had no hostile mobs. Local gyms run Normal, where mobs cause most deaths (zombies, spiders, creepers, skeletons, a warden; endermen and the dragon in the End). The standing rule says mobs on. Switch Server B to Normal from i12? It breaks comparison with i7–i11.
2. Safety reads of unexposed lava outside site selection remain (`raw_lava_near`, shaft seal, `dig_down`'s buried-lava refusal). Delete under decision 5, or allow as danger avoidance?
