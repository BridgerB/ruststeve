---
title: The parity table
description: How ruststeve checks the server model against a real server before any sim number counts - captured worlds, the 234 moments (walks, digs, crafts, stone and the table, air, fire, lava, eating and melee, smelting, buckets, endermen, the burrow, eyes of ender, the End's rows), running them in real and in the model, the outcome classes compared field by field, the stated tolerance, what the table found, and what it leaves out.
type: reference
tags: [parity, sim, measurement, capture, moments, real]
aliases: [end rows, e03-strike, crystal script, lift, summon_nbt, report rows, eye rows, y01-throw, y02-fetch, stronghold field, eye class, burrow rows, e04-mouth, fill rows, trace, enderman rows, frozen row, stare row, difficulty rows, drown rows, fire rows, lava rows, eat rows, cow rows, breathe, dig rows, craft rows, give, random drops, parity table, parity report, moments, capture, captured world, landings, outcome class, tolerance, moments compare, parity.yml, simrace, simrace.yml]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4
sourceRefs:
  - "ruststeve:crates/bot/src/bin/capture.rs#fn capture(a: &Args) -> Result<(), String> {"
  - "ruststeve:crates/bot/src/bin/moments.rs#/// script saw, so when a peaceful heal lands (every 20 ticks) cannot flip it."
  - "ruststeve:crates/results/src/parity.rs#pub fn compare(real: &[MomentResult], sim: &[MomentResult]) -> Vec<Row> {"
  - "ruststeve:crates/bot/src/bin/simrace.rs#fn simrace(a: &Args) -> Result<bool, String> {"
  - "ruststeve:config/moments.toml#id = \"m00-0\""
  - "ruststeve:config/moments.toml#id = \"d00-late\""
  - "ruststeve:config/moments.toml#id = \"e00-stare\""
  - "ruststeve:crates/bot/src/script.rs#pub fn acts(self, tick: u64, w: &World) -> Vec<Act> {"
  - "ruststeve:.github/workflows/parity.yml#./target/release/moments compare parity/real parity/sim --report parity/parity.md"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[server-model]]"
  - "[[measurement-rules]]"
  - "[[cc-stage18-lessons]]"
---

# The parity table

A sim number counts only after the model agrees with a real server at the same moments ([[measurement-rules]]): clojurecraft's sim said 30% where real said 3% for weeks ([[cc-stage18-lessons]]). Issue 08c builds the table for what the model models now; each later issue adds moments for its part.

## Key files
- `crates/bot/src/bin/capture.rs`, `src/captured.rs`: captured worlds.
- `crates/bot/src/bin/moments.rs` (`real`, `sim`, `compare`), `crates/results/src/parity.rs`.
- `config/landings.toml` (11 landings), `config/moments.toml` (234 moments).
- `.github/workflows/parity.yml`; `crates/bot/src/bin/simrace.rs` and `simrace.yml` for sim races.

## How it works
- **Capture**: a spectator joins a fresh server on the race seed, is teleported over each landing by RCON, reports its position once (the server re-centres chunks only on a reported move) and keeps every `level_chunk_with_light` body within 4 columns: one file per landing, byte for byte. A spectator changes nothing, so the terrain is what bots meet.
- **A moment**: a landing, a start offset (the start stands on the captured surface there), a script (`still`, `straight:<yaw>`, `jump:<yaw>`), 160 ticks. Five per landing, six blocks apart, each walking away from the centre (vanilla players push each other).
- **Real**: the same fresh server, peaceful, bots in waves of 10, each with a fresh name (the server saves a player's place and health by name); RCON gives water breathing and teleports to the start; the script begins after 20 physics ticks with the ground loaded and no teleport since.
- **Sim**: the model in the captured world, the bot spawning at the start, in lockstep, the same settle and script.
- **Dig and craft rows** (issue 10b), after the walks so the real world's holes come after every walk row: per landing a dig of the block under the feet with FINISH at tick 30 (`dig:30`) and at tick 5 (`dig:5`, a delayed destroy), and a craft (`craft:planks|sticks|table`) from items a row `give`s (`/give` by RCON in real, `Survival::give` in the model). A script now also acts (`Script::acts`): a craft sends its next click only once the last moved the window's state id.
- **The outcome class**, compared field by field: the final block cell, on the ground, in water, corrected (a teleport after the script began), the lowest health during the script (full, hurt, dead), and since 10b the inventory by item name (slots 1-45 and the cursor) and the count in the grid, result and cursor.
- **Random drops are not a class**: an item some block drops only by chance and no recipe makes (a sapling from leaves, a seed from grass, flint from gravel) is left out of the inventory on both sides; sticks, which a dead bush drops by chance, stay in (11a found them left out). The model's roll is seeded, not vanilla's, and **a fresh server grows different canopies on the same seed** from run to run (two runner captures disagree about leaves at landings 4 and 7), so no dig spot can be promised leaf-free.
- **Stone and table rows** (issue 11a), on the five dry landings (digging and placing from water is issue 12's): a moment may `set` blocks at the start (`setblock` by RCON in real, retried until the chunk is loaded, which right after a teleport it often is not; into the model's world in sim). Stone under the feet dug by hand (the delayed destroy at 150 ticks, no drop) and with a given wooden pickaxe (late and early FINISH); a table placed east of the feet on laid stone, opened, and a wooden pickaxe crafted, or a stone pickaxe and a sword.
- **Survival rows** (issue 12a), on the dry landings: a moment may say `breathe = false` (no water breathing), `effects` (`effect give` in real, the model's `Life` in sim) and `summon` (a mob with no AI, no loot and kept). Drowning in a capped water shaft, standing in fire on netherrack, lava in a stone cup, a honey bottle drunk after a hunger effect (`eat:220`), and a stone sword against a cow two blocks east (`attack:13:270`, a swing each 13 ticks, the sword's full charge being 12.5). The class gains the food the server last sent and the bot's kills (deaths of entities it struck).
- **The stated tolerance**: all but 2 of the moments match in every field (158 of 160, `TOLERANCE` in `moments.rs`; 150 of 152; 126 of 128; 117 of 119; 103 of 105; 78 of 80 with 10b; 48 of 50 before). A mismatch is a model or harness bug until shown otherwise, never tuned away.
- **simrace**: lanes of lockstep runs in generated worlds, the same case and batch files with `source = sim`, released as `race-sim-<run>` so the insight store keeps sim beside real, never mixed.

## What the table found
The first table matched 19 of 50. Each cause was named from a recording, then fixed:
1. **A deadlock in the bot**: teleported far from its loaded columns, a body with no ground ran no physics, sent no move, so the server never re-centred its chunks. The world now sends its once-a-second status even without ground ([[physics]]).
2. **Peaceful regeneration** was missing from the model (`ServerPlayer.tickRegeneration`: 1 every 20 ticks below full).
3. **Settle jitter**: the settle before a script counted server ticks, which the bot learns 20 at a time, and began before the start's teleport landed; now 20 physics ticks with ground loaded since the last teleport, the same in real and lockstep.
4. **Drowning while waiting**: real bots sat under water while their chunks came; the model has no air supply yet, so the real side breathes (below).
5. **Saved players**: a reused bot name rejoined where the last run left it, hurt.
Then: **50 of 50** locally.

Issue 10b's 30 rows first matched 28: the bot's world kept a stale cursor (a click claims an empty cursor and the server answers only a difference, so the world now forgets its cursor as it clicks, [[wire-container-click]]); two leaf rows differed by a sapling roll. Then a grass row's leaf litter popped off in real only: the model now breaks what rests on a broken block. Then **80 of 80** locally; on a runner 79 of 80, the miss a sapling on a canopy the local world did not have, which led to leaving chance drops out of the class.

Issue 11a's 25 rows first matched 15 locally: `setblock` landed before the chunk loaded (retried now), the model judged the eyes in water by the cell alone (now by the fluid's height), a scripted click changed nothing and the craft stalled, and water starts differed (the rows moved to dry landings). Then 25 of 25, and 105 of 105 with the earlier rows.

Issue 12a's 14 rows first differed only in kills: the world counted every death in view (another bot's cow, a bot in lava), so it now counts only entities it struck; and one cow row was kicked (`invalid_entity_attacked`) for swinging at the cow's experience orb, so the script strikes only mobs, and on a runner it struck wild animals that wandered within reach after the cow died (kills 3), so it strikes only at the summon spot. Eating bread could not be a row on peaceful, which keeps the food full: the rows drink a honey bottle, which can always be drunk. Then 14 of 14 locally.

Issue 12b's 9 escape rows run the planner itself (`script = "plan:hold"`: `Aim::Hold`, the always-rows only, on both sides, capped at the row's ticks after the settle): a stone-walled pond, a lava cup and a fire block. The lava rows first died in real only, sitting in lava through the start's setblocks and the settle; they now carry fire resistance (modelled: fire, lava and burning do no damage) and check the flee. Then 9 of 9 locally ([[escapes]]).

Two walk rows that fell and were hurt (m00-3, m06-1) missed on every runner after 12a: the world now takes the server's `set_entity_motion` for itself as its velocity, and the real server sends one when the player is hurt ((0, -0.0784, 0) and (0, -0.2305, 0), one and three ticks of gravity), which the model did not. The model now sends it ([[server-model]]).

Issue 13a's 9 smelting rows (`smelt:<take>`, [[smelting]]) found the bot's tick loop running at about 0.65 of a client's pace, and a click's answer arriving after the window closed; both fixed, then 9 of 9 locally.

## Limits
- The server runs peaceful, which removes hostile mobs: zombie rows (their attacks and knockback) need a non-peaceful row and come with defence (12d). Rows other than the drowning ones still breathe.
- Moments test movement, falls, the server's corrections, digging by hand, drops and pickup and the 2x2 grid; tools, placing and other windows add moments with their issues.
- A dig row checks whether the block broke by the row's end, not the tick it broke on.

## Issue 14a
- Rows `w00` to `w02` (scoop, pour, lava, obsidian, portal: 15 rows, on landings 0 to 2) and the `watch` field: cells from the feet cell whose blocks join the outcome class as `blocks`, by name ([[buckets-and-fluids]]). Scripts `use:<tick>:<yaw>:<pitch>` (the held item used along a look) and `light:<tick>:<dx>:<dy>:<dz>:<face>` (used on a block's face).

## Client parity rows (issue 16)
- Rows `p00` and `p01` (eight, on landings 0 and 1, each in a corridor laid from the feet; `docs/problems/client-parity.md`): a walk over magma (hurt), the same floor sneaked (full: only a server told the shift key by `player_input` spares the feet), a sneak to the edge of a pit (the body stops on the last block), and a walk along soul sand (slowed, not corrected). Scripts `sneak:<yaw>` and `crouch`.
- What these rows cannot show: a server never simulates a client's speed, so the walk's pace on soul sand or while sneaking is the same on both sides whatever it is. The pace is checked against the game's own formula in `crates/physics`' unit tests; the rows show the server accepts it.

## Enderman rows (issue 17)
- Rows `e00` to `e03` (`docs/problems/endermen-model.md`), each at its own landing 40 south of the centre: an enderman two above a floor 64 east (4 chunks: beyond the simulation distance) stays where it was put; one 30 east falls (it ticks); one penned 12 east and looked in the eye turns creepy; the same look 15 degrees aside leaves it neutral.
- A row may name a `difficulty` (`normal`): those rows run after the others, the real server's difficulty set by RCON for them and put back to peaceful after (an enderman is a monster: peaceful removes it at once); the model runs them off peaceful. `mobs` places mobs with their AI (`summon` kept and lootless in real, the model's own brain in sim; the model places endermen only).
- The class gains `mobs`: for each placed mob, the entity of its kind nearest where it was put in the bot's world: seen, moved more than half a block, and creepy (entity data 17).
- Tolerance 162 of 164. The first run (parity 45483333265, 764d798): **164 of 164**, the four enderman rows matching on both sides: frozen still at 64, ticking and fallen at 30, creepy under the stare, neutral 15 degrees aside.

## Burrow rows (issue 18)
- Rows `e04` to `e08` (`docs/problems/pearls-model.md`) prove the pearl hunt's burrow on a real server before any race: a level burrow one wide and two high of stone, B the bot's cell, M the mouth, O outside where the enderman stands. `e04-mouth` (it comes to O, the bot in B is never hit), `e05-strike` (struck on a full charge from B, it dies; loot kept), `e06-roof` (looked in the eye from B at O, M's roof cuts the line: not stared, not creepy), `e07-pickup-b` and `e07-pickup-m` (a pearl at O is out of reach from B, in reach from M), `e08-near` (the bot in M is hit).
- A row may `fill` boxes before its `set` (`fill` by RCON, each cell in the model), keep its mobs' `loot`, and lay `items` on the ground (`summon item` with no pickup delay; `Sim::place_item`). Script `burrow:<mode>` (`hold`, `strike`, `eye`, `near`) reads the world each tick (`Script::steer`): the stare until creepy, the step into B, the torso look, the strike.
- The class's `mobs` gains `stared` (entity data 18) and `cell` (where its feet ended). A row with mobs or items writes `<id>.real.trace.txt` (folded from its recording) and `<id>.sim.trace.txt`: the bot and every enderman and item within 32 every 10 ticks, for reading what the enderman did.
- Tolerance 168 of 170.

## Eye rows (issue 19)
- Rows `y01-throw-N` (landings 0 to 3 and 8) and `y02-fetch-N` (landings 0, 4, 5) (`docs/problems/eyes-model.md`), on the peaceful table, 40 off their landing's centre, the bot given one eye. A row's `stronghold = [x, z]` is the point RCON's `locate structure minecraft:stronghold` names from the landing (the model is told it; the real line is judged against it). `use:20:0:-20` throws looking up; `fetch:<tick>` (`Script::Fetch`) throws, then walks to the dropped eye.
- The class gains `eye`: seen, on the line (the line from its first place to its farthest on the plane within 1 degree of the bearing to the stronghold), ended, and for a fetch row none left on the ground. Drop or shatter is the jar's 4 in 5 roll: an ender eye held is out of the inventory class, and the end is written in `<id>.<real|sim>.eye.txt` with every place, the line, its error and the flight's numbers.
- Tolerance 176 of 178; then 179 of 181 with the portal room rows (`y04` differs until silverfish are modelled: `TOLERANCE` 179). The End rows below have not moved the constant: they run with parity.yml's `only` and `tolerance` inputs.

## End rows (issue 20)
- Rows `e01-frame` to `e06-drop` (`docs/problems/end-model.md`), development setups laid by RCON in the overworld (never evidence), 40 off their landing's centre, at Normal, in full iron with boots. They measure the End's dangerous geometry before any race: an eye into a frame and the portal lit, the portal's arrival, a crystal struck by reach through a wall or its cage, the enderman's stare cone, the perched dragon's head and its blow, the water clutch.
- New row fields: `lift` (the start stands that many over the surface; the real side stands it on the ground first, lays the row, then lifts it), `summon_nbt` (more NBT for the real `summon`s: a dragon's phase and yaw), `report` (writes `<id>.<real|sim>.end.txt`: health at each change, hits, blasts, crystals, dragons and endermen with their health, the dimension, the watched cells' full block states). `set` and `fill` take a block with properties as a command does (`iron_bars[north=true,south=true]`).
- Scripts `crystal:<every>` (strike the nearest crystal by reach), `dragon:<every>:<part>` (strike the dragon's part id + part: the server judges the part's box) and `clutch:<yaw>:<walk>:<above>` (off a pillar, pour the water bucket falling).
- Rows `e09-pour-D-K` (E4's clutch window): e06's fall onto end stone from D = 8, 15, 25, 40, 60, with one use only (`pour:<yaw>:<walk>:<before>`), on the tick the body's own fall predicts a landing `before` ticks on, or (0, `-reach`) on the first tick the eye is within the bucket's 4.5 of the ground; `-drop` has no bucket. Their real.end report gives, per use, the position it went from (a tick's acts go out before its move, so it is the last position the server heard), the eye over the ground there, the fall's ticks, the landing, the watched cells turning to water and the health lost.
- Rows `e07-bow-24` and `e07-bow-40` (E8's bow line, `docs/problems/end-bow.md`): an end crystal on bedrock over an obsidian column 24 or 40 east of a bot on a stone floor with a bow and arrows; script `bow:<at>` aims by `plan::bow::aim` (the jar's flight, the low arc through the box's middle), uses the bow at tick `at` and lets go `DRAW_HOLD` (24) ticks later; the real.end report says whether the crystal was removed and what its blast did at that range.
- `moments --only <prefixes>` and parity.yml's `only` and `tolerance` inputs run a few rows in minutes instead of the table's hour.

## See also
- [[server-model]] - what the model models.
- [[measurement-rules]] - why the table gates sim numbers.
