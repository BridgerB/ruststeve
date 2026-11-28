---
title: The End kit and the portal
description: How the bot goes from the stronghold's portal room to the End (plan::end_portal, issue 20c and 20d) - up its own stairs to wood, the end-kit row (stone pickaxes, the shears, an iron sword and boots), the meat cooked and eaten, a carved pumpkin, back down wearing it; then the spawner, an eye into each empty frame from the floor outside the ring, and the step in.
type: reference
tags: [plan, overworld, stronghold, end, portal, kit]
aliases: [end_portal_task, end kit, end-kit row, frame_has_eye, empty_frames, eye_stands, KitPhase, armed, carved pumpkin, pumpkin phase, helmet_off]
status: draft
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@95bb348
sourceRefs:
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn end_portal_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn frame_has_eye(state: u32) -> Option<bool> {"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn eye_stands(w: &World, room: &Room, frame: [i32; 3], from: Cell, reach: f64) -> Vec<Cell> {"
  - "ruststeve:config/goals.toml#id = \"end-kit\""
  - "ruststeve:config/goals.toml#way_up = [1608, 63, -32]"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn entries(w: &World, room: &Room) -> Vec<Entry> {"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn climb_route(from: Cell, to: Cell) -> Vec<Cell> {"
  - "ruststeve:crates/plan/src/end_portal.rs#fn end_kit_pumpkin("
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn pumpkin_head(&mut self, w: &World, tick: u64) -> bool {"
  - "ruststeve:crates/plan/src/lib.rs#pub fn unworn_armour_kept("
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[overland-and-stronghold]]"
  - "[[goal-table-and-needs]]"
  - "[[way-home]]"
---

# The End kit and the portal

Issue 20 (#25), parts c and d: from the frontier bot on the portal room's floor (no pickaxe, no wood, 30 raw meat, 7 raw iron, 13 eyes) to the End. The walls are in `docs/problems/end-portal.md`.

## Key files
- `crates/plan/src/end_portal.rs`: `Plan::end_portal_task`, called first in `nether_task` (fenced `E2`), in the overworld under the race once the stage `portal_room` is reached.
- `config/goals.toml`: the `end-kit` goal row and `[end_kit]` (moving to E1's `config/end.toml`).

## The kit (`KitPhase`)
Each phase is noted as it ends ("end kit: <phase> over, <next> next" with the kit held) and ends within `phase_ticks` or `phase_fails` failed tasks.
1. **Up**: the bot's own way, a point at a time (`[end_kit] way`, its legs' ends from the room up, then `way_up`, the stairs' top), from the point after the nearest, each noted reached or skipped. With no walk to a point: the route's reach under it walked to (only when nearer the point); right under it a pillar to its height on held blocks (`arrival_rise_to`, Q7's shaft with a stop, never ended for the pickaxes); from a way point, the old leg back up cell by cell (`climb_route` with `Stairs::along`: level first, then a step up a block, the reverse of a downward leg, which leaves no head room to walk up); else a shaft by hand to the sky. With the points spent, the dry stairs up.
2. **Wood**: a log seen within `log_reach` walked to and dug by hand (`Task::ToLog`, the hand emptied of a wearing tool first) to `logs`; with none in reach, the overland's legs (`overland_task`, run with the frames on record while `wood_legs` is set) toward the nearest log on record, else to each of `wood_points` in turn.
3. **Row**: the needs planner on `end-kit` where the bot stands: crafts, a table and the carried furnace placed, the whole smelt's fuel crafted into planks first, the raw iron smelted; boots are put on by `equip`.
4. **Cook**: the row's table dug first (at hand, the cook's planner presses cobblestone into furnaces), then the way home's cook (`home_cook`: a hut by night, its fuel fetched by day).
5. **Eat** to `fed`.
5b. **Bow** (E8's bow line, `[end_kit] bow`; [[end-bow]]): by day the `bow-kit` row built from what is held (a bow from 3 string, arrows from the feathers held with flint redug from the gravel, a 4th stone pickaxe, 16 sticks) through the same row step (`kit_row_step`); then the nearest chicken (feathers) or spider (string) seen within `bow_radius` hunted (`Task::Hunt`, which picks up feathers and string; any lying within 24 walked to first); with none, the overland's legs to `bow_points` in turn (eight look points 60 and 100 out round the stairs' top; `wood_points` when empty). Its own clock `bow_ticks` (18,000: the night and the morning, so the walk back is by day) and `bow_fails`; notes "end kit: hunt", "end kit: bow row over", "end kit: legs toward bow look point". In the portal room with no bow (`bow_room_task`): cobwebs within 40 with a face open to the air walked to and dug for string, one with no walk dug toward (48 tries at most, each refusal noted), then the bow alone made (the `bow-room` row, no trip for wood), and the way down runs again to the room; "end kit: cobweb at", "end kit: the room's bow over". With no frame in the search and over 48 off the stairs' top, the way down first takes the overland's legs back to it.
5c. **Pumpkin** (issue 80, `[end_kit] pumpkin`, `docs/problems/pumpkin.md`): a pumpkin on record (memory watches pumpkins; a sighting needs an exposed face) within `pumpkin_radius` (150) flat and `pumpkin_rise` of the stairs' top is gone for ("pumpkin seen at", once each), walked to (`pumpkin_walks`), and the shears used on its open face nearest the eye within `frame_reach` (`Act::UseOn`, `pumpkin_tries` each `eye_every` apart): the block turns carved in place ("pumpkin carved at"), then is dug by `Task::ToLog` (hardness 1, an axe best, it drops itself) and picked up ("carved pumpkin picked up" ends the phase). The shears come from the iron held (the `pumpkin-shears` row), never a trip: the end-kit row makes them ahead of the iron boots (`shears_first`; the armed pack's 7 raw iron leave 1 after the sword and boots), and a bot under 2 iron gives up ("pumpkin given up: ..."). With none on record, the overland's legs to `pumpkin_points` (else `bow_points`); its own clock `pumpkin_ticks` and `pumpkin_fails`. Patches are rare (1 chunk in 300, probe 46159976076), so the kit goes on without one.
6. **Down**: out of the night's hut, then a found walk to the room's standing cell (`stronghold::stand_target`), else its own way down a point at a time (`end_way_down`: the overland's legs back when far, a walk to each point, the pillar it left dug down to the point under the feet); then `stronghold_task`.

## The head slot (issue 80)
From the way down (`KitPhase::Down` or `Over` in the overworld, and in the End: `Plan::pumpkin_leg`; never before `armed`, the pearl hunt stares on purpose) a held carved pumpkin goes on: the iron helmet shift-clicked out of slot 5 first, then the pumpkin shift-clicked in (its quick move fills slot 5 only when empty, probe 46159976076), on `equip`'s clock (`Plan::pumpkin_head`), "pumpkin worn (helmet held)" noted once. The one rule `Plan::helmet_off` keeps the helmet in the pack: `equip` and the Nether's armour branch (`unworn_armour_kept`) never put it back over the pumpkin's place. `[end_kit] dragon_helmet` (false) would take the pumpkin off in `Task::Dragon` for the helmet's 2 armour points; the numbers against it are in the config's comment. The kit line carries the carved pumpkins, the shears and the head.

## The portal
- `frame_has_eye` reads the frame's `eye` property from the block report (state 9469 to 9472 hold one).
- The spawner is dug from a cell not over lava; an eye goes into each empty frame by `Act::UseOn` on its top face from a feet cell outside the 5x5 ring with no lava within one, the eye within `frame_reach` of the top (`eye_stands`). The state is read again after each.
- **The step in** (`entries`, `Entry`): the frames float two over the floor, so the bot lays two steps outward of a side's frame (`high`, two blocks, its top level with the frame's foot; `low`, one block, beyond it), clicked onto the floor's top from a start beside `low`, then a forced walk (`Task::Enter`) up both, over the frame's top, into the portal cell and on toward the cell past it (`Entry::deeper`: a body whose box still rests on the frame's edge at 19.81 is over the portal's 0.75 top and never goes through); done in the End. A way whose start has no walk, whose blocks will not go in, or whose steps in fail is given up for the next frame's.

## Limits
One world: the stairs' top and the room are the frontier's.
