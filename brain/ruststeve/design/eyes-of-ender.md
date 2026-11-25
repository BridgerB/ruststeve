---
title: Eyes of ender
description: How the bot makes its eyes of ender and finds the stronghold with them (plan::eyes, issue 19c) - the eyes row in the 2x2 grid, a throw followed tick by tick into a line on the plane, the dropped eye picked up, the lines crossed by weighted least squares into a fix with its errors, and a throw policy that spends as few eyes as it can.
type: reference
tags: [plan, eyes, stronghold, triangulation, crafting]
aliases: [eyes, eye of ender, throw, Task::Throw, fix, triangulation, eyes_task, eyes_craft_task, eyes_way, EyeRules]
status: draft
lastUpdated: 2026-11-25
verifiedAgainst: ruststeve@c271faf
sourceRefs:
  - "ruststeve:crates/plan/src/eyes.rs#pub fn fix_why(lines: &[Line]) -> Result<Fix, Refusal> {"
  - "ruststeve:crates/plan/src/eyes.rs#pub fn due(rules: &EyeRules, k: &Known<'_>, feet: [f64; 2]) -> Option<Why> {"
  - "ruststeve:crates/plan/src/eyes.rs#pub fn eyes_row(table: &make::Table, counts: &make::Counts) -> Option<make::Row> {"
  - "ruststeve:config/goals.toml#id = \"eyes\""
  - "ruststeve:crates/results/src/snapshot.rs#pub fn read_eyes(path: &Path) -> Result<plan::eyes::Kept, String> {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[goal-table-and-needs]]"
  - "[[pearl-hunt]]"
---

# Eyes of ender

Issue 19 (#24), part 19c: the eyes are made in the warped forest before the way home and thrown in the overworld to find the stronghold. The walls are in `docs/problems/eyes-throw.md`.

## Key files
- `crates/plan/src/eyes.rs` (pure): `fix`/`fix_why`, `Flight` (an eye's positions, `line`, `near_point`), `due` and `way` (the throw policy), `eyes_target`/`eyes_row` (the craft), `Throw` (the task: aim, use, follow, read, pick up), and `impl Plan` with `eyes_task`, `eyes_craft_task`, `eyes_way`, `eyes_kept`/`restore_eyes` and `throw_step` (the one call site, in `step_inner`).
- `config/goals.toml`: the `eyes` goal row and `[eyes]`, every number with its source.
- `crates/plan/tests/eyes_row.rs`: the row on the real goal table (12 pearls and 7 rods make 12 eyes and keep a rod).

## The eye (the 26.1.2 jar, Q5's probe 45685968956)
It spawns at the thrower's (x, y + 0.9, z) and flies straight along the exact bearing to the stronghold point, 12 blocks out and 8 up (or to the point itself and down, within 12), through terrain; after 81 ticks it drops as an item four times in five, else shatters. A use whose look hits an end portal frame throws nothing, so the throw looks up (pitch -10). Nothing is thrown in the Nether.

## The fix
Each line weighs `1 / (range * spread)^2`, its range the distance to the last crossing (four passes). Refused when no two lines differ by 3 degrees or the crossing lies behind a throw. The errors are the covariance along the mean bearing and across it: for two lines `b` apart at `d`, about `d * spread / sqrt 2` across and `sqrt 2 * d^2 * spread / b` along. A point line (the near rule) is the fix itself.

## The policy
Throw 1 on the surface (open sky, no hostile within 16, by day); throw 2 once 200 out and 140 across the first line; a refine only at the fix with no frame seen; at most 4. `Plan.fix`, `Plan.throws` and `Plan.eyes` are serde-defaulted; `eyes_kept`/`restore_eyes` carry them across a snapshot.

## Across a snapshot
Every `StageUp` carries `eyes_kept()`; `race` writes it as `eyes.rsw` beside `memory.rsw` (MessagePack with named fields, zstd: `results::snapshot::write_eyes`). On a resume `race` reads `eyes.rsw` from the `--memory` file's directory (`read_eyes`; a snapshot without it reads as no lines and no fix) and passes it in `Opts.eyes`; the run restores it into the planner before the first tick, which notes `eyes: restored` with the lines and the fix at tick 0. Test: `results` `the_eyes_round_trip_through_the_snapshot_files`.
