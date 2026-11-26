# Issue 19c: the eyes, crafted, thrown, read and crossed (Q3)

**Status: built to unit tests (2026-11-25); not yet thrown on a server.** Part of [`eyes.md`](eyes.md).

## Needs
- The eyes made where the bot stands in the forest, in the 2x2 grid, before the way home: as many as the held pearls make, up to 14, a rod spent only for powder needed.
- In the overworld, on the surface, eyes thrown and followed: each flight read into a line on the plane, its end recorded (dropped or shattered), a dropped eye walked to and picked up.
- The lines crossed into a fix with its errors, refused when they say nothing (parallel, or crossing behind a throw).
- Few throws: every eye spent is issue 20's margin.

## What is built (`crates/plan/src/eyes.rs`, `[eyes]` and the `eyes` row in `config/goals.toml`)
- `eyes_row` / `Plan::eyes_craft_task`: the row's count is `min(eyes_max, eyes + min(pearls, powder + 2 * rods))`; the needs planner chains powder from rods. Real-table test: 12 pearls and 7 rods make 12 eyes and keep 1 rod; 14 pearls use all 7.
- `Task::Throw` (`Plan::eyes_task`, `Plan::throw_step`): the eye held, the look settled 2 ticks at pitch -10 along the best bearing (look::keep_off still applies in `Plan::step`), `use_item`; the new eye entity within 3 of (feet + 0.9) followed every tick (distinct positions only); at its removal the reading is taken; a new dropped ender_eye within 3 of its last position in 20 ticks is walked to (within 24, 600 ticks, 3 walks), else it shattered.
- `fix_why`: weighted least squares, reweighted by range; covariance read along and across the mean bearing.
- The policy (`due`, `way`): throw 1 on the surface; throw 2 once 200 out and 140 across the first line (the way there turns the bearing 45 degrees); a refine at the fix (within 64) only with no frame seen; at most 4.
- Ledger notes: `eye throw N (why)` with its spot, yaw and eyes held; `eye throw N (why) waits` with what blocks it; `eye flight: heading ...` or `eye flight: sank within 12 ...` with positions and ticks; `eye ended: dropped at ...` / `shattered at ...` / `dropped out of reach`; `eye picked up` / `eye pickup given up`; `fix (x, z) err along A across C` / `no fix from N lines`; `eye throw over` with thrown, kept, lost, shattered; `eyes: craft ...`.

## Numbers (the jar, Q5's probe 45685968956)
The heading is exactly the bearing (no curve), so a line's spread is only the positions' quantization: the floor is 0.2 degrees. At 1,500 off, a 140 baseline gives about 75 along: inside the frames' 128-block view. The eye ends after 81 ticks; it spawns at (x, y + 0.9, z).

## Risks
- An always-row (defend, hunt) cutting a throw mid-flight loses that line and the eye's pickup.
- The client sees the eye every 4 ticks or so: about 20 positions a flight.
- A frame in the look's block hit throws nothing (pitch -10 up).
- Resume: built. Every `StageUp` carries `eyes_kept()`; `race` writes it as `eyes.rsw` beside `memory.rsw` and reads it back on a resume (`Opts.eyes`, restored before the first tick, noted `eyes: restored`). A snapshot without the file starts with no lines.
- The first throw has no bearing, so its drop column cannot be checked. Guard (the coordinator, 2026-11-25): with a log or leaf seen within 6 on the plane over the feet (`trees_near`), the plan walks 16 to the side with the fewest (`open_side`, `Task::Explore`, noted `eye throw 1 (why): walk to open ground`), at most 4 times, then throws anyway; a canopy that still catches it loses that eye at once by the rest rule.
- Who calls `eyes_task`, `eyes_craft_task` and `eyes_way`: the eyes phase after Home (Q2 / Q4 / the orchestrator).

## Done (world terms)
A dev-start race on the surface: the ledger shows a throw, a flight with its heading, the end, the pickup, and after throw 2 a fix within 128 of the stronghold.

## Walls

### A drop at rest on a canopy (Q5's parity 45688030470)
8 real throws on 7 landings, two strongholds: every line within 0.0006 degrees of the bearing, the eye out to 12.15 and up 8.18 about 34 to 38 ticks after the throw, then hovering, removed at 81 ticks. 7 of 8 dropped, 1 shattered; 2 of 3 drops picked up. The third came to rest on a tree canopy at feet +4: a pickup reaches about feet +2.3, so the 600-tick walk could never end in a pickup.

Fix (structural, two parts):
- `apex_blocked`: a throw with a bearing waits (noted `eye throw N (why) waits`, "the drop's column ... is blocked at ...") until the column `apex_out` 12 out on the bearing, from the feet's level up `apex_up` 9.1, holds no solid block seen. Tests: `crates/plan/tests/eyes_drop.rs`.
- The rest rule: a drop whose height is unchanged for `rest_ticks` 8 more than `rest_rise` 2 over the feet is given up at once (`eye pickup given up: rests high`). Test: `a_drop_resting_over_the_reach_is_given_up_and_a_falling_one_is_not`.

Not yet counted on a server: both notes are the count to read in the first real race.

### Throw 1 rested 9 up with no walk first (Q4's race 45698008920, both lanes)
The guard looked for trees within 6 of the feet, but the eye comes down about 12 out and 9 up: the canopy that caught it was outside the 6. Fix: `ring_blocked` checks the eight sides 10 to 14 out from the feet's level up 10 (the same column test as `apex_blocked`); any side blocked (`first_blocked`, or a tree within 6) walks 16 to the point whose ring has the fewest sides blocked (`open_side`), at most 4 walks. A clear side does not help: the eye flies the stronghold's bearing, not the look's. Test: `a_canopy_12_east_blocks_throw_1_and_the_walk_leaves_it`. Count to read in the next race: `walk to open ground` before throw 1, and no `rests high` on it.

### Both throws a bot made rested 9 up (Q4's overland races)
Even with the ring guard and its walks, each bot's two throws came down in a canopy 9 up and were given up: in a real run a pearl each, 12 left for issue 20's 11. Fix: a drop at rest 2 to 12 over the feet with filler held is fetched (`eyes::Climb`, `Phase::Climb`): a walk to within 1.2 of its column (or from within 4 when the ground goes no nearer), a pillar of held filler (jump, a block into the cell the feet left) with leaves over the head dug through, a wait at the drop's height, then the pillar's own blocks dug back top first. Given up only with no filler, a drop over 12, or after 400 ticks; the pillar comes down either way. Notes: `eye at rest high: fetch it`, `eye fetch: pillar up`, `eye fetched from a canopy`, `eye fetch: back down`, `eye fetch given up`. Test: `crates/bot/tests/eyes_fetch.rs` (the model: leaves at +9, 3 east). `Plan::fetch_eye_task` starts a fetch of a drop seen without a throw.

### No walk within 4 of the drop (Q4's race 45732530352 lane 0)
The fetch fired and kept an eye in lane 1; in lane 0 it gave up: `no way within 4 of the drop (Failed(NoPath))`, the drop 12 off and 9 up at (36.5, 73.0, -105.8). Fix: the approach cuts its way (`ClimbStage::Cut`): a level cut toward the drop's column, a step at a time on the axis it is furthest off (the feet and head cells dug while solid, never beside lava or water seen), 16 steps at most (`climb_cut_max`), then the pillar. The whole fetch is given up after 600 ticks (`climb_ticks`). Note: `eye fetch: cut toward the drop`. Test: `an_eye_behind_a_wall_with_no_way_round_is_fetched_through_a_cut`. The bridge across at the drop's height is not built.

### The cut did not start; a pillar begun 3.2 off (Q4's race 45740169100 lane 0, a1afa9f with 01612f0)
- The cut's first step into (29, 64, -109) failed `NoPath`: the step was a walk with a route. Now a step digs its feet and head cells, lays a floor from held filler against the side of the block under the feet when none is under it, then steps straight in with no route (60 ticks a step at most, `climb_step_ticks`).
- "pillar up (from (188, 68, 61), the drop 1.0 over the feet and 3.2 off)" then 600 ticks with 0 blocks up: a walk that ended within 4 began the pillar there, and the drop, 1.0 over the feet then (the bot had walked up since it rested 9 over them; the height is read against the feet now), was already reached in height but 3.2 off on the plane: it waited. Now a pillar (and the wait) begins only within 1.2 of the column (`climb_beside`); further off, a cut goes beside it first. `climb_near` is gone.
- Tests: `a_cut_step_onto_a_missing_floor_lays_one`, `a_pillar_is_refused_3_off_and_a_cut_goes_beside_first`; every fetch test checks each `pillar up` note is within 1.2.
