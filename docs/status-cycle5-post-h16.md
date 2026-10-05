# ruststeve cycle 5, post-handoff status 2 (2026-10-05 10:50 MDT, handoff + 15 h)

Every number below is from gym.db and the box gym logs. Intervals are Wilson 95%; P(B>A) uses Beta(1,1) priors.

## Recommended race build for cycle 6: the tree with `ASTAR_SYNC=1`

| | tree + one-shot A* | comparison |
|---|---|---|
| portal (regions 84/85, separate) | 11/17 | 6b-head 8/18, P = 0.88 |
| water wall (rebuilt 33×33 pool) | 4/10 | tree + sliced A* 1/10, P = 0.93 |
| arena water set | 14/14 (on the water-fix build) | 6b-head 13/14 |
| iron_from_surface | 5/10 (early iron) | 1/10 baseline |

It also carries:
- event rows and bandits;
- the table fix (cave_air);
- the frame-anchor and respawn race-harness fixes (in race-b.sh);
- the stale-window craft fix (unmeasured).

## What changed since the 08:00 report update

- **The tree's portal gap was the sliced A*.** Wall-clock budget 1/15 vs 6b-head 8/10; search-time budget 11/25 vs 16/27; one-shot 11/17 vs 8/18.
- **Water wall.** Fixes: lily-pad false bank, the SDK physics porting bug in the out-of-water lift (`does_not_collide` got a velocity offset, not a position), and the shallow-water mine_stone loop.
  - Measures: a fixed real lake wore out (target-wade 4/10 then 0/10 at one spot); random real lakes were too slow to find; the rebuilt pool is the measure now.
- **Tool loop at depth.** Race cause: `CRAFT 36: result not seen in slot 0 (grid [])`; a 2×2 craft clicked into a stale table window. Fixed (3968bcf), unmeasured.
  - The tunnel slug passes 10/10 because it never has a stale window.

## Open

- Measure the window fix: a slug that opens a table, walks away, then crafts 2×2; or the next race.
- Crystals v2 (list in the report, section 7).
- find_lava_cluster honesty (decision 3).
