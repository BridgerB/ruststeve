# ruststeve cycle 5: status at hour ~7.5 (2026-10-04 03:11 MDT)

Cycle start 2026-10-04 01:40 UTC (2026-10-03 19:40 MDT). Branch `feat/cycle5-race-funnel`, pushed through `bae1d82`; later commits local until the post-race push. Every number below is from gym.db, race.db or the gym/race logs on the box; intervals are Wilson 95%.

## Decision 1: 6b-safe did not qualify; race i7 runs 6b-head

| arm | binary | any-pass | clean | deaths / trial | notes |
|---|---|---|---|---|---|
| A 6b-head | target-head (old tree) | 8/16 (0.28–0.72) | 6/16 | 20/16 = 1.25 | region 57, 2 skipped (no land), cap reached |
| B 6b-safe-p | target-safe3 (GYM_PARITY) | 3/6 (0.19–0.81) | 3/6 | 5/6 = 0.83 | region 59, stopped by clause (a) |

- First B batch (region 58) excluded as a harness fault: B had the new same-failure-4× cut and A did not. Fixed with `GYM_PARITY=1`, B restarted.
- B stopped by clause (a): rust-gym-004 died twice in 31 s in lava at one mold site, both during mold goto moves, a path 6b-safe changed. Below 12 trials, so compare.ts cannot pass it; per decision 1 the race runs 6b-head.
- Region-62 matched check, all three builds in one region:
  - 6b-head: 4/6.
  - The current tree with SAFE_FIXES off: 1/6, and 1/12 across both its batches.
  - Cause found: the sliced A* budget was wall-clock, so the search got about 0.9 s of its 2 s.
  - The A* fix (`7d3e236`) was itself stopped by clause (a): two bots died going to a remembered pool that turned out dry.
  - So race i7 runs **target-head**, which has no event rows and no bandit arms (a deviation from decisions 5 and 6 for this race).

## Fixes and audits (committed)

- Frame check by stored mold cells instead of a radius count. Reproduced as a 102-attempt hang on 6b-head behaviour (n=1).
- settle_xz lava refusal: it fired 13× at one site, all on real lava.
- Per-process log, aborted rows, same-failure rule, respawn check (gym).
- Race placement on land: i7 put 5/5 bots on land.
- Re-site-after-death counter (one false positive, fixed); budget cuts logged with the alternative taken.
- Per-bot bandit arms; iron_from_surface slug; SAFE_FIXES gate; GYM_PARITY.
- find_lava_cluster audit:
  - It reads lava with no exposure test.
  - 110/138 logged portal trials used it, including 57/64 passes.
  - Race i6: 2/5 bots.
  - Not fixed; this needs a strategist decision.
- Lava-rule path audit: settle_xz was the only raw walk on the portal path with no lava check.
- Tick-rate test rejected by audit: fixed 50 ms client tick, wall-clock timers.
- Dragon gym, mandatory: PASS by the server check, 0 deaths.

## Race i7 (race-20261004-070530-i7, target-head, 5 bots, started 01:05 MDT), t ≈ 2 h

| bot | step now | y | furthest milestone | deaths |
|---|---|---|---|---|
| 001 | craft_bucket (~35 min) | 63 | Build Nether Portal (started) | 1 |
| 002 | build_nether_portal | −54 | Fill Water Buckets → portal | 2 |
| 003 | build_nether_portal | −60 | Craft Buckets → portal | 0 |
| 004 | gather_wood | 63 | none (in water since ~t=10 min) | 0 |
| 005 | gather_wood | 63 | none (in water; relaunched once) | 0 |

- Placement worked: no bot started in water. 004 and 005 walked into water while gathering wood. This is 6b-head behaviour (the old tree lacks the cycle-3/4 water fixes).
- Harness defect: the race respawn check measured from the lane landing, so two portal bots were tp'd off their molds. Fixed locally (`race-b.sh`: a far respawn needs to be far from the last position too). Not deployed mid-race.
- Deaths so far, all lava: "pre-frame heal → hp=0" at the scoop stand (001), and 002 ×2.

## Next

- Race ends ~05:05. Then:
  - race report (funnel vs i5/i6, tick stats, deaths by cause, time-to-portal);
  - sync the race-b.sh fix;
  - iron_from_surface baseline (10 trials);
  - largest step past budget in i7;
  - bandit batch, once a tree with bandits measures as well as 6b-head;
  - dragon gym ~hour 12;
  - crystals slot at hour 20;
  - race i8 ~hour 18.
- Builds: four builds this window changed portal behaviour or the pathfinder (safe2, race-gate, fix, plus the original safe), one over the limit of three per 24 h. No further portal builds until 19:40 MDT.
