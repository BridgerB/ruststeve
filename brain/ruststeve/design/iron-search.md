---
title: The iron search
description: What an iron mine row does at its depth with no exposed iron in sight (plan::iron) - rise out of deepslate to the band, walk into or dig toward a cave seen beyond exposed_near's reach, cut strips only into fresh rock and turn them on their own progress, and never walk its own tunnel again; within strict no X-ray.
type: reference
tags: [plan, mine, iron, strip, cave, progress-invariant, no-xray]
aliases: [iron search, IronSearch, IronRules, strip, fresh rock, fresh_heading, cave_near, cave_cell, rise toward iron's band, strip turned, no fresh rock on any heading]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@201dfcf
sourceRefs:
  - "ruststeve:crates/plan/src/iron.rs#pub fn fresh(w: &World, feet: Cell, dir: (i32, i32), ahead: i32) -> bool {"
  - "ruststeve:crates/plan/src/iron.rs#pub fn cave_cell(w: &World, (x, y, z): Cell) -> bool {"
  - "ruststeve:crates/plan/src/iron.rs#pub fn cave_near(w: &World, feet: Cell, rules: &IronRules, taken: &[Cell]) -> Option<Cell> {"
  - "ruststeve:crates/plan/src/iron.rs#pub fn next_strip("
  - "ruststeve:crates/plan/src/iron.rs#pub(crate) fn iron_search("
  - "ruststeve:crates/plan/tests/iron_search.rs#fn a_heading_into_its_own_tunnel_is_turned_from() {"
  - "ruststeve:config/goals.toml#Feet under this cut stairs up."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[veins-and-flint]]"
  - "[[stairs-down]]"
  - "[[progress-invariants]]"
---

# The iron search

Generality item B, wall G3 of `docs/problems/seeds.md`; the problem file is `docs/problems/iron-search.md`. From `stone_pickaxe`, gear needs about 22 iron, and with strict no X-ray the bot mines only iron it has seen with an open face.

## Key files
- `crates/plan/src/iron.rs`: `IronRules` (`[iron]` in `config/goals.toml`), `IronSearch` (on the planner as `Plan::iron`), `fresh`, `fresh_heading`, `cave_cell`, `cave_near`, `IronSearch::next_strip`, `Plan::iron_search`.
- `crates/plan/src/lib.rs`: the `// >>> iron search (gen)` block in `Plan::mine`, after the walks and the dig toward exposed ore, before the old level tunnel.
- `crates/plan/tests/iron_search.rs`: one hand-built world for each choice.

## Why
The old fallback at the depth was `Stairs::level` on the held heading, and a tunnel that ends "done" never turns the heading. At a face it could not cut, the level step turned inside the task back into its own open tunnel (an open cell is never refused) and walked it, each step a "stair". In race 46130982111 (typecraft, 14 bots) 2,727 of 3,711 "cut 6 stairs" tunnels ended in under 100 ticks with nothing dug; rs-l00-b00 walked one six-block corridor at y -40 for its last 13,600 ticks. The bots also sank into deepslate (median end y about -23) through digs toward ore below.

## How it works
Only for a row whose blocks name iron ore, with the feet at or under its depth (16), and only when no exposed iron has a walk or a dig toward it. In order:
1. **Rise**: feet under `rise_below` (y 0) cut `Stairs::up` on the heading ("rise toward iron's band from y …"). Under y 0 the rock is deepslate (hardness 3, stone's 1.5) and the middle band thins to nothing at -24.
2. **A cave**: `cave_near` looks ring by ring, every second column, from `cave_from` (24, `exposed_near`'s radius: nearer caves' walls are already read) to `cave_reach` (48), within 12 of the feet and inside y -24 to 56, for a `cave_cell`: open, open over it, a full block under it, and open on all four sides on the plane. A one-wide tunnel or staircase never has that cross. The search runs at most every `cave_every` (200) ticks. A cave with a route is walked into (`Task::Explore`, "walk into the cave at …"); with none it is dug toward through `Plan::dig_toward` ("dig toward a cave at … (cut n of 8)"), given up after `cave_cuts`. Caves taken (walked to, reached, refused, given up) are kept (64) and one within `cave_apart` (16) of them is the same cave.
3. **A strip in fresh rock**: a heading is fresh when its first level step passes `stairs::refusal` and the feet and head cells of its next `fresh_ahead` (3) columns are full blocks. `fresh_heading` tries the wanted heading, a quarter right, a quarter left, then back. The strip keeps its heading, its start, how far along it the feet have been and the raw iron held when it began. It ends:
   - after `strip_len` (24) blocks: turned a quarter, left and right in turn, so strips step across new ground and never close a square ("strip turned after 24 blocks", with the raw iron it gave);
   - after `strip_stalls` (2) tunnels in a row no further along ("strip turned, stalled at …");
   - when the cells ahead are open ("strip turned from open cells ahead …");
   - when the feet are more than a block off its line or height, say after a walk to ore ("strip left after …").
4. **No fresh rock on any heading**: stairs down while the feet are over both the depth and the rise line plus six, else up ("stairs down for fresh rock"), never the old shuttle.

## No X-ray
The search reads what the repository already counts as seen: open cells and blocks with an open face (the rule of `exposed_near`, `tunnel.rs` and `Stairs::rock`'s cave test). `fresh` reads only whether cells ahead are open or full; it never reads the kind of a block with no open face. Ore is still taken only through `exposed_near`.

## Limits
- Not raced yet on this branch: the judging races (8-seed relay snapshots, paired against `stack`, cap 72,000) are the lead's.
- Coal and gravel rows keep the old tunnel; the shuttle can still happen there.
- `IronRules` is read from its defaults; a test holds `[iron]` in `config/goals.toml` equal to them (the make table does not load it).
- The level step inside a tunnel can still turn into an open cell (stairs.rs is not changed); the strip's stall count turns the strip after two such tunnels.

## See also
- [[veins-and-flint]] - the mine rows and their depth.
- [[stairs-down]] - the staircase and the refusals a strip takes.
- [[progress-invariants]] - the rule the strip's turns follow.
