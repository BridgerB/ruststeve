# ruststeve cycle 5: status at hour ~14.7 (2026-10-04 10:20 MDT)

Written early, ahead of race i8, which starts ~11:00 instead of ~13:40 because the gym is idle until then. Every number is from gym.db, race.db or the box logs; intervals are Wilson 95%; P(B>A) uses Beta(1,1) priors.

## Since hour 8

### Race i7 (target-head, 5 bots, 240 min)

| reached (of 5) | i5 | i6 | i7 |
|---|---|---|---|
| Craft Buckets | 1 | 2 | 3 |
| Portal step | 0 | 2 | 2 |
| Nether | 0 | 0 | 0 |

- 3 deaths (all lava) and 8 relaunches.
- Where bots ended:
  - water, 3/5: gather_wood walked into a lake, despite a land placement;
  - a 2,500-block blind tunnel at y −60;
  - a tool loop at depth.
- Harness defect: a respawn check measured from the lane start. Fixed after the race.

### iron_from_surface (Phase 3)

- Baseline 1/10 (0.02–0.40).
- Cause: the step picker kept bots on mine_iron until the full 22-iron quota (no furnace before then), so no iron pickaxe was made in 1,200 s. The last-line "NO PICKAXE" was a stale log line, not the cause.
- Build b08ffcd (craft an iron pickaxe at 3 iron; plus a table fix: cave_air counts as empty): **5/10 (0.24–0.76)**, P(better) = 0.968.
- One loop remains (furnace craft desync); bounded in 4e6c4a6.

### Tree vs 6b-head on portal

- Wall-clock sliced A*: 1/15 against 8/10 (regions 62 and 68).
- With the search-time A* budget (`ASTAR_SEARCH_BUDGET=1`), region 69: 4/7 against 6/10, P(tree > head) = 0.45, i.e. on par.
- That arm was stopped by clause (a) (one bot died twice at one mold site).
- One contaminated pass excluded: a bot entered another bot's portal; this only happens with shared-region arms.

### Late game

- Dragon 2/2 by the server check (hours ~3.5 and ~11).
- Crystals slot, one build, cage climb: **0/3**. The bow killed 3–4 of 10 per trial; climbs reached tower tops but opened 1 bar in total. The geometry and kit fixes are listed in CHANGES.

### Water set

6b-head 13/14, the same as 6b-safe 12/14. The water slugs do not reproduce the race water wall.

## Race i8

target-head, the same binary as i7 (decision 1). This keeps i7 and i8 comparable; the fixes measured since (early iron, table, A* budget) are on the other tree.

## For the handoff

- Strongest candidate for the next race build: the current tree with `ASTAR_SEARCH_BUDGET=1` + early iron + the table fix.
  - Matches 6b-head on portal.
  - Fixes the iron quota stall.
  - Restores event rows and bandits.
  - Still needs the clause-(a) death at the mold site understood first.
- find_lava_cluster reads unexposed lava (57/64 portal passes depended on it): a strategist decision.
