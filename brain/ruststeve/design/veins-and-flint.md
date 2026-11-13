---
title: Veins and flint
description: How the bot mines coal, iron and gravel and gets flint (issue 13b) - mine rows with their blocks and depth, stairs down to the depth then a level tunnel, the vein taken whole because the planner looks again from the feet after each ore, a dig that collects a block's chance drops, flint re-dug from held gravel, and making room to place in a narrow staircase.
type: reference
tags: [plan, mine, ore, vein, flint, gravel, stairs]
aliases: [mine-coal, mine-iron, dig-gravel, flint-from-gravel, Redig, redig, depth, tunnel, Stairs::level, vein, chance drops, make room, NoRoom]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@7e42e00 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/redig.rs#pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>, RedigStatus) {"
  - "ruststeve:crates/plan/src/stairs.rs#pub fn level(dir: (i32, i32), tick: u64) -> Self {"
  - "ruststeve:crates/plan/src/place.rs#fn wall(&self, w: &World) -> Option<Cell> {"
  - "ruststeve:crates/bot/tests/iron.rs#fn flint_is_dug_out_of_held_gravel() {"
  - "clojurecraft:src/clojurecraft/vein.clj#(def vein-radius \"Blocks from the eye an ore of the vein is still taken, before moving on.\" 8)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[stairs-down]]"
  - "[[digging]]"
  - "[[placing]]"
  - "[[fuel-and-smelt-needs]]"
---

# Veins and flint

Issue 13b.

## How it works
- **Mine rows** name their blocks and a depth (`mine-coal` and `dig-gravel` 40, `mine-iron` 16, clojurecraft's): the nearest exposed block of the kinds (no X-ray, [[stairs-down]]'s rings) with a route is walked to and dug; with none, stairs down; once the feet are the row's depth below where the planner began, a **level tunnel** (`Stairs::level`: six level steps, each counted as a stair) instead.
- **The vein**: after each ore the planner decides again, and the exposed search runs out from the feet, so the ore the last dig uncovered beside it is the next taken while the row still wants its item: the whole vein before moving on (clojurecraft's `vein.clj`, by construction here).
- **Chance drops**: a dig collects what the block always drops when nothing rarer does (gravel's gravel) and its chance drops before that (gravel's flint): either one picked up ends the collect.
- **Flint** (`flint-from-gravel`, `Act::Redig`): a held gravel is put down beside the feet and dug again, one cycle a task; the planner asks again until flint is held (one in ten); the gravel is a need, so running out of it sends the bot for gravel.
- **Room to place** ([[placing]]): with no free floor cell beside the feet (a one-wide staircase whose only free cell the table took), a wall block at the feet's height with a full floor under it, no fluid near, is dug first (at most two).

## Evidence
- Model: the kit from the `stone` kit (three iron as a vein, coal, stone by stairs, the furnace placed after a wall was dug); flint from thirty held gravel by re-digging.

## Limits
- Depth counts from where the planner began, not the surface above the feet.
