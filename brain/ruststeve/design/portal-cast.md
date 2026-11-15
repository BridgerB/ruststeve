---
title: The portal cast
description: How ruststeve casts and lights a Nether portal with no diamond pickaxe (issue 14c) - the cast as a build plan in data ported from clojurecraft's build.clj and checked against it, the executor that works the first op not done, the site chosen beside lava seen exposed under strict no X-ray, lava refills, the lava hunt, and the portal goal row and kit start.
type: reference
tags: [portal, cast, build, obsidian, lava, plan, site]
aliases: [plan::build, plan::cast, portal_plan, frame_plan, cast_one_plan, Op, Done, Frame, Facing, Cast, Site, choose_site, blocks_needed, spent, foundation, Aim::Portal, portal row, kit start, lava hunt, LAVA_DEPTH]
status: verified
lastUpdated: 2026-11-15
verifiedAgainst: ruststeve@da0e4fa clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/cast.rs#pub fn choose_site(w: &World, given_up: &[Cell]) -> Option<Site> {"
  - "ruststeve:crates/plan/src/cast.rs#pub fn blocks_needed(w: &World, plan: &[Op]) -> usize {"
  - "ruststeve:crates/plan/src/cast.rs#pub fn spent(w: &World, lava: Cell, frame: Frame) -> bool {"
  - "ruststeve:crates/plan/tests/build_differential.rs#fn portal_plans_equal_clojurecraft() {"
  - "ruststeve:crates/bot/tests/cast.rs#fn the_kit_casts_and_lights_a_portal_beside_lava() {"
  - "clojurecraft:src/clojurecraft/build.clj#(defn cell-ops"
  - "clojurecraft:src/clojurecraft/build.clj#(defn choose-site"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[mc-portal-cast]]"
  - "[[buckets-and-fluids]]"
  - "[[memory-sightings]]"
  - "[[planner]]"
  - "[[strict-no-xray]]"
---

# The portal cast

Issue 14c. No bot mines obsidian; the frame is cast ([[mc-portal-cast]]). The rock is the mould: the frame's ten cells are holes cut a block deep into a rock face beside lava; lava is poured low against a hole's back wall, water into the cell over it against the same wall (it lands on the source: obsidian) and scooped straight back.

## Key files
- `crates/plan/src/build.rs`: the plan as data (`Op`, `Done`, `Frame`, `Facing`, `cast_one_plan`, `frame_plan`), `holds` and `next_op`, and the differential's text.
- `crates/plan/src/cast.rs`: the executor (`Cast`), the site (`choose_site`, `blocks_needed`, `spent`, `foundation`), refills.
- `crates/plan/src/lib.rs`: `Aim::Portal`, `cast_task`, `after_cast`, `LAVA_DEPTH`.
- `config/goals.toml` row `portal`; `config/starts.toml` start `kit`.
- Tests: `crates/plan/tests/build_differential.rs`, `crates/bot/tests/cast.rs`.

## How it works
- **The plan** (a port of `build.clj`): the room dug out in front of the face (the landing first, then every cell nearest the landing first, then firm floors), each frame cell in cast order (walls made firm where the rock is missing, hole and the cell over it dug, lava poured, water poured, water scooped back), the 2x3 inside dug, flint and steel on the bottom cell. Each op is done by a predicate over the world; a dig is done once its cell is obsidian, so **obsidian is never dug** (v1 dug its own frame and restarted forever). Positions are written for working from the south and turned for each facing.
- **The executor** works the first op not done, so a death or a reload resumes from the world. A dig is worked from wherever the bot reaches the block (a walk to within 2.5 when not). A place clicks a solid neighbour's face toward the cell, the block under first, after stepping out of the cell if the body is in it; a cell no neighbour holds gets a foundation built up through the air under it, at most 4 deep. A pour, a scoop and the light are worked from the op's own stand with the look on its aim, held 6 ticks before the use.
- **Refills**: a pour whose bucket is empty fills it first, lava at the site's pool (the nearest four sources tried in turn, the first a walk reaches), water at a remembered surface source ([[buckets-and-fluids]]).
- **Progress invariants**: 3 tries an op; 2,400 ticks with no op done; lava seen in the room or the walls (not the cast's own holes) gives the site up; a given-up site refuses a new one within 16 blocks.
- **The site** (`choose_site`, after clojurecraft's): beside one of the 3 nearest lava sources memory saw exposed within 48 blocks; the room's landing an open standable cell 4 to 9 blocks from the lava on the plane, at its height to three over; every footprint cell loaded, none **seen** holding fluid and none within a block of a remembered lava source; the frame facing so the lava is in front; at most 8 blocks to place (counted only where the eye sees the rock missing, foundations included); a pool with at least as many sources as cells left (`spent`). Fewest blocks, then nearest the feet.
- **Strict no X-ray** ([[strict-no-xray]]): the site reads only cells with a see-through neighbour; a cell inside rock counts as rock.
- **The lava hunt**: with no site, the mine task's stairs go down toward `LAVA_DEPTH` (feet at y -50, a block over the lava sea); the lava they expose enters memory's pools ([[memory-sightings]]).
- **The aim**: `portal` holds the kit the cast spends (a water bucket, a bucket for the lava, flint and steel, a pickaxe); once held, the cast; met when a portal block shows in the frame. The lit frame is kept (`Plan.portal`) for issue 15 to walk back to.
- **The `kit` start** (Bridger, 2026-11-15): an iron pickaxe, two buckets, flint and steel, a stone pickaxe and sword, a table, 8 sticks, 16 cobblestone, spread 128.

## Evidence
- `build_differential`: 144 plans (cast-one, portal and nether; four facings; lava heights 0, 1, 3, 6; three origins), **32,124 of 32,124** op lines equal to clojurecraft's `plan-for` at be85a92 (a one-off oracle in its flake shell).
- `tests/cast.rs`: from the kit, on a stone plateau with a corridor, an 18-source lava pool in its floor and a pond outside, the bot scoops its water, chooses a site, works 103 ops and lights the portal by tick 2,272.

## Gotchas found on the way
- A scoop's source shows gone a tick before the bucket shows filled, and water forms again between two sources: a scoop is judged gone only before its use.
- A pool's edge sources go first; the rest lie four or more blocks from the bank, reached over the flowing lava (a source-only ray passes over it): the scoop reaches 4.4 and walks to within 3.5.
- A site whose room fronts open ground needs its stair floors built on foundations: more than 8 blocks, so the chooser prefers rooms cut from rock, as clojurecraft's do.

## Limits
- A cast spends one lava source a cell: a site's pool must hold ten (`spent`), and a pool's far sources are reached only across its flowing edge.

## See also
- [[mc-portal-cast]] - the cast as the game plays it.
- [[buckets-and-fluids]] - the look, buckets and the model's fluids.
