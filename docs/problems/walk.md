# Problem: reaching a log that walking cannot reach

Issue 09's measure (`simrace --landings-file config/landings-grid.toml --goal log:30`, the `walk`
job in ci.yml) runs the planner in 320 landings captured on the race seed. On the first run of
the full grid, 13 to 15 landings with a log 30+ blocks away in view never reached one.

## What the bot needs
To reach a log it can see when no walking route reaches any: swim across water, dig through a
bank or step, or pillar up a cliff.

## Risks
- A bot that cannot leave an island, a mangrove swamp or a cliff pocket stands until its cap.
- Treating water as a wall (clojurecraft's rule, `path.clj`) keeps bots out of drowning traps; a
  route through water needs the escape rules first (issue 12).

## Done, in world terms
From a landing whose only logs in view are across water, under a bank or up a cliff, the bot
reaches one: in the measure's raw line ("any log 30+ in view, walkable or not"), the rate reaches
the qualifying rate.

## The last recorded failure
Measure on d683c9b + 09b (local, 2026-11-12): raw 216/231 (93.5%); every one of the 15 misses
had no walking route to any log in view, checked with a 200,000-node search over the 40 nearest
(`bot::landings::walkable_log`): starts in mangrove swamps (mud, roots), on sand islands, in
grass or stone pockets under cliffs. Since Bridger's decision (2026-11-12) such landings do not
qualify for issue 09 (a walkable log in view is required); they are counted every run as "a log
in view but none walkable" and wait for issues 11 (digging, safe stairs) and 12 (escaping
water).

## Found and fixed on the way (09b)
- Landing starts on a tree's canopy (14 of 22 early misses): starts now stand on the ground
  under leaves, as vanilla places a player (`MOTION_BLOCKING_NO_LEAVES`, `ModelWorld::ground`).
- Starts with leaves at head height (the body began embedded): starts now need two free cells
  (`ModelWorld::placeable`).
- Targets down caves or up cliffs were chosen on a partial route: a walk to a log now needs a
  found route, or a partial one toward a column not loaded yet (clojurecraft's `walkable?`).
- With every log refused, the planner stopped instead of exploring: explore is now the fallback
  in the same choice.
