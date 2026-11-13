---
title: Smelting
description: How ruststeve cooks in a furnace (issue 13a) - the furnace window on the wire and in the world, the server model's furnace block entity and tick read from the 26.1.2 jar, the smelt task (fuel first, never more than the batch burns, coal before wood, a load judged by its slots), and the parity rows that hold the cook's pace.
type: reference
tags: [smelting, furnace, window, sim, plan, parity]
aliases: [furnace, Furnace, smelt, Smelt, SmeltStage, container_set_data, furnace window, cook, fuel, burn order, lit, smelt rows, half rows]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@3ee956a clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/sim/src/furnace.rs#pub fn tick(&mut self) -> bool {"
  - "ruststeve:crates/sim/src/survival.rs#fn furnace_quick_move(&mut self, i: usize, l: Loc, stack: Stack) {"
  - "ruststeve:crates/plan/src/smelt.rs#pub fn fuels_held(w: &World) -> Vec<(ItemKind, u32)> {"
  - "ruststeve:crates/bot/tests/smelt.rs#fn planks_burn_only_what_the_batch_needs() {"
  - "ruststeve:config/moments.toml#id = \"f00-half\""
  - "clojurecraft:src/clojurecraft/smelt.clj#(def cook-slack 5000)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[crafting-window]]"
  - "[[table-window]]"
  - "[[server-model]]"
  - "[[parity-table]]"
  - "[[wire-container-click]]"
---

# Smelting

Issue 13a. The furnace is the first window whose slots change on their own: the server cooks while the bot waits, and every rule below follows from that.

## Key files
- `crates/protocol/src/packets.rs`: `S2c::ContainerSetData` (window, property, value).
- `crates/world/src/lib.rs`: `Window::properties`; `closed_window`.
- `crates/sim/src/furnace.rs`: `Furnace` and its tick; `survival.rs`: the furnace menu (`Loc::Furnace`, 39 slots), opening, quick moves, properties, `tick_furnaces`.
- `crates/plan/src/smelt.rs`: the `Smelt` task.
- `crates/bot/tests/smelt.rs`; `config/moments.toml`'s `f*` rows (`smelt:<take>`).

## How it works
- **The window**: `open_screen` with the `furnace` menu; slots 0 input, 1 fuel, 2 result, 3-38 the inventory (window 0's 9-44). Properties by `container_set_data`: 0 lit time left, 1 lit total, 2 cook, 3 cook total.
- **The model's furnace** (`AbstractFurnaceBlockEntity.serverTick`, read with `javap`): a lit furnace burns a tick; lit, or with fuel and input, and a smelting recipe whose result fits: an unlit one lights from the fuel (`fuel_ticks`, one item taken, a lava bucket leaving its bucket), the cook rises and at the recipe's time (200) turns one input into the result; not able, the cook resets; neither lit nor fed, it cools by 2. A new input item restarts the cook with its recipe's time. The `lit` block state follows. It cooks with its window closed. `quickMoveStack`: the result to the inventory from the end; from the inventory, what smelts to the input, else fuel to the fuel slot, else between main and hotbar. Only fuel (or a bucket) goes in the fuel slot.
- **The smelt task**: approach and open as a table craft; settle the cursor; **fuel** first: the batch's cook time less what burns now and what sits in the fuel slot, from the hottest held fuel (coal, charcoal, a coal block, then planks; logs, sticks and tools are never burned), the fewest items that cover it, one per right-click; then the **input**; then **cook**: results are shift-clicked out as they show; done when the batch's results are held; past the batch's cook time by the slowdown and 5 s, `Slow`. One click per round trip; loads are judged by the window's slots, never its state id (the furnace's own changes move it).

## What the parity table found
- **The bot played at two thirds of a client's pace.** The half rows (take at script tick 330: one cook done, the second half way) disagreed: in real a 200-tick cook took 129 bot ticks. The loop waited for a packet up to a whole tick after each tick, so ticks drifted 50 to 100 ms apart; ticks now keep a fixed 20 Hz schedule ([[tick-loop]]). Every real run before issue 13a walked, dug and clicked at about 0.65 of a vanilla client's pace.
- **A late answer to a click in a closed window.** A take's answer came over two ticks (the result slot emptied, then the inventory slot filled 30 ticks later); the script had closed the window between them and the world dropped the second. The world keeps the last closed window and still puts what its answers bring into the player's slots ([[wire-container-click]]).
- Then the 9 rows (coal, planks and half, on landings 0-2) **9 of 9** locally.

## Limits
- Experience from a take is an orb entity, not in the outcome class.
- A furnace broken with items in it does not drop them in the model.
- The batch left cooking is remembered by the planner and collected first ([[fuel-and-smelt-needs]]).

## See also
- [[crafting-window]] - the click rules the smelt task shares.
