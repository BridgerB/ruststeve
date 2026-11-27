---
title: The End's arrival and island
description: How the bot goes from the End's arrival platform to the main island (plan::end, issue 20e) - a pillar up to the bridge's height, a bridge west over the void with guards on both sides cut only while the dragon is far or perched, stairs up into a cliff of end stone, a roofed 1x2 bunker dug about 12 from the fountain, a stock of end stone from level tunnels, the look kept down, then the hand-offs to the crystals and the dragon.
type: reference
tags: [plan, end, bridge, bunker, gaze, void]
aliases: [end, EndState, end_task, end_step, end_look, end_gaze, void_edge, void_column, bunker, Bunker, the bridge, BRIDGE_FLOOR, STOCK_WANTED, DRAGON_CLEAR, stock_dir]
status: draft
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@117843e
sourceRefs:
  - "ruststeve:crates/plan/src/end.rs#pub fn end_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/end.rs#pub fn end_step(&mut self, w: &World, tick: u64) -> Option<Controls> {"
  - "ruststeve:crates/plan/src/end.rs#pub fn end_look(w: &World, want: Option<(f32, f32)>) -> (f32, f32) {"
  - "ruststeve:crates/plan/src/end.rs#pub fn void_edge(w: &World, feet: Cell) -> bool {"
  - "ruststeve:crates/plan/tests/end.rs#fn in_the_sealed_bunker_short_of_stone_a_stock_tunnel_begins() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[way-home]]"
  - "[[walk-and-watchdog]]"
  - "[[stairs-down]]"
---

# The End's arrival and island

Issue 20e (agent E3, `docs/problems/end-island.md`). `Plan::end_step` runs ahead of the task choice in the End while no task runs: it drives E3's own steps a tick at a time (the bridge, a work of digs and blocks, an eat) or asks `Plan::end_task` for the next. `end_task` returns ordinary tasks (a walk as `Task::Explore`, stairs as `Task::Stairs`) or sets one of the own steps; it always answers in the End, so the planner's other phases never run there.

## Order
1. **Arrival** ("end: arrived at").
2. **The pillar**: on the platform (obsidian 5x5 at y 48, x 98 to 102, z -2 to 2) the bot rises to the bridge's floor `BRIDGE_FLOOR` 52 (E1's measure: the island ends at x 72 in a lip one block thick at y 52; a bridge at the platform's height runs under it into its side): a jump from the cell's centre and a block into the cell left at the jump's top ("end: pillar up N at").
3. **The bridge** west along z 0: the guards first (`home::guard_next` across the axis, none on the platform's obsidian), then the floor ahead clicked onto the side of the block stood on, then a sneaking step. Cut only while the dragon is more than `DRAGON_CLEAR` 48 off or perched (phase 5 to 7, metadata 16); otherwise it waits crouched at the cell's centre ("end: bridge waits for the dragon", "end: bridge goes on"). Progress invariant: no cell gained in `BRIDGE_STALL` 1,200 ticks (waits not counted) fails it. Over on end stone under the sky ("end: on the island at") or at a cliff of end stone ahead, which is climbed by `Stairs::up` west ("end: cliff at …").
4. **The bunker site**: about `BUNKER_FROM_FOUNTAIN` 12 off the fountain on the bot's bearing (turned up to 60 degrees on failure), a far walk ("end: walk to the bunker site"); dug where the bot stands within 30 after `SITE_WALKS`.
5. **The bunker**: begun only with the dragon over 64 off or perched; a post of two blocks on the ground beside the column, then four digs under the feet, the cell over the new head laid after each (`shaft_ops`, never open to the sky); the door (a three-step stair out) on another side. "end: bunker dug at". Sealed (roof and door laid back) on every return; opened (roof and stair dug) to leave.
6. **Retreat, eat, heal**: under `RETREAT_HEALTH` 14 back to the bunker until `HEALED` 18; food under `FED` 18 with food held is eaten there.
7. **The stock**: every run begins and ends at the bunker: stairs down `STOCK_DEPTH` 6 first (their foot kept), then three level arms from the foot, each run walked out to its tip and back (`stock_step`); until `STOCK_WANTED` 160 end stone or `STOCK_RUNS` 24; a run with no walk back ends the stock. From under the ground with no walk home, stairs up to hand over (`OUT_RUNS`).
8. **The hand-offs**: while a crystal is alive E4's spikes task, then E5's `Plan::dragon_task` (`end_dragon`; the fight waits in `Plan.bunker` while the dragon flies); out by the bunker's stair first. With none ready the bot holds sealed in the bunker. On the bow line (E8, [[end-bow]]) both crystal hand-offs, `end_spikes` and `open_task` before its end stone stock, ask `Plan::bow_task` first; the pillar gets the caged crystals and any the bow leaves.

## Gaze
Every look still leaves through `keep_look` ([[way-home]]'s endermen rules). In the End a look no act asked for is straight down (`end_gaze`, ahead of `keep_look` in `Plan::step`); `end_look` sends a wanted look only when it stares at no enderman within 64 from the standing or the crouching eye.

## The first look
The server judges a stare by the look it holds; after the teleport into the End that is pitch 0 until a look packet arrives. `world::World::move_body` sends a look the controls ask for even with no column loaded under the feet (it sent only the status before), and `end_gaze` asks for pitch 90 from the first End tick.

## The void
`void_column`: a cell and everything under it to the dimension's floor loaded and clear. `void_edge` is the walk's edge in the End ([[walk-and-watchdog]]); `home::rail_wanted` and the idle sneak count it too.

## Covered
In the End a roofed 2-high cell with 2-high roofed sides (`covered`: the bunker, its tunnels) needs no pocket from an angry enderman: `Plan::ender_threat` returns at once there (`end_covered`, noted every 600 ticks), and the stock goes on (race 45869900923: endermen hurt near the dragon turned angry and held both bots in pockets in their own tunnels).

## Gotchas
- Cobblestone and netherrack are not dragon-immune (E1): the dragon breaks bridge blocks it touches. End stone, obsidian, iron bars and bedrock are.
- The caged spike at (42, 0) sits on the bridge's line 30 into the island: the site walk paths round it.
