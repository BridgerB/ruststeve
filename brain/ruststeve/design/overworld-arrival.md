---
title: The overworld arrival
description: How the bot climbs from its own overworld portal by the lava sea (feet y -53) to the open sky on the way home (plan::arrival, issue 19) - a one-wide shaft straight up in rock, a dig and a jump a block, sealed below by its own blocks; refused at caves, fluids and falling blocks, where the way home's stairs take a leg; the climb's pickaxes made at a table where the bot stands.
type: reference
tags: [plan, home, overworld, climb, shaft, stairs]
aliases: [arrival, shaft, Arrival, Shaft, arrival_step, arrival_climb, arrival_table, arrival_hand_over, arrival_food_walk, side_cell, SHAFT_BLOCKS, TABLE_REACH, HAND_OVER_FOOD, FOOD_POINT, the shaft, pillar up, step aside]
status: draft
lastUpdated: 2026-11-26
verifiedAgainst: ruststeve@a4695bc
sourceRefs:
  - "ruststeve:crates/plan/src/arrival.rs#pub fn step(w: &World, feet: Cell, opened: Option<Cell>, items: &[ItemKind]) -> Step {"
  - "ruststeve:crates/plan/src/arrival.rs#pub(crate) fn arrival_step(&mut self, w: &World, tick: u64) -> Option<Controls> {"
  - "ruststeve:crates/plan/tests/arrival.rs#fn the_shaft_refuses_a_cave_fluid_or_falling_block() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[way-home]]"
  - "[[stairs-down]]"
  - "[[digging]]"
  - "[[end-crystals]]"
---

# The overworld arrival

Issue 19 (agent Q7, `docs/problems/eyes-arrival.md`). A real bot comes out of its own overworld portal at x 125 to 126, y -52 to -50, z -108, by the lava sea and under the cave band that killed 7 of 16 bots in issue 16 (`docs/problems/overworld-leg.md` wall 1). It holds no food, so it cannot heal; the way home ([[way-home]]) climbs to the open sky, then hunts and cooks.

## The shaft
The way home's stairs in rock climb a block for three digs (race 45727358847: about 120 ticks a block in deepslate, 33 blocks in 12,000 ticks). The shaft climbs a block for one dig and a jump:
- the cell over the head is dug (`Dig`, the best tool), the bot jumps and, at the top of the jump, puts a block into the cell its feet left (the pearls hunt's pillar, `pearls::out_of_hole`);
- each dig gives the block the next step places (`SHAFT_BLOCKS`, cobbled deepslate first);
- below, the shaft is filled by its own blocks; round it is rock: no mob reaches the bot.

`arrival::step` (pure) says what is next from the feet: `Dig` the cell over the head, `Rise` into a cell this shaft opened, `Out` (the sky over the head and a side to walk off by), or `Refused` with why: a fluid in or beside the cell, a falling block in or over it (gravel, sand, dripstone), an open cell beside or over it (a cave) unless the sky is over it (the surface), a break over `MAX_BREAK`. Reading the cells beside a cell about to be opened is the stairs' own no-X-ray exception.

`Plan::arrival_climb`, called from the way home's climb (`home_climb`), begins a shaft where `step` allows one ("arrival: shaft up from"); refused, it notes "arrival: no shaft from" and the stairs take a leg (they turn from caves), and the next climb tries again from where the leg ended. `Plan::arrival_step` drives the shaft a tick at a time ahead of the task choice while no task runs; it ends "arrival: shaft over at" with its reason, rise, digs and blocks placed. A shaft ends for the climb's pickaxe run when the way home's own rule would make pickaxes. `Plan::arrival_rise_to` (issue 20c) begins the same shaft with a stop: it ends "risen to its stop" once the feet stand at the given y (the End kit's way up pillars a drop in its own way).

Before each jump the feet go to the cell's centre (`CENTRE` 0.12, sneaking): off it, the body touches the next column and the jump stops on the rock over it. The rise's two halves are `rise_jump` (centre, the block to hand, the jump) and `rise_click` (the block in at the top of the jump), shared with the End's spike pillars ([[end-crystals]], issue 20f) and unchanged in behaviour. A dig whose drop a full pack cannot take still counts once its cell is open. The junk blocks of the trip (soul soil, nylium, wart blocks, tuff, granite) go under the feet first, so the climb empties the pack. A stack left on the cursor `CURSOR_WAIT` (100) ticks goes to a free slot or swaps out a junk block (never sooner: a hold's swap carries its item on the cursor between clicks).

A refused shaft first steps aside (`side_cell`): a level tunnel of up to `SIDE_REACH` 3 cells, nearest first, every cell on a full floor, rock or open, neither a falling block nor under one, no fluid in, beside or over it, no open cell beside it but the one behind, quick to break, and a shaft allowed from its end; at most `SIDES_MAX` 3 with no rise between ("arrival: shaft steps aside to").

## The climb's table
`base::CLIMB_STEPS` took any table within `TABLE_SEARCH` (64): from y -21 the bot walked back down to the portal room's table at y -53. `Plan::arrival_table` refuses tables farther than `TABLE_REACH` from the eye for the run, so the base places the carried one; when the run is over that table is dug and taken along ("arrival: the table at … taken along").

## The hand-over and the food
On the surface by day (dusk more than `DUSK_MARGIN` off) with food `HAND_OVER_FOOD` (6) or more, `Plan::arrival_hand_over` ends Home at once ("home: over on the surface, the eyes take over"; the coordinator: the overland walk hunts on the way), after digging the block over the head (the night hut's roof) so the first throw has the open sky. Under food 6, hungry by day with no prey in view, `Plan::arrival_food_walk` walks far legs of `FOOD_LEG` (48) toward `FOOD_POINT`, the stronghold's `locate` point (1648, -16), known geography: the ground round the portal was hunted bare by issue 16's bots.

## Tests
`crates/plan/tests/arrival.rs`: the dig over the head in rock; the refusals (a cave beside, lava over, water beside, gravel over, a cave over the head not opened by the shaft); the rise into a cell it opened; out under the sky with a side to walk off by; the climb from under rock begins a shaft and digs; gravel over the head steps aside to rock; on the surface by day with food 6 the eyes take over.
