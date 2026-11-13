---
title: Stairs down
description: How the bot goes underground safely (plan::stairs, a port of the core of clojurecraft's stairs.clj) - a stair opens three cells one block ahead and down and is walked onto, the block under the feet is never dug, every cell is checked for water, lava, an unloaded column and a floor before anything is dug, refusals turn a quarter, level steps when boxed in, done after six stairs.
type: reference
tags: [plan, stairs, mine, safety, underground]
aliases: [Stairs, StairStage, refusal, Boxed, level step, STAIR_DEPTH, never dig under the feet, stairs-down, mine stone, exposed_near]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@3fa1b05 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/stairs.rs#pub fn refusal(w: &World, cells: &[Cell], floor: Cell) -> Option<String> {"
  - "ruststeve:crates/plan/src/stairs.rs#fn plan(&mut self, feet: Cell, w: &World) -> Result<StairStage, StairsFailure> {"
  - "ruststeve:crates/plan/src/lib.rs#pub fn exposed_near(w: &World, kinds: &[data::BlockKind]) -> Vec<Cell> {"
  - "ruststeve:crates/bot/tests/stairs.rs#fn water_lava_and_drops_refuse_a_stair() {"
  - "clojurecraft:src/clojurecraft/stairs.clj#Going down safely: the :stairs-down intent, which cuts a staircase one stair at a time in the"
  - "clojurecraft:src/clojurecraft/stone.clj#(def search-radius 24)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[mc-water]]"
  - "[[digging]]"
  - "[[goal-table-and-needs]]"
---

# Stairs down

Issue 11b. The mine act (`mine-stone`) digs the nearest **exposed** block of its kinds the bot can stand beside, and when none is in sight cuts stairs down. Blind drops into lava killed clojurecraft's siblings, so the stairs never dig under the feet.

## Key files
- `crates/plan/src/stairs.rs`: `Stairs`, `refusal`.
- `crates/plan/src/lib.rs`: `exposed_near`, `Plan::mine`.
- `crates/bot/tests/stairs.rs`, `crates/bot/tests/stone.rs`.

## How it works
- **Exposed** means a face open to air, read from the loaded terrain within 24 blocks and 12 up or down (clojurecraft's `search-radius`, `search-height`): stone is most of the world, and only what is exposed now is worth a route. The search goes ring by ring outward and stops at 8 found (a whole-radius scan cost 0.4 s in one tick); the nearest 3 not refused are routed with the pickup goal and the first with a way is dug ([[digging]]). No block without an open face is ever chosen: no X-ray.
- **A stair**: from the feet F along the heading (dx, dz), open (F+dx, y+1), (F+dx, y) and the new feet (F+dx, y−1), top first, each by the dig task (its drop collected: digging stone stairs gives cobblestone), then walk onto the new feet. The block under the feet is never dug.
- **Before any cell is dug** (`refusal`): an unloaded column, water or lava in a cell, lava beside a cell, or no full block under the new feet refuses the stair; the heading turns a quarter. All four refused: a level step (two cells along, a floor under them), at most 3, then `Boxed`.
- **Done** after 6 stairs (`stair-depth`): the planner looks again. Judged by the feet having moved, never by a packet sent.
- A stair that fails makes the planner turn its heading for the next.

## Evidence
- `crates/bot/tests/stairs.rs`: a clear stair is taken; water in a cell, lava beside one, a missing floor and an unloaded column each refuse it.
- Sim measure from the `table` start: 195 of 200 landings make a stone pickaxe and sword in 12,000 ticks.

## Limits
- Gravel or sand overhead is not checked (it falls on the bot); the stairs do not go up again (clojurecraft's stairs-up), nor lay a floor over a cave (its cast floor). Air underwater and drowning are issue 12.

## See also
- [[mc-water]] - why water and lava are refused.
- [[digging]] - the dig each cell is.
