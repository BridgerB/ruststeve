---
title: The portal cast
description: How ruststeve casts and lights a Nether portal with no diamond pickaxe (issue 14c) - the cast as a build plan in data ported from clojurecraft's build.clj and checked against it, the executor that works the first op not done, the site chosen beside lava seen exposed under strict no X-ray, lava refills, the lava hunt, and the portal goal row and kit start.
type: reference
tags: [portal, cast, build, obsidian, lava, plan, site]
aliases: [plan::build, plan::cast, portal_plan, frame_plan, cast_one_plan, Op, Done, Frame, Facing, Cast, Site, choose_site, blocks_needed, spent, foundation, Aim::Portal, portal row, kit start, lava hunt, LAVA_DEPTH]
status: verified
lastUpdated: 2026-11-17
verifiedAgainst: ruststeve@610fd87 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/cast.rs#pub fn choose_site(w: &World, given_up: &[Cell]) -> Option<Site> {"
  - "ruststeve:crates/plan/src/station.rs#pub fn station_among("
  - "ruststeve:crates/plan/src/scoop.rs#pub fn ray_cells(from: [f64; 3], to: [f64; 3]) -> Vec<Cell> {"
  - "ruststeve:crates/plan/tests/rays.rs#fn the_grazed_corner_of_race_44041169156_is_entered() {"
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
- `crates/plan/src/station.rs`: the lava station, the straight way to a stand, the sight dig.
- `crates/plan/src/scoop.rs`: the bucket's aim (`aim_for`) and its exact sight line (`ray_cells`, `clear_ray`).
- `crates/plan/src/lib.rs`: `Aim::Portal`, `cast_task`, `after_cast`, `LAVA_DEPTH`.
- `config/goals.toml` row `portal`; `config/starts.toml` starts `kit` and `lava`.
- Tests: `crates/plan/tests/build_differential.rs`, `crates/plan/tests/rays.rs`, `crates/bot/tests/cast.rs`.
- The walls, each read from a recording before its fix: `docs/problems/portal.md` (1 to 27).

## How it works
- **The plan** (a port of `build.clj`): the room dug out in front of the face (the landing first, then every cell nearest the landing first, then firm floors), each frame cell in cast order (walls made firm where the rock is missing, hole and the cell over it dug, lava poured, water poured, water scooped back), the 2x3 inside dug, flint and steel on the bottom cell. Each op is done by a predicate over the world; a dig is done once its cell is obsidian, so **obsidian is never dug** (v1 dug its own frame and restarted forever). Positions are written for working from the south and turned for each facing.
- **The executor** works the first op not done, so a death or a reload resumes from the world. A dig is worked from wherever the bot reaches the block (a walk to within 2.5 when not). A place clicks a solid neighbour's face toward the cell, the block under first, after stepping out of the cell if the body is in it; a cell no neighbour holds gets a foundation built up through the air under it, at most 4 deep. A pour, a scoop and the light are worked from the op's own stand with the look on its aim, held 6 ticks before the use.
- **No walk to an op** (wall 22): the approach digs the first block of the straight way at the feet's height toward the op, when it is in reach, never the frame's rock or a cell the plan places, at most 12 an op (clojurecraft's `toward-stand`). Before, three tries went in three ticks and the site was given up.
- **Refills**: a pour whose bucket is empty fills it first, water at a remembered surface source ([[buckets-and-fluids]]), lava in this order:
  1. the station (below), when one stands;
  2. else the nearest four sources of the site's pool, the first whose scoop routes; a scoop's route counts only when its end, or the eye when the bot is already there, sees a face of the source (wall 26);
  3. else the first block on the eye's line to a source in reach is dug: under the feet, never kept or placed, no lava beside it at its height or over it (lava never flows up; wall 24), at most 24 a cast;
  4. else a level tunnel toward the pool's nearest source, at most 4.
  A water refill takes first the water sources seen within 7 of the frame's origin, the cast's own poured water (wall 28: a pour that made no obsidian left its water in the room, and memory keeps only surface water).
- **A place** (walls 30, 31): steps out first when the player's box overlaps the cell (`overlaps`, the box from `physics::aabb`, not the feet's cell), and lays the op's block when held, else the first of `FILLERS` held (clojurecraft's `filler/fillers`: cobblestone, cobbled deepslate, the stones that drop themselves, dirt, the Nether's rock; never gravel or sand). Below y 0 a bot carries cobbled deepslate, and once the kit's 16 cobblestone were laid every place op had failed.
- **The bucket's aim** at the use takes a face the exact line reaches first (`aim_at_use`); stands and routes keep the sampled line. The pair lit 62 of 210 (00c2c3a) against 58 of 224, and wasted uses fell from 168 to 84 a race.
- **Defend beside lava** (wall 29): a fight's close-in and a retreat with no route step only where `safe_ahead` finds no lava seen ahead or under it down to 4 and a floor within 4; 15 of 19 lava deaths in one pair came within 300 ticks of a defend.
- **The sight line** (wall 27): `clear_ray` samples forty points along the eye's line and decides; `exact_ray` (`ray_cells`, every cell the line enters, Amanatides and Woo) is computed beside it at every use and only measured. The exact line is right in the one case read (a sampled line stepped past a grazed corner and the bucket filled nothing), but the pair that let it decide lit 32 of 224 against 56. A cast counts its bucket uses four ways (filled or empty, exact clear or blocked) and names them in its notes, so a race says where the exact line and the server differ.
- **The refill's trail**: a cast keeps its refill's last eight steps (station or none, walks and digs failed, scoops routed and failed with the reason, no route), and a failed cast's note names them.
- **Progress invariants**: 3 tries an op; 2,400 ticks with no op done; lava seen in the room or the walls (not the cast's own holes) gives the site up; a given-up site refuses a new one within 16 blocks. A cast counts each time its first op not done moves back to an earlier one (a regression) and names the last in its note.
- **The site** (`choose_site`): beside one of the 3 nearest lava sources memory saw exposed within 48 blocks; the room's landing an open standable cell 4 to 9 blocks from the lava on the plane, three under its height to three over; every footprint cell loaded, none **seen** holding fluid and none sharing a face with a remembered lava source; the lava in front of the frame's plane, on the room's side (wall 24); at most 16 blocks to place (counted only where the eye sees the rock missing, foundations included); a pool with at least as many sources as cells left within 16 (`spent`); a walk from the landing to within reach of the lava, or lava three under to one over the landing. Fewest blocks, then nearest the feet; of the best four, the first a walk reaches.
- **Not clojurecraft's site yet**: its `choose-site` builds the site from a stand beside the cell over a source, the frame seven behind and 1, 3 or 6 under it, so every site has a station. Ported alone it lit 7 of 112 against 18 and 20 (wall 21, reverted): the room sits in rock, and it needs the executor to dig its way to every stand.
- **Strict no X-ray** ([[strict-no-xray]]): the site reads only cells with a see-through neighbour; a cell inside rock counts as rock.
- **The lava hunt**: with no site, the mine task's stairs go down toward `LAVA_DEPTH` (feet at y -50, a block over the lava sea); the lava they expose enters memory's pools ([[memory-sightings]]).
- **The aim**: `portal` holds the kit the cast spends (a water bucket, a bucket for the lava, flint and steel, a pickaxe); once held, the cast; met when a portal block shows in the frame. The lit frame is kept (`Plan.portal`) for issue 15 to walk back to.
- **The `kit` start** (Bridger, 2026-11-15): an iron pickaxe, two buckets, flint and steel, a stone pickaxe and sword, a table, 8 sticks, 16 cobblestone, spread 128.

- **Lava station** (`crates/plan/src/station.rs`, clojurecraft's `station-among`): a stand at h 1 to 6 over the room's floor, four to nine in front of the frame and up to eight aside of the landing's column, firm floor, dry round, lava below the feet in reach. A lava refill scoops any station source already in reach, else walks to the stand (off its height, to the landing at it; at its height, digs the straight way's first block), there scoops through a clear face or digs the line to a source below the feet. A way never digs the frame's rock or a cell the plan places (wall 20: a way through the room's floor undid its place op 360 times). Two failed walks or digs and the cast refills from the nearest sources instead (wall 18).
- **The `lava` start** (Bridger, 2026-11-16: work from a checkpoint): the kit with the water bucket filled, at fourteen places beside lava on the race seed, one bot a pool a lane, cap 24,000 ([[race-and-results]]).

## Evidence
- `build_differential`: 144 plans (cast-one, portal and nether; four facings; lava heights 0, 1, 3, 6; three origins), **32,124 of 32,124** op lines equal to clojurecraft's `plan-for` at be85a92 (a one-off oracle in its flake shell).
- `tests/cast.rs`: from the kit, on a stone plateau with a corridor, an 18-source lava pool in its floor and a pond outside, the bot scoops its water, chooses a site, works 118 ops and lights the portal by tick 2,880.
- `tests/rays.rs`: the exact sight line, with race 44041169156's grazed corner as a literal case.
- **Real, from `kit`** (the issue's measure): never over 2% (walls 1 to 15).
- **Real, from `lava`**, pairs of 8 lanes x 14 pooled over 224: the crowded checkpoint 10; the station refill 14; one bot a pool 38 (0347960); the way dig 53 (c29153b); walls 24 and 26 52 and 56 (98e43ab, 6d82b7b). Six of the fourteen places light none; the bots that light mostly light their second site.

## Not verified
- `plan::station` has no differential against clojurecraft (the plan has one); its world-reading parts have no unit test.
- Why the exact sight line lit fewer portals (wall 27): counted by the next race, not known.
- Walls 28 and 29 raced (1d4eae0, 62 of 224): own water fired in 2 bots; 1,410 steps held back, lava and fire deaths 3 a pair against 10 to 19. Wall 31 (fillers) is not raced.
- `FILLERS` lays stone a spare pickaxe needs (clojurecraft's `stone-kept` is not ported).
- Why a water pour sometimes makes no obsidian (wall 28's first cause): not read.
- The thresholds added with walls 18 to 27 (`STATION_MISSES`, `WAY_DIGS`, `SIGHT_DIGS`, `REFILL_TRAIL`, the station's reach) are constants in the code, each with its source in a comment, not rows of a data file.

## Gotchas found on the way
- A scoop's source shows gone a tick before the bucket shows filled, and water forms again between two sources: a scoop is judged gone only before its use.
- A pool's edge sources go first; the rest lie four or more blocks from the bank, reached over the flowing lava (a source-only ray passes over it): the scoop reaches 4.4 and walks to within 3.5.
- A site whose room fronts open ground needs its stair floors built on foundations, many blocks, so the chooser prefers rooms cut from rock, as clojurecraft's do.
- A checkpoint that puts bots at the same pool measures the crowd, not the cast: twelve places round five pools had three or four bots a pool on one server, digging each other's cells (wall 19).
- Three fixes reasoned from reach and geometry, with no bot's own lines to confirm them, were reverted after racing worse (walls 21, 23, 25). A cause is confirmed in a recording first, and its new path leaves a note so its firings can be counted.

## Limits
- A cast spends one lava source a cell: a site's pool must hold ten (`spent`), and a pool's far sources are reached only across its flowing edge.

## See also
- [[mc-portal-cast]] - the cast as the game plays it.
- [[buckets-and-fluids]] - the look, buckets and the model's fluids.
