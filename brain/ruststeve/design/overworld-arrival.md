---
title: The overworld arrival
description: How the bot climbs from its own overworld portal by the lava sea (feet y -53) to the open sky on the way home (plan::arrival, issue 19) - a one-wide shaft straight up in rock, a dig and a jump a block, sealed below by its own blocks; refused at caves, fluids and falling blocks, where the way home's stairs take a leg; the climb's pickaxes made at a table where the bot stands.
type: reference
tags: [plan, home, overworld, climb, shaft, stairs]
aliases: [arrival, shaft, Arrival, Shaft, arrival_step, arrival_climb, arrival_table, SHAFT_BLOCKS, TABLE_REACH, the shaft, pillar up]
status: draft
lastUpdated: 2026-11-25
verifiedAgainst: ruststeve@4fe8848
sourceRefs:
  - "ruststeve:crates/plan/src/arrival.rs#pub fn step(w: &World, feet: Cell, opened: Option<Cell>, items: &[ItemKind]) -> Step {"
  - "ruststeve:crates/plan/src/arrival.rs#pub(crate) fn arrival_step(&mut self, w: &World, tick: u64) -> Option<Controls> {"
  - "ruststeve:crates/plan/tests/arrival.rs#fn the_shaft_refuses_a_cave_fluid_or_falling_block() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[way-home]]"
  - "[[stairs-down]]"
  - "[[digging]]"
---

# The overworld arrival

Issue 19 (agent Q7, `docs/problems/eyes-arrival.md`). A real bot comes out of its own overworld portal at x 125 to 126, y -52 to -50, z -108, by the lava sea and under the cave band that killed 7 of 16 bots in issue 16 (`docs/problems/overworld-leg.md` wall 1). It holds no food, so it cannot heal; the way home ([[way-home]]) climbs to the open sky, then hunts and cooks.

## The shaft
The way home's stairs in rock climb a block for three digs (race 45727358847: about 120 ticks a block in deepslate, 33 blocks in 12,000 ticks). The shaft climbs a block for one dig and a jump:
- the cell over the head is dug (`Dig`, the best tool), the bot jumps and, at the top of the jump, puts a block into the cell its feet left (the pearls hunt's pillar, `pearls::out_of_hole`);
- each dig gives the block the next step places (`SHAFT_BLOCKS`, cobbled deepslate first);
- below, the shaft is filled by its own blocks; round it is rock: no mob reaches the bot.

`arrival::step` (pure) says what is next from the feet: `Dig` the cell over the head, `Rise` into a cell this shaft opened, `Out` (the sky over the head and a side to walk off by), or `Refused` with why: a fluid in or beside the cell, a falling block in or over it (gravel, sand, dripstone), an open cell beside or over it (a cave) unless the sky is over it (the surface), a break over `MAX_BREAK`. Reading the cells beside a cell about to be opened is the stairs' own no-X-ray exception.

`Plan::arrival_climb`, called from the way home's climb (`home_climb`), begins a shaft where `step` allows one ("arrival: shaft up from"); refused, it notes "arrival: no shaft from" and the stairs take a leg (they turn from caves), and the next climb tries again from where the leg ended. `Plan::arrival_step` drives the shaft a tick at a time ahead of the task choice while no task runs; it ends "arrival: shaft over at" with its reason, rise, digs and blocks placed. A shaft ends for the climb's pickaxe run when the way home's own rule would make pickaxes.

## The climb's table
`base::CLIMB_STEPS` took any table within `TABLE_SEARCH` (64): from y -21 the bot walked back down to the portal room's table at y -53. `Plan::arrival_table` refuses tables farther than `TABLE_REACH` from the eye for the run, so the base places the carried one; when the run is over that table is dug and taken along ("arrival: the table at … taken along").

## Tests
`crates/plan/tests/arrival.rs`: the dig over the head in rock; the refusals (a cave beside, lava over, water beside, gravel over, a cave over the head not opened by the shaft); the rise into a cell it opened; out under the sky with a side to walk off by; the climb from under rock begins a shaft and digs.
