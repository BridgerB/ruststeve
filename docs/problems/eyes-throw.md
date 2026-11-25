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
- Resume: Plan state is not in the snapshot. `Plan::eyes_kept` / `restore_eyes` (serde struct `eyes::Kept`) are ready; the snapshot must write and read them (Q1 / the orchestrator own `results` and `race.rs`).
- Who calls `eyes_task`, `eyes_craft_task` and `eyes_way`: the eyes phase after Home (Q2 / Q4 / the orchestrator).

## Done (world terms)
A dev-start race on the surface: the ledger shows a throw, a flight with its heading, the end, the pickup, and after throw 2 a fix within 128 of the stronghold.

## Walls
(none yet)
