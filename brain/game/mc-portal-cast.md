---
title: Casting the Nether portal
description: How every bot builds the Nether portal without a diamond pickaxe (casting obsidian from lava and water in a mould), and the rules each learned.
type: reference
tags: [game, portal, obsidian, nether]
aliases: [portal cast, obsidian cast, mould, mold, lava bucket, build plan]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f ruststeve@201dfcfe
sourceRefs:
  - "clojurecraft:src/clojurecraft/build.clj#casts the frame as a player does (issue #124): the rock is the mould."
  - "clojurecraft:src/clojurecraft/build.clj#a dig never takes obsidian (ruststeve dug its own frame and restarted forever)."
  - "clojurecraft:src/clojurecraft/build.clj#Nothing about progress is stored: the op to work on is the first one not satisfied, so a build resumes after a death, a chunk reload or a relaunch by reading the world again."
  - "ruststeve-v1:CLAUDE.md#The portal is cast, not mined:"
  - "ruststeve:crates/plan/src/portal_shape.rs#pub fn any_shape("
  - "ruststeve:crates/plan/src/portal_shape.rs#pub fn find_empty("
  - "ruststeve:crates/plan/tests/portal_shape.rs#fn the_cast_plan_builds_a_frame_portal_shape_accepts() {"
  - "ruststeve:crates/sim/src/fluid.rs#fn spread_or_convert(&mut self, p: Pos)"
  - "ruststeve:crates/sim/src/fluid.rs#let obsidian_beside ="
related:
  - "[[game/_moc|Game]]"
  - "[[hist-walls]]"
  - "[[cc-goals-and-intents]]"
---

# Casting the Nether portal

No bot mines obsidian; that needs a diamond pickaxe. Each casts it: lava poured into a cell, then water poured onto the lava source, turns it to obsidian. Ten cells make a frame, lit with flint and steel.

## How it works (clojurecraft)
- The frame's ten cells are holes cut a block deep into a rock face beside any lava pool, at any depth: the rock is the mould. Lava is poured into a hole, water into the cell directly over it (it lands on the source: obsidian) and scooped straight back; the lava bucket is refilled at the pool.
- The build is a plan of ops as data (`:place :dig :pour :scoop :use-on :stand-in`), each with a done predicate over the world. Nothing about progress is stored: the op worked on is the first not satisfied, so a death or relaunch resumes by reading the world.
- A dig never takes obsidian.

## Measured from the 26.1.2 jar (issue 83)
Read with `javap -c -p -constants` from the server's inner jar (`versions/26.1.2/server-26.1.2.jar`, probe 46139819290, 2026-11-28). Each line names its class and method.

**Water meeting lava** (`LiquidBlock.shouldSpreadLiquid`, called from `LiquidBlock.onPlace` and `neighborChanged` of the lava block):
- A lava block reads the fluid at `pos.relative(d.getOpposite())` for `d` in `POSSIBLE_FLOW_DIRECTIONS` = (DOWN, SOUTH, NORTH, EAST, WEST): so the cell **over** it and its **four sides**, in the order up, north, south, west, east. Water **under** lava is never read.
- Any water there (`FluidTags.WATER`: a source or flowing) turns the lava to **obsidian when the lava is a source** (`FluidState.isSource`), **cobblestone when it flows**; `setBlockAndUpdate`, then `fizz` (level event 1501).
- Over soul soil with blue ice beside: basalt instead (not the cast's case).
- `LavaFluid.spreadTo`: lava flowing **down** into water makes **stone** in the water's cell.
- So the cast's pour (water into the cell over the lava source) is the cleanest case: one read, a source, obsidian. Water beside the hole at its height works too; water under the hole never does.

**The frame** (`PortalShape`):
- `MIN_WIDTH` 2, `MAX_WIDTH` 21, `MIN_HEIGHT` 3, `MAX_HEIGHT` 21: the inside's size (`isValid`).
- `FRAME` (`lambda$static$0`): `state.is(Blocks.OBSIDIAN)`, nothing else: crying obsidian is not frame.
- `isEmpty`: `isAir` (air, cave air, void air), the `fire` tag (fire, soul fire) or a nether portal block.
- `findAnyShape(level, pos, axis)`: `rightDir` WEST on X, SOUTH on Z. `calculateBottomLeft`: down from `pos` while the cell under is empty, to at most 21 under (`max(getMinY, y - 21)`); then `getDistanceUntilEdgeAboveFrame` toward the left (the opposite of right) minus 1 (none: no shape).
- `getDistanceUntilEdgeAboveFrame` (0 to 21 steps): a non-empty cell ends the run, its distance if it is frame, else 0; an empty cell whose cell under is not frame ends it with 0. `calculateWidth`: that distance toward right, kept only within 2 to 21.
- `getDistanceUntilTop` (rows 0 to 20): a row ends the count when the frame left of it (`rightDir`, -1) or right of it (`width`) is not frame, or one of its cells is not empty; a nether portal block counted on the way (`numPortalBlocks`). `calculateHeight`: kept only within 3 to 21 and when `hasTopFrame` (frame over every top cell).
- **The corner rule**: the four corners are never read. Ten obsidian (two under, two over, three each side) make the smallest frame; corners change nothing.
- `findEmptyPortalShape`: valid and no portal block in it (`lambda$findEmptyPortalShape$0`). `findPortalShape` asks the preferred axis, then the other.
- `createPortalBlocks`: `NETHER_PORTAL` with `AXIS` the shape's axis, set (flags 18) in the box from `bottomLeft` to `height - 1` up and `width - 1` right. `isComplete`: valid and `numPortalBlocks == width * height`.
- `NetherPortalBlock.updateShape`: a change from a side on the portal's own axis or from above or below, by a block that is not portal, turns the portal to air unless `findAnyShape` there is still complete. A change in front of or behind the plane never breaks it.

**Flint and steel and the light**:
- `FlintAndSteelItem.useOn`: a campfire, candle or candle cake that can light is lit (`LIT`). Else the fire cell is `clickedPos.relative(clickedFace)`; if `BaseFireBlock.canBePlacedAt(level, it, ctx.getHorizontalDirection())`, `BaseFireBlock.getState` (soul fire on soul soil or soul sand, else fire) is set there (flags 11) and the item takes 1 damage (`hurtAndBreak`); else the use fails.
- `canBePlacedAt`: the cell must be air; then the fire must survive there (`FireBlock.canSurvive`: the block under sturdy on its top face, or a burnable neighbour) **or** `isPortal`.
- `isPortal`: only in the Overworld or the Nether (`inPortalDimension`); obsidian on one of the fire cell's six sides; then `findEmptyPortalShape` with the axis `getHorizontalDirection().getCounterClockWise().getAxis()` (random for a vertical direction). Both axes are tried, so the facing only orders the search.
- `BaseFireBlock.onPlace`: in the Overworld or the Nether, `findEmptyPortalShape(level, pos, X)` (X first, then Z); found, `createPortalBlocks` and the fire is gone into the portal. Else a fire that cannot survive is removed.
- So the cast's light (flint and steel on the top of a bottom frame cell, the fire in the inside's bottom cell, on obsidian: sturdy, so it stands) lights the frame whenever `PortalShape` accepts it.

**ruststeve's port and the sim**:
- `crates/plan/src/portal_shape.rs` is `PortalShape` as pure code over a function from cell to block (`any_shape`, `find_empty`, `Shape::inside`, `Shape::frame`). `crates/plan/tests/portal_shape.rs` holds vanilla's cases and the done condition: `build::frame_plan` worked op by op on a hand-built rock with a pool builds a frame it accepts, every `Facing`.
- The sim (`crates/sim/src/fluid.rs`) already had `shouldSpreadLiquid` with the same five neighbours and order, the stone rule and `PortalShape`. It differed in `isPortal`: a frame round an unsupported fire cell lit with no obsidian beside it; now it needs obsidian on a side. It still lights in the End (no `inPortalDimension`) and has no burnable-neighbour fire; neither touches the cast.

## Rules the earlier bots paid for
- ruststeve v1 dug its own frame and restarted forever; never dig obsidian.
- v1 cast with a template mould, a sealed scoop station for refills, and per-bot frame anchor files; steve's frame order and lighting are in its code ([[hist-walls]]).
- Lava refills and lava deaths at the frame were the dominant costs in both earlier bots.

## See also
- [[hist-walls]] - the portal as a wall.
- [[cc-roadmap-stages]] - the portal stage's last failure.
