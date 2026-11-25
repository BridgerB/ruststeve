---
title: The forest base
description: How the bot makes the hunt's kit in a warped forest before the first provoke (issue 18d) - warped stems dug by hand, a table, the raw iron smelted with coal only, swords counted in kills, pickaxe uses, a shield - each step bounded and noted, through the goal table's base rows.
type: reference
tags: [plan, base, forest, pearls, crafting]
aliases: [fungus cap, on_cap, dig_down, base_task, base_done, forest base, forest kit, base rows, weapon kills, plan::base, Aim::Base, --goal base]
status: verified
lastUpdated: 2026-11-24
verifiedAgainst: ruststeve@e927314
sourceRefs:
  - "ruststeve:crates/plan/src/base.rs#pub fn base_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/base.rs#pub fn weapon_kills(w: &World, base: &Base) -> i32 {"
  - "ruststeve:crates/plan/src/base.rs#pub fn dig_down(w: &World) -> Option<Cell> {"
  - "ruststeve:crates/plan/src/base.rs#pub fn skip_why(step: Step, w: &World, base: &Base, table_near: bool) -> Option<String> {"
  - "ruststeve:config/goals.toml#id = \"base-shield\""
  - "ruststeve:crates/bot/tests/base.rs#fn the_frontier_kit_makes_an_iron_sword_and_a_shield_in_the_forest() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[goal-table-and-needs]]"
  - "[[fuel-and-smelt-needs]]"
  - "[[mc-endermen]]"
---

# The forest base

Issue 18d (`docs/problems/pearls-base.md`). The frontier bot reaches a warped forest with no table and no wood, an iron sword of 161 uses (23 kills at 7 hits), 144 pickaxe uses, a furnace, 7 raw iron and one coal. `plan::base` makes what the hunt needs where it stands, before the first provoke.

## Key files
- `crates/plan/src/base.rs`: `Plan::base_task` (the next task, `None` once every step is over), `base_done(w)` (the kit held by the config's numbers), `weapon_kills`, `stem_targets`, `skip_why`, `drop_slot`, `bare_hand`.
- `config/goals.toml`: the `base-*` rows and `[base]` (read into `make::Table::base`).
- `crates/plan/tests/base.rs` (hand-built worlds, the real table); `crates/bot/tests/base.rs` (the model's forest, closed loop, `Aim::Base`).

## How it works
- **Steps**, in order: wood, table, smelt, iron sword, stone swords, pickaxe, shield. Each is skipped when what it makes is held (`skip_why`: 8 logs' worth of wood, a table near and 4 sticks, no raw iron or no coal, swords worth `kills` 30, `pickaxe_uses` 100, a shield), else begun with a note of the kit, and ends `done` or `gave up` within `step_ticks` 2,400 and `step_fails` 4 failed tasks (the planner's failure count rising). Every end is a ledger note: "base: <step> begun / done / skipped / gave up", and "base over: the kit is held / short".
- **Wood**: the nearest stem (the `#logs` tag holds warped stems) seen exposed within 48 flat, each column taken at its lowest seen cell within 12 of the feet's height, walked beside and dug by hand (the gather's walk and dig). A sword or pickaxe in hand is swapped out first: a sword digging a block spends 2 uses.
- **Off a fungus cap** (race 45537373222: two bots stood on warped wart at y 51 to 52 for 2,400 ticks): standing on wart, shroomlight or a stem (`on_cap`), the bot first walks by route to seen warped nylium 8 to 24 off with room over it (`floor_targets`), else digs the cap block underfoot by hand when it comes to rest within 3 below (`dig_down`), at most 24 moves, each noted "base: down off the fungus cap".
- **Rows**: the table, sword, pickaxe and shield steps ask the needs planner for their `base-*` row; the repeated rows make one more of their first provide than was held when the run began, and the step runs again while it wants (stone swords until 30 kills, pickaxes until 100 uses; an iron pickaxe only with 3 ingots over the shield's). The act is done where the bot stands: a craft (at the table near for 3x3), a placing, or a stem; a smelt, mine or trip gives the step up.
- **Smelt**: only with coal or charcoal held (warped planks have no burn time; sticks and rods are never this step's fuel), as much raw iron as the coal cooks (1,600 / 200 = 8 a coal), in the held furnace placed where the bot stands.
- **Swords in kills**: uses left over hits a kill (`hits_per_kill`: iron 7, stone 8, from 40 health over 6 and 5), a sword under 10 uses counting for nothing.
- **Never** with an angry enderman (`endermen::angry`): the base waits, noted. Under 4 free main slots a stack of `drop` goes (granite, tuff, dirt first), never pearls, rods, powder or eyes. Every look leaves through `Plan::keep_look`.
- **The hunt** calls `base_task` in its `base` row; `Aim::Base` (`--goal base`) runs it alone for development and tests.

## Gotchas
- `base_done(w)` reads `Base::default()`; a test holds it equal to the config's `[base]`.
- No mushroom stew: the warped forest places no nether mushrooms (P1's jar read, 2026-11-24).

## See also
- [[goal-table-and-needs]] - the needs planner the rows go through.
- [[fuel-and-smelt-needs]] - why only coal burns here.
