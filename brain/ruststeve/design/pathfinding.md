---
title: Pathfinding
description: How ruststeve finds a route over the block grid - a port of clojurecraft's path.clj (cells classified, moves and goals as data, A* budgeted in node expansions), checked route for route against clojurecraft on captured terrain.
type: reference
tags: [plan, path, astar, routes, core]
aliases: [path, pathfinder, A*, route, waypoints, Cells, classify, awkward, moves, goals, Near, Afar, budget, expansions, start_cell]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@a5a7aa4 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/path.rs#pub fn classify(state: Option<u32>) -> Class {"
  - "ruststeve:crates/plan/src/path.rs#pub fn search(cells: &mut Cells<'_>, start: Cell, goal: &Goal, max_nodes: u32) -> Route {"
  - "ruststeve:crates/plan/src/path.rs#pub fn plan(world: &World, from: Cell, goal: &Goal, max_nodes: u32) -> Route {"
  - "ruststeve:crates/plan/tests/differential.rs#fn every_route_equals_clojurecraft() {"
  - "clojurecraft:src/clojurecraft/path.clj#Liquids are walls, never floors (the physics has no fluid model), lava is a wall and no cell"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[walk-and-watchdog]]"
  - "[[planner]]"
  - "[[cc-stall-and-budgets]]"
---

# Pathfinding

Issue 09a: clojurecraft's `path.clj` in Rust (`plan::path`), constant for constant, pure and deterministic.

## How it works
- **Cells** (`classify`, `Cells`): water, lava, awkward, solid, clear, unknown. Awkward is clojurecraft's list of block definition types the physics cannot simulate or that hurt (slabs, fences, doors, ice, webs, soul sand ...), plus any partial shape taller than a carpet; stairs, end portal frames and the end portal are stood on; one layer of snow is walked through; fire is awkward. The definition type comes from the blocks report through datagen (`BlockInfo::definition`, [[datagen]]). Ice is floor when the bot stands on a sheet. `clear` also means no lava beside or under. Cells are read once per question and held.
- **Moves are data**: walk (1.0), jump up (2.0), drop (1.0 + 0.5 a block, at most 3, landing only on a floor), diagonal (√2, both corners clear), each in four rotations, in clojurecraft's order.
- **Goals are data**: `Near`, `Block`, `Xz`, `Away`, `Afar`, and since issue 10c `Pickup` (a neighbouring column with the block at the feet's height or up to `rise` above, clojurecraft's `:pickup`, [[digging]]), each a done-test and a heuristic (octile, flat distance).
- **The search**: A* over an open set ordered (f, g, node), so ties break on the node and every machine gets the same route. **Budgeted in node expansions, never time** (`Budget`: near 400, reach 300, far 6,000). Out of budget or frontier it returns `Partial` or `None` with the route to the node nearest by the heuristic: a walker follows that and plans again as chunks arrive.
- `plan` starts from the feet cell, or the standable cell the body's box rests on (`start_cell`, `rests`), and may leave a supported, clear feet cell directly when that reaches the goal.

## Evidence
- `every_route_equals_clojurecraft`: 17 queries over two landings captured on the race seed (5×5 columns each: plains, and a sand island in water) give the same status, the same cost bit for bit, the same start and every waypoint as clojurecraft's own `path/plan` (found, partial and none among them).

## Gotchas
- Traps and the Nether's wary blocks (crimson forest) are not ported yet: they come with the issues that meet them.
- A 6,000-node search costs tens of milliseconds; the planner asks at most a few per tick (the CPU budget, [[planner]]).

## Issue 16: the Nether walk (docs/problems/nether-walk.md)
- **Soul sand is ground**: `classify` answers `Solid` for it (a box 0.875 high the physics stands on). Feet resting on it floor to the soul sand's own cell, so `plan` searches from the cell over it and the walk reads its node with `stand_cell`; `feet_cell` is unchanged for every other task (a global lift of the feet's cell broke the tunnel's stairs on soul sand and was taken out). Feet at a soul sand top (y + 0.875) stand in the cell over it whatever is under the centre's column (a body over the edge of a dug cell rests on the soul sand beside: race 45777976793's dig-in aimed at air 336 times). Magma stays `Awkward`.
- **The bridge move** (`MoveKind::Bridge`, a far search's alone (`plan_far`, `Cells::far`), Nether only, with `BRIDGE_RESERVE` of the `BRIDGE_BLOCKS` held): one cell on the level whose floor is air or lava, laid first by the walk. Its cost is a walk plus `BRIDGE_COST` 3. Its feet cell may have lava under (`clear_over`), never beside. The overworld is not bridged and water is never laid in, but for an overland leg's river (issue 19e, below). Basalt and blackstone are bridge blocks too (issue 19d: the deltas' own walls, restocked by the way home).
- **A far search keeps clear of mobs that kill on sight** (`SHUN_KINDS`: piglins, brutes, hoglins, and from issue 18c wither skeletons): no cell within `SHUN_RADIUS` 24 of one (20 through issue 17; the travel to the warped forest is at no-heal health), none within `SHUN_MAGMA` 8 of a magma cube and none within `SHUN_SKELETON` 16 of a skeleton (`path::shun_radius`); the travel's plain walks shun too (`Walk.more.shun`), except within `SHUN_FREE` 6 of the start. Two piglins killed a bot in iron in 100 ticks on its first bridged route (race 45337580375). Magma blocks are awkward, never a floor.
- **The bridge block held** (`path::bridge_item`): the bridge block in hand, else the stack on the cursor, else the kind with the most held; `bridge_blocks` counts the cursor's stack. Chosen afresh by count alone, a stack lifted for the swap stopped counting and the swap undid itself for 3,500 ticks 35 blocks from the warped forest (race 45531577912 lane 1).
- **`Goal::Plane`** (issue 18c): within a range on the plane at any height; a travel leg toward a far forest ends where the ground there is.
- **A route keeps its moves** (`Route::kinds`, one a waypoint).
- **The weighed search** (`search_weighted`, `plan_weighted`): f = g + weight × h. At 1 it is `search`. The fortress leg uses 4 with 40,000 expansions (`walk::FAR_WEIGHT`, `FAR_NODES`): on the recorded Nether of race 45194253932 the 222-cell route to bricks 110 blocks off is found in 12,664 expansions, where weight 2 needs 147,900 and 6,000 plain ones find nothing.
- **`navmap`** (crate `navprobe`) folds a recording to a tick and prints the terrain as layer maps and the route `plan_weighted` finds over it: the probe to run before a race.

## Issue 17d: the heading's legs (docs/problems/heading.md)
- **`Goal::Ahead { from, dir, gain }`**: `gain` or more along a compass axis from `from` (`path::along` gives a cell's projection and how far aside), no more than `gain` aside. A heading's leg ends wherever the ground lets it, not at one cell that may be rock.
- **Blocked cells** (`Cells::blocking`): a search never enters them. The heading gives its cut cells at or behind the feet, so a leg never walks back over ground it covered (clojurecraft's causeway pacing).
- **Bridges over water** (issue 19e, `Cells::far_over_water`, `walk::More::water`): an overland leg's far search may lay held blocks over water (a river) in the overworld, with its keep; every other search still never lays into water.
- **A keep for bridging** (`Cells::far_keeping`): a far search bridges only with that many blocks held; the heading keeps 32, the fortress walk `BRIDGE_RESERVE` 16. `plan_over` searches cells the caller made.

## Issue 56: `Goal::Reach`
- **`Goal::Reach { pos, range, rise }`**: `Near`, and `pos` at most `rise` over the feet (`path::reach_rise`, which the reach walk's arrival reads too). A walk to within reach of a block routes to it, so its route never ends under a block too high to arrive at ([[walk-and-watchdog]]).

## See also
- [[walk-and-watchdog]] - following a route.
- [[cc-stall-and-budgets]] - the budgets' source.
