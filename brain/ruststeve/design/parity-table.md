---
title: The parity table
description: How ruststeve checks the server model against a real server before any sim number counts - captured worlds, the 128 moments (walks, digs, crafts, stone and the table, air, fire, lava, eating and melee), running them in real and in the model, the outcome classes compared field by field, the stated tolerance, what the table found, and what it leaves out.
type: reference
tags: [parity, sim, measurement, capture, moments, real]
aliases: [drown rows, fire rows, lava rows, eat rows, cow rows, breathe, dig rows, craft rows, give, random drops, parity table, parity report, moments, capture, captured world, landings, outcome class, tolerance, moments compare, parity.yml, simrace, simrace.yml]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@b86d924
sourceRefs:
  - "ruststeve:crates/bot/src/bin/capture.rs#fn capture(a: &Args) -> Result<(), String> {"
  - "ruststeve:crates/bot/src/bin/moments.rs#/// script saw, so when a peaceful heal lands (every 20 ticks) cannot flip it."
  - "ruststeve:crates/results/src/parity.rs#pub fn compare(real: &[MomentResult], sim: &[MomentResult]) -> Vec<Row> {"
  - "ruststeve:crates/bot/src/bin/simrace.rs#fn simrace(a: &Args) -> Result<bool, String> {"
  - "ruststeve:config/moments.toml#id = \"m00-0\""
  - "ruststeve:config/moments.toml#id = \"d00-late\""
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
- `config/landings.toml` (10 landings), `config/moments.toml` (105 moments).
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
- **The stated tolerance**: all but 2 of the moments match in every field (126 of 128; 117 of 119; 103 of 105; 78 of 80 with 10b; 48 of 50 before). A mismatch is a model or harness bug until shown otherwise, never tuned away.
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

## Limits
- The server runs peaceful, which removes hostile mobs: zombie rows (their attacks and knockback) need a non-peaceful row and come with defence (12d). Rows other than the drowning ones still breathe.
- Moments test movement, falls, the server's corrections, digging by hand, drops and pickup and the 2x2 grid; tools, placing and other windows add moments with their issues.
- A dig row checks whether the block broke by the row's end, not the tick it broke on.

## See also
- [[server-model]] - what the model models.
- [[measurement-rules]] - why the table gates sim numbers.
