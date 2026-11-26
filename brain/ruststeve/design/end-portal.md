---
title: The End kit and the portal
description: How the bot goes from the stronghold's portal room to the End (plan::end_portal, issue 20c and 20d) - up its own stairs to wood, the end-kit row (stone pickaxes, an iron sword and boots), the meat cooked and eaten, back down; then the spawner, an eye into each empty frame from the floor outside the ring, and the step in.
type: reference
tags: [plan, overworld, stronghold, end, portal, kit]
aliases: [end_portal_task, end kit, end-kit row, frame_has_eye, empty_frames, eye_stands, KitPhase, armed]
status: draft
lastUpdated: 2026-11-26
verifiedAgainst: ruststeve@34ec399
sourceRefs:
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn end_portal_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn frame_has_eye(state: u32) -> Option<bool> {"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn eye_stands(w: &World, room: &Room, frame: [i32; 3], from: Cell, reach: f64) -> Vec<Cell> {"
  - "ruststeve:config/goals.toml#id = \"end-kit\""
  - "ruststeve:config/goals.toml#way_up = [1608, 63, -32]"
  - "ruststeve:crates/plan/src/end_portal.rs#pub fn entries(w: &World, room: &Room) -> Vec<Entry> {"
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
1. **Up**: a walk to `way_up`, the top of the bot's own stairs (cells it dug, sent by the server as air); with no route, the dry stairs up (`stairs_to_surface`).
2. **Wood**: logs seen within `log_reach` walked to and dug by hand (`Task::ToLog`, the hand emptied of a wearing tool first) to `logs`; with none, explore walks.
3. **Row**: the needs planner on `end-kit` where the bot stands: crafts, a table and the carried furnace placed, the raw iron smelted with planks; boots are put on by `equip`.
4. **Cook**: the way home's cook (`home_cook`: a hut by night, its fuel fetched by day).
5. **Eat** to `fed`.
6. **Down**: a walk to the room's standing cell (`stronghold::stand_target`), out of the night's hut first; with no route, `stronghold_task`.

## The portal
- `frame_has_eye` reads the frame's `eye` property from the block report (state 9469 to 9472 hold one).
- The spawner is dug from a cell not over lava; an eye goes into each empty frame by `Act::UseOn` on its top face from a feet cell outside the 5x5 ring with no lava within one, the eye within `frame_reach` of the top (`eye_stands`). The state is read again after each.
- **The step in** (`entries`, `Entry`): the frames float two over the floor, so the bot lays two steps outward of a side's frame (`high`, two blocks, its top level with the frame's foot; `low`, one block, beyond it), clicked onto the floor's top from a start beside `low`, then a forced walk (`Task::Enter`) up both, over the frame's top and down into the portal cell; done in the End. A way whose start has no walk, whose blocks will not go in, or whose steps in fail is given up for the next frame's.

## Limits
One world: the stairs' top and the room are the frontier's.
