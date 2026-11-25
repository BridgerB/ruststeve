---
title: Stairs down
description: How the bot goes underground safely (plan::stairs, a port of the core of clojurecraft's stairs.clj) - a stair opens three cells one block ahead and down and is walked onto, the block under the feet is never dug, every cell is checked for water, lava, an unloaded column and a floor before anything is dug, refusals turn a quarter, level steps when boxed in, done after six stairs.
type: reference
tags: [plan, stairs, mine, safety, underground]
aliases: [Stairs, StairStage, refusal, Boxed, level step, STAIR_DEPTH, never dig under the feet, stairs-down, mine stone, exposed_near]
status: verified
lastUpdated: 2026-11-25
verifiedAgainst: ruststeve@b09de3d clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/stairs.rs#pub fn refusal(w: &World, cells: &[Cell], floor: Cell) -> Option<String> {"
  - "ruststeve:crates/plan/src/stairs.rs#fn plan(&mut self, feet: Cell, w: &World) -> Result<StairStage, StairsFailure> {"
  - "ruststeve:crates/plan/src/lib.rs#pub fn exposed_near(w: &World, kinds: &[data::BlockKind]) -> Vec<Cell> {"
  - "ruststeve:crates/bot/tests/stairs.rs#fn water_lava_and_drops_refuse_a_stair() {"
  - "clojurecraft:src/clojurecraft/stairs.clj#Going down safely: the :stairs-down intent, which cuts a staircase one stair at a time in the"
  - "clojurecraft:src/clojurecraft/stone.clj#(def search-radius 24)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[mc-water]]"
  - "[[digging]]"
  - "[[goal-table-and-needs]]"
---

# Stairs down

Issue 11b. The mine act (`mine-stone`) digs the nearest **exposed** block of its kinds the bot can stand beside, and when none is in sight cuts stairs down. Blind drops into lava killed clojurecraft's siblings, so the stairs never dig under the feet.

## Key files
- `crates/plan/src/stairs.rs`: `Stairs`, `refusal`.
- `crates/plan/src/lib.rs`: `exposed_near`, `Plan::mine`.
- `crates/bot/tests/stairs.rs`, `crates/bot/tests/stone.rs`.

## How it works
- **Exposed** means a face open to air, read from the loaded terrain within 24 blocks and 12 up or down (clojurecraft's `search-radius`, `search-height`): stone is most of the world, and only what is exposed now is worth a route. The search goes ring by ring outward and stops at 8 found (a whole-radius scan cost 0.4 s in one tick); the nearest 3 not refused are routed with the pickup goal and the first with a way is dug ([[digging]]). No block without an open face is ever chosen: no X-ray.
- **A stair**: from the feet F along the heading (dx, dz), open (F+dx, y+1), (F+dx, y) and the new feet (F+dx, y−1), top first, each by the dig task (its drop collected: digging stone stairs gives cobblestone), then walk onto the new feet. The block under the feet is never dug.
- **Before any cell is dug** (`refusal`): an unloaded column, water or lava in a cell, lava beside a cell, or no full block under the new feet refuses the stair; the heading turns a quarter. All four refused: a level step (two cells along, a floor under them), at most 3, then `Boxed`.
- A magma block is no floor (blaze wall 23): `refusal` answers "magma under" for it and a short drop never lands on it, as `path::classify` never stands on it. Stairs planned onto magma in the Nether failed `StepFailed after 0 stairs` for ever, nothing refused and no heading turned.
- **Done** after 6 stairs (`stair-depth`): the planner looks again. Judged by the feet having moved, never by a packet sent.
- A stair that fails makes the planner turn its heading for the next.

## Evidence
- `crates/bot/tests/stairs.rs`: a clear stair is taken; water in a cell, lava beside one, a missing floor and an unloaded column each refuse it.
- Sim measure from the `table` start: 195 of 200 landings make a stone pickaxe and sword in 12,000 ticks.

## Limits
- Gravel or sand overhead is not checked (it falls on the bot); the stairs do not go up again (clojurecraft's stairs-up), nor lay a floor over a cave (its cast floor). Air underwater and drowning are issue 12.

## Issue 13b
- A mine row's `depth`: past it, `Stairs::level` tunnels along instead ([[veins-and-flint]]).

- **Short drops** (wall 17): a stair may land up to 3 below where it was cut (a cave under it); the step is done where the feet land, deeper refused.
- **Falling blocks**: a cell with gravel or sand over it is refused (it falls on the bot).
- Stair digs collect nothing: the stairs never stop for a drop.

## See also
- [[mc-water]] - why water and lava are refused.
- [[digging]] - the dig each cell is.

## Issue 16: stairs up toward what was seen
- `Plan::dig_toward` cuts `Stairs::up` toward a target more than a block over the feet, three digs without coming nearer refusing it. Two callers send a bot underground up to what it saw: the scoop's bank over the feet (`stairs_to_bank`, blaze wall 5) and a log more than 3 blocks over the feet with no walk, under a rock roof (`stairs_to_log`, blaze wall 6: a cave bot with the trees 40 blocks up explored level for three hours). A third: a far pool (24 to 96 blocks) more than 3 over the feet, from under rock, gets stairs up to its bank before any walk (`roofed_below`, blaze wall 9: cave bots walked at the same three pools over a hundred times each, every walk NoPath).

## Issue 16: the travel tunnel (`plan::tunnel`)
- **When**: `fortress_task`, nether bricks seen and no found walk to them. It replaces the stairs `dig_toward` cut there (down six from the lava sea's level, beside lava: `docs/problems/nether-tunnel.md`).
- **The route**: `tunnel::route` searches feet cells from the feet to within 5 of the target brick or any brick within 12 of it, each move a level step, a stair up or a stair down (`tunnel::opened` gives the cells and the floor, the same cuts as above). A weighted A*, at most 60,000 nodes.
- **What it may read** (no X-ray): open cells, blocks with an open face, and fluid whose column's top has one. Anything else is rock: diggable, a floor. What rock hides is left to `refusal` one step ahead.
- **Costs steer it**: a step under y 40 (`SAFE_Y`) costs 6 more, so a long route climbs first and comes down at the far end; a step in the open costs 4 more than a cut; a laid block 2. A portal's blocks, obsidian and bedrock are never opened.
- **Cutting it**: `Stairs::along(stretch)` takes six steps of the route. A refusal ends the stretch (no heading is turned); the planner (`Plan::travel_tunnel`) reads where the feet are on the route, notes the refusal ("tunnel step to … refused", with the reason: lava beside, magma under, no floor, a falling block), blocks that cell and searches again. A stretch that ended short twice at one cell with nothing refused blocks it too.
- **Laid stairs**: a stair up with nothing to step onto (a cavern's floor, the air under a roof) lays held netherrack or cobblestone first: the cell under the step, clicked onto the side of the block the feet stand on, then the step on top of it (`stairs::lays`, `StairStage::Lay`).
- **Progress invariant**: 16 stretches begun with the feet no nearer the target on the plane give the tunnel up and refuse the target a while.
- **A staircase that did nothing fails**: six steps with nothing dug that end within a block of where they began fail `Boxed` ("nothing cut and nowhere gone"), not "cut 6 stairs".
- **Looking at a recorded world**: `survey <file.rsr> --at <ticks> --slab|--line …` prints slabs and sections; `--route x,y,z,tx,ty,tz` prints the route the search finds there and the steps today's world would refuse.
- **Raced** (race 45337739022, 3079a6b): 3 of 3 bots from the Nether snapshot reached `fortress` by the tunnel alone, one went on to `spawner`; 23 to 25 routes, 58 or 59 stretches and 22 refusals a bot.
- **Suspect rock**: a step into rock within 3 of a blocked cell costs 8 more, so a route leaves a bad face instead of trying it cell by cell.
- **`Plan::tunnel_toward`** (pub): the tunnel to one cell, ending only at it; `dig_toward` hands it any Nether target more than 8 blocks off (`TUNNEL_FROM`).
- **Every failure names its reason**: a step (the cells, the walk's failure, the floor's block), a dig, and `Boxed` (each heading's refusal).

## Issue 16: dry stairs (overworld-leg wall 7)
- **`Stairs::dry`**: water beside a cell a stair or level step opens refuses it too ("water beside", the heading turns), as lava beside does for every staircase. Set by the lava hunt's own stairs (`Plan::own_stairs`) only: a leg at y 55 cut beside an aquifer, the water ran in and the bot was held in it 20,000 ticks (race 45343519424 lane 2). The ending note counts the turns ("cut 6 stairs (N turned from water beside)"). Test `a_dry_staircase_turns_from_water_beside` (bot).
- **Back from a ledge** (overworld-leg wall 9): two own-stairs legs from one cell that left the feet there walk the bot back up the leg that led to it, once (`Plan::own_into`, "own stairs back to …"); the next leg from there turns from that heading (`own_back`, "own stairs turned from … to …").
- **Laid stairs need ground**: only with ground within 6 under the step (`LAY_OVER`), never out over a drop; a block that has shown leaves the list and the next gets its own click.
- **A staircase is bounded**: one stair's cells take at most 8 digs (`MAX_CUT_TRIES`) and the feet stay in one cell at most 1,200 ticks (`STAND_TICKS`); a travel tunnel's step whose walk fails is steered straight (the end note counts blocks laid and steps steered).
- **Infested blocks** (issue 19f, the stronghold): a cell to open that is an infested kind (`stairs::infested`, read from chunk data as lava beside is) is refused ("infested at …"), and the travel tunnel's route reads it as a wall: a silverfish woken is a fight under no-heal health.
- **No bricks seen, in the Nether**: `Plan::explore_tunnel`, a search tunnel on a held compass heading, 48 blocks a leg, before any explore walk.
- **Raced again** (race 45344386490, d4422d4): 2 of 3 at `fortress`, refusals 7 a bot (22 before); the third died to a ghast on the portal ledge at tick 240, before its first stretch.
