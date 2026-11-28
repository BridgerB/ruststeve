---
title: Buckets and fluids
description: How ruststeve looks up and down and uses a bucket along the look (issue 14a) - the controls' pitch, look_at, the server model's fluid ticks, bucket pick-up and pour, lava turned to obsidian by water, a frame lit by flint and steel, all read from the 26.1.2 jar, and the parity rows that hold them.
type: reference
tags: [fluids, buckets, water, lava, obsidian, portal, look, sim, parity]
aliases: [bucket, buckets, look_at, pitch, Controls.pitch, FlowingFluid, fluid tick, tick_fluids, use_bucket, obsidian, portal lighting, PortalShape, flint and steel, use script, light script, watch cells]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@9b5da2e clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/sim/src/fluid.rs#pub(crate) fn tick_fluids(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/fluid.rs#pub(crate) fn use_bucket(&mut self) -> Option<Result<Vec<Wire>, EncodeError>> {"
  - "ruststeve:crates/sim/src/fluid.rs#fn portal_shape(&self, p: Pos) -> Option<PortalFrame> {"
  - "ruststeve:crates/plan/src/walk.rs#pub fn look_at(eye: [f64; 3], to: [f64; 3]) -> (f64, f32) {"
  - "ruststeve:crates/bot/tests/buckets.rs#fn a_water_source_is_scooped_along_the_look() {"
  - "ruststeve:crates/bot/tests/buckets.rs#fn flint_and_steel_lights_an_obsidian_frame() {"
  - "ruststeve:config/moments.toml#id = \"w00-obsidian\""
  - "clojurecraft:src/clojurecraft/scoop.clj#(def surface-height 0.85)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[server-model]]"
  - "[[parity-table]]"
  - "[[physics]]"
  - "[[mc-portal-cast]]"
  - "[[mc-water]]"
---

# Buckets and fluids

Issue 14a, the first part of the portal cast ([[mc-portal-cast]]). A bucket is used along the look (`use_item`), not on a block face (`use_item_on` with a bucket does nothing), so the bot had to learn to look up and down before anything else.

## Key files
- `crates/physics/src/lib.rs`: `Controls.pitch`.
- `crates/world/src/lib.rs`: the look sent in move packets.
- `crates/plan/src/walk.rs`: `look_at`.
- `crates/sim/src/fluid.rs`: the model's fluids, buckets, fire and portal shape.
- `crates/bot/tests/buckets.rs`: closed-loop tests.
- `config/moments.toml`: the `w` rows.

## The look
- `Controls.pitch` is an `Option<f32>`, `#[serde(default)]` so old recordings load: `None` keeps the pitch the server last set (a teleport), `Some` is sent in the next move with rotation.
- `look_at(eye, point)` gives the yaw and the Notchian pitch (down positive) to a point.
- A bucket act is: hold the bucket, set the look, let a tick pass so the server has it, then `UseItem`. Done is judged by the inventory, never by the packet.

## The model's rules (26.1.2, read with CFR)
- **Bucket use** (`BucketItem.use`): the eye ray to `blockInteractionRange` 4.5, clipped to sources for an empty bucket, to blocks for a full one. An empty bucket takes a source (`LiquidBlock.pickupBlock`: level 0 only) and becomes the filled bucket; a full one places a source in the cell beside the hit face when that cell is air, a fluid or a block that does not block motion, and becomes a bucket. Of a stack of two buckets one is filled and added as a pickup is.
- **Fluid ticks** (`FlowingFluid.tick`, `spread`, `getNewLiquid`, `getSpread`, `getSlopeDistance`): water every 5 ticks, overworld lava every 30; drop-off 1 for water, 2 for lava; slope search 4 and 2; in the Nether (the dimension type's `gameplay/fast_lava`, `ModelWorld::fast_lava`) lava ticks every 10 with drop-off 1 and slope search 4, so it runs 7 cells from a source, not 3 ([[server-model]]); a new water source between two sources over a solid or a source (the default game rule; lava never). Ticks are kept in the model's own queue by due tick and order scheduled, run before the players' tick as `ServerLevel` does.
- **Updates** (`Level.setBlock` with flag 3): a change tells every liquid neighbour, which converts or schedules its tick, and a placed fluid schedules its own (`onPlace`).
- **Obsidian** (`LiquidBlock.shouldSpreadLiquid`): lava with water above or beside it becomes obsidian when a source, cobblestone when flowing; lava falling into water makes stone (`LavaFluid.spreadTo`).
- **Fire and the portal** (`FlintAndSteelItem.useOn`, `BaseFireBlock`, `PortalShape`): fire goes beside the clicked face when that cell is air and has a sturdy floor or an empty frame round it with obsidian on one of its six sides (`BaseFireBlock.isPortal`; the obsidian rule added in issue 83, [[mc-portal-cast]]); fire placed in an empty frame fills it with `nether_portal` (the X axis tried first, right to the west, then Z). Frames 2 to 21 wide and 3 to 21 high.
- A portal block has no collision in the generated data (v1 found the opposite once; a test holds it).

Left out, and why: waterlogged blocks; partial shapes occluding a flow; drops from blocks a flow replaces; lava's random fire ticks; the plant outline being smaller than a cube to the ray. None is in the cast's rock.

## Evidence
- `crates/bot/tests/buckets.rs`, all seven pass: a source scooped along the look, one of two buckets filled, water poured and flowing three blocks out and no further than seven, lava poured low against a hole's back wall staying a source, water against the wall over a lava source making obsidian, flint and steel lighting a 4x5 frame, and a portal block with no collision.
- Parity rows `w00` to `w02`: scoop, pour, lava, obsidian and portal, five a landing, each with `watch` cells whose blocks join the outcome class (a `blocks` field). **152 of 152** moments match in every field on the runners (run 43732515322, f39f801).
- The first run (43730534618) found every real bucket use missing: `use_item` carries a rotation the server snaps the player to before the use (`handleUseItem`), and the world sent pitch 0. It now sends the look's pitch, and the model applies the packet's rotation.

## Gotchas
- A pour aimed at a hole's floor from the bank hits the bank's top first and lands a block high: aim low against the hole's back wall, as clojurecraft's `side-aim` does.
- The server's ray takes the first block it touches, so a sight line is checked cell by cell (`scoop::ray_cells`, exact voxel traversal), never by samples: forty samples along a line stepped past a deepslate corner it entered two hundredths deep, and a bucket aimed true at a lava source filled nothing eight times (race 44041169156, rs-l00-b04; clojurecraft issue #225; [[portal-cast]] wall 27).
- A scoop aims at the first of a source's faces in reach with a clear line (the surface, the four sides, the underside): the ray takes a source through any face.
- Two sight lines, two jobs: the sampled line (`aim_for`, `clear_ray`) finds stands and routes; the exact line (`aim_at_use`, `exact_ray`) decides the use and every "a use from here would fill" check. A use on a face only the sampled line passes filled nothing 342 times in one race ([[portal-cast]] walls 34 and 36).
- A stand is judged from its cell's middle; a walk ends off it. When the real eye sees no face there the scoop centres, sneaking, for up to 20 ticks (wall 37).
- A scoop walk tries the four nearest guessed stands, then one search for any bank cell beside the source with the feet level with the surface or one above (`Goal::Pickup`, range 1, rise 1: clojurecraft's scoop walk), then a `Near` walk (wall 39).

## Issue 20: a pour while falling (the water clutch, docs/problems/end-model.md)
- Rows `e06-clutch-*` and `e06-drop`: a 21 fall onto water poured on the way down cost 0; the same fall dry cost 18.
- Rows `e09-pour-*` (parity 45929976619, falls of 8 to 60 onto end stone): water is placed exactly when the eye is within the bucket's 4.5 of the ground at the **last position the server heard** (a tick's acts go out before its move), whatever the fall's speed. A use on the tick whose move lands is not too late. So a clutch pours by height (the eye 4.5 or less over the ground now), never by ticks before the landing ([[end-crystals]]).

## See also
- [[server-model]] - the model these rules join.
- [[parity-table]] - the rows that hold them.
- [[mc-water]] - water and lava as the game plays them.
