---
title: Overland and the stronghold
description: How the bot walks the overworld to the eyes' fix and digs into the stronghold's portal room (plan::overland and plan::stronghold, issue 19e and 19f) - legs to seen ground with river bridges and huts at night, then dry stairs out of the room's footprint and a floor-level tunnel in, never opening an infested block.
type: reference
tags: [plan, overworld, stronghold, travel, stairs, tunnel]
aliases: [overland, overland_task, stronghold_task, Room, room_of, stand_target, portal room, infested]
status: draft
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4
sourceRefs:
  - "ruststeve:crates/plan/src/overland.rs#pub fn overland_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/overland.rs#pub fn top(w: &World, x: i32, z: i32, near_y: i32) -> Option<Top> {"
  - "ruststeve:crates/plan/src/stronghold.rs#pub fn stronghold_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/stronghold.rs#pub fn ring_centre(frames: &[[i32; 3]]) -> Option<[i32; 3]> {"
  - "ruststeve:crates/plan/src/stronghold.rs#pub fn stand_target(w: &World, room: &Room, from: Cell, reach: i32) -> Option<Cell> {"
  - "ruststeve:crates/plan/src/stairs.rs#pub fn infested(k: Option<BlockKind>) -> bool {"
  - "ruststeve:crates/plan/src/path.rs#pub fn far_over_water(world: &'a World, keep: i64) -> Self {"
  - "ruststeve:config/goals.toml#frame_over_feet = 2"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[stairs-down]]"
  - "[[pathfinding]]"
---

# Overland and the stronghold

Issue 19 (#24), parts e and f: from the overworld surface with the eyes' fix in hand to standing in the portal room. The walls are in `docs/problems/eyes-stronghold.md`.

## Overland (`crates/plan/src/overland.rs`)
- `OverlandState.fix` is set by the caller from the eyes' `Plan.fix` (the point and the larger of its two errors). With no fix yet it is the next throw's walk point (`Plan::eyes_way`, `way` set): its arrival starts no search. `retarget` drops the leg, the mark and the arrival.
- **Order** (`Plan::eyes_stronghold_task`): stone pickaxes, the stronghold (frames seen, or the search after arrival), the eyes' throw due here (`eyes_task`), the overland; eyes held in the overworld with none of it now is `Task::Idle`, never the Nether task's portal.
- **Legs** of 64 to 128 blocks on the bearing to the fix (plus a held turn), each to a cell on seen ground: `top` reads the first block from the top of a loaded column that is not air, a plant, leaves or a log; a full block with two open cells over it is ground. Sky-facing faces are exposed: no X-ray.
- **Rivers** are crossed by the far walk's bridge move over water (`path::Cells::far_over_water`, `walk::More::water`), 32 blocks kept back, 40 laid at most a leg.
- **Oceans** (64 or more of water on the bearing): the bearing turns 60 to the side with more land seen. `around` in `[overland]` holds travel points round a known ocean (Bridger's geography ruling).
- **Invariants**: a leg no nearer its target in 600 ticks, or three walks begun, turns the bearing 30 (alternating sides, widening); the fix 32 nearer every 1,200 walking ticks, else a turn; a done leg eases the turn back 30.
- **Huts**: under `DUSK_MARGIN` to dusk, or under the danger line with a way to heal, a hut (`cover::hut_block`) and a wait; a held furnace is put down first when raw meat is held, and the meat is cooked inside (`cook_held`); the wall on the fix's side is dug to leave. In a hut nothing is dropped (the junk list is empty), and the hut is built of `HUT_JUNK` (dirt, andesite, basalts, soul soil and the like) first.
- **Stuck legs**: after `TUNNEL_AFTER` 2 refused leg walks in a row the leg goes by `tunnel_toward` (race 45747459284 lane 0, in a hollow 17,000 ticks); a far walk that dies within 8 expansions digs a wall toward the leg, else any side (race 45725479883: a hut wider than its exit).
- **A throw's walk point** is reached within `leg_reach`; after `WAY_WAIT` 600 at the spot with no fix the spot moves 20 east, south, west, north in turn, 4 times at most.
- **The End kit's wood** (issue 20c, [[end-portal]]): with frames seen and no log in reach, the overland's legs go to the nearest log on record or `[end_kit] wood_points`.
- Ledger: "overland leg N: X blocks to the fix (err E), health H, food F, eyes K", "overland turned", "overland ocean ahead", "overland hut N (night|hurt)", "overland arrived at the fix", "overland over: frames seen".

## The stronghold (`crates/plan/src/stronghold.rs`)
- `room_of` (pure): the ring's centre is the one cell every frame seen is a ring member of (three a side, two out), else the middle of the frames' bounds; the feet's floor is two under the frames (the `PortalRoom` piece: frames at y 3, floor at y 0).
- `stand_target`: a standing cell inside the `portal_room` stage's own box (`world::stage::ROOM_HALF`, `ROOM_BELOW`, `ROOM_ABOVE`, so the stage cannot miss it by rounding), nearest the bot, out of the ring, with no lava within two at the feet's and the floor's height (the pool under the ring, the side pools by the door).
- **Way in**: a far walk on the surface toward the room first while it is more than the floor's depth, `approach` and 8 off (`stronghold_surface`, 6 walks at most); then dry stairs (`Stairs::dry`) down, toward the target while they end 10 or more off the centre, else away from it; a leg from the same cell turns a quarter, four end the stairs. At the floor's height the travel tunnel (`tunnel_toward`) breaks in; a plain walk only within 24 at that height. Within `STAIR_LEG` 6 of the floor's height `at_floor` latches and the tunnel has the way, stairs only its fallback; at the tunnel's end near the standing cell, a walk (a drop in) or a wall step.
- **Infested blocks** are never opened: `stairs::refusal` refuses them and the tunnel's route reads them as walls.
- **Inside**: the spawner broken, the standing cell walked to, a nook sealed; then the wait (issue 20 starts there).
- **No frame at the fix**: dry stairs down to `search_y` once (`descended` latches: a spiral leg that climbs never starts the stairs again), then a square spiral of tunnel legs, toward stone bricks seen exposed first. Where the tunnel ends short of a brick (its reach is 5 to the cell, a dig's 4 from the eye) a wall step digs the head and feet cells toward it and walks in; in reach the brick is broken, `wall_breaks` at most (race 45689919380 lane 0 stood at the wall and turned away).
- **Pickaxes** (`stronghold_pickaxes`): between tasks, outside the room, fewer than `spare_pickaxes` stone pickaxes with `spare_uses` left are made from the stairs' cobblestone, the pack's sticks and its table (placed beside the feet, dug again after), `pick_tasks` a round. Ledger: "stronghold pickaxes", "stronghold pickaxes held", "stronghold pickaxes not made", "stronghold wall step", "stronghold wall broken", "stronghold search at depth".
- **Junk churn** (`junk_watch`, `junk_now`, `junk_place`): a junk kind back by 4 or more within 200 ticks of its drop is held back for `JUNK_BACK_TICKS` (2,400). It is not dropped again; a kind with no block form may still go while a walk is the task. With no main slot free, a held-back block is put down beside the feet, never dropped, only from a stack of `JUNK_PLACE_MAX` 8 or under and at most 8 placings a hold; then the kind is released and the drop takes the stack whole (races 45719263137, 45732530352). Ledger: "junk held back", "junk placed".
- **The capped tunnel** (`Plan::tunnel_toward_capped`, `tunnel::Limits::max_y`): the room's route is kept within `STAIR_LEG` over the floor. `SAFE_Y`'s cost under y 40 is the Nether's lava sea, and in the overworld it took routes to y 40 under a river and into its water. With no capped way, the approach turns to the room's north or south side (`SIDE_OFF`), twice (`CAP_TRIES`); then the uncapped route, noted "stronghold tunnel uncapped".
