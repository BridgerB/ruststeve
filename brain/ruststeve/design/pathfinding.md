---
title: Pathfinding
description: How ruststeve finds a route over the block grid - a port of clojurecraft's path.clj (cells classified, moves and goals as data, A* budgeted in node expansions), checked route for route against clojurecraft on captured terrain.
type: reference
tags: [plan, path, astar, routes, core]
aliases: [path, pathfinder, A*, route, waypoints, Cells, classify, awkward, moves, goals, Near, Afar, budget, expansions, start_cell]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@8df1710 clojurecraft@be85a92
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

## See also
- [[walk-and-watchdog]] - following a route.
- [[cc-stall-and-budgets]] - the budgets' source.
