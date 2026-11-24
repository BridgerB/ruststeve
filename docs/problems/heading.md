# The heading (issue 17d)

The way from the fortress to a warped forest: a long walk out on one compass axis, a leg of 48 blocks at a time. Agent E2's wall file; the index is `docs/problems/endermen.md`.

## Needs
- A bot that has left the blaze spawner with 7 rods holds a direction for hundreds of blocks (the plan's sizing: forests 170 to 485 blocks from the fortress in clojurecraft's sim Nethers) without walking in squares, pacing a corridor or spending its pickaxes.
- The starting kit (the `rods_7` snapshots): 2 stone pickaxes (at most 262 uses, wear unknown), about 620 filler, no wood, no table. Pickaxe uses are the scarce resource; filler is not.

## Risks
- clojurecraft's three ways to burn a trip (`brain/oracle/handoff/cc-stage18-lessons.md`): tunnels turned on a 30 s clock dug squares (877 of one bot's 1,244 digs); legs paced a 1-wide causeway; filler spent sealing and unsealing until there was none to bridge with.
- Lava (the sea at y 31, falls, pockets), drops into the sea from ledges (11 and 40 blocks), piglins and hoglins (no gold armour: hostile), ghasts in the open, wither skeletons.
- One world: every frontier race is the same seed and the same heading, so what judges a fix is one place.

## Done, in world terms
The heading carries a frontier-started bot out of the fortress area toward unseen columns with no square, no pacing (0 "paced" refusals beyond a counted few) and its spend per metre in every leg's note, until a warped forest is at its feet or known from blocks seen (then issue 17e's endermen task takes over).

## The build (crates/plan/src/heading.rs)
- **Choice, once, held**: the axis pointing most away from the centroid of the columns seen (the fortress and the portal trail are in them); a tie (centroid within 16 blocks, or two axes level) to the fewest seen columns 7 to 16 chunks ahead, 2 either side. A forest known from warped blocks seen replaces the axis: legs run to it. Never anything from RCON or an unseen column's biome. Note "heading chosen".
- **Legs**: 48 ahead (`Goal::Ahead`), the target's height clamped to 40 to 90. Each leg tries, in order: a plain walk; the far walk bridging with 32 kept (`Cells::far_keeping`); the travel tunnel only with pickaxe uses over the reserve (40) plus 2 a metre left on the leg. A way that ends short is tried again while it gains, twice without gaining moves to the next. Notes "heading leg N toward", "heading leg N by walk / far walk", the tunnel's own notes, "heading leg N done / failed (why) / cut short" with gain, digs, placed and uses, each per metre.
- **Invariants**: a quarter turn only after a failed leg (toward the side with fewer seen columns, alternating on a tie), a fresh projection from the feet, the cut kept; after 3 turns with none gaining 48 the axis is re-chosen (never the one just failed), and after 4 choices the heading is turned out. `best` must grow 6 every 1,200 server ticks (`NoProgress`). A leg's routes never enter a cell the heading cut or stood in at or behind the feet; a way only back over them is noted "refused for cut (paced)". Over 3 digs or 1 block placed a metre (past 8 metres) ends the leg. Notes "heading turned to", "heading re-chosen", "heading turned out".
- **Biome signs**: the feet's biome crimson forest or basalt deltas turns the axis once on entering; crimson or basalt signs seen 1 to 4 chunks ahead (12 or more) turn it; a warped forest at the feet ends the heading ("heading ended: warped forest").
- **Filler**: under 32 before a leg, exposed netherrack wall cells within reach are dug (at the feet's height to 3 over, no lava beside, nothing over that falls, never the floor) until 48 or 24 digs; "restock dig", "restock over" with blocks gained and uses spent. A restock that found no wall cell waits 1,200 ticks.
- **Stubs until merge** (agreed signatures): `Memory::seen_columns` (E1; the stub reads the loaded columns), `Memory::forest_known`, `Memory::biome_signs_in_column` and `World::feet_biome` (E1; the stubs know nothing), `crate::endermen::pickaxe_uses_left` and `rules.pickaxe_reserve` (E3; the stubs are `dig_budget` and 40).
- **Not built here**: the ghast's cover (a short tunnel into the wall when a ticking ghast sees the bot) is the always-rows' and E3's cover; wither skeletons are the defend rules' `never_fight`.

## Unit tests (crates/bot/tests/heading.rs)
Open ground 200 blocks with no turn; a wall tunnelled on the axis at 2 digs a metre; the dead-end causeway turned from once and never walked back (the far walk bridges north instead of the plain walk back over the cut); the way only back over the cut refused and said ("paced"); a lava sea bridged with the keep held after a restock from filler 20; a leg with no gain failing `NoProgress` at 1,200 ticks; a crimson forest at the feet turning once; a known forest replacing the axis and a warped forest at the feet ending it; the axis choice and the turn's side.

## Walls
(None raced yet.)
