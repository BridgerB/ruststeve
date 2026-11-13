---
title: Other windows and the crafting table
description: How the bot and the model handle a window besides the inventory - open_screen, the open window in the world (its slots, state id and the inventory mirrored into window 0's view), clicks and close going to the open window, and the model's CraftingMenu (3x3 grid, its quick-move ranges, closing puts the grid and the cursor back).
type: reference
tags: [window, crafting, table, clicks, model]
aliases: [open_screen, OpenScreen, Window, World::window, inventory_slot, CraftingMenu, crafting table window, 3x3 grid, Loc, container counter, stillValid, dropOrPlaceInInventory]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@34b2252 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/world/src/lib.rs#pub struct Window {"
  - "ruststeve:crates/world/src/lib.rs#pub fn inventory_slot(&self, slot: i16) -> Option<i16> {"
  - "ruststeve:crates/sim/src/survival.rs#fn open_table(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/survival.rs#fn quick_move(&mut self, i: usize, l: Loc) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/tests/table.rs#fn a_wooden_pickaxe_in_the_table_then_stone() {"
  - "clojurecraft:src/clojurecraft/packet.clj#[:play :s2c :open-screen] [[:window-id :varint] [:menu-type :varint] [:title :rest]]"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[crafting-window]]"
  - "[[wire-container-click]]"
  - "[[server-model]]"
---

# Other windows and the crafting table

Issue 11a: a window besides the inventory (window 0), first the crafting table's.

## Key files
- `crates/protocol/src/packets.rs`: `S2c::OpenScreen` (window id, menu type; the title skipped).
- `crates/world/src/lib.rs`: `Window`, `World::window`.
- `crates/sim/src/survival.rs`: the model's menus (`Loc`, `Table`, `open_table`, `tick_menu`); `crates/sim/tests/table.rs`.

## How it works
- **The world**: `open_screen` opens a `Window` (id, menu type, slots, size, state id); its content and slot packets fill it, and its last 36 slots (the player's main inventory and hotbar) are mirrored into window 0's view, so what is held stays right while a table is open. `Act::Click` and `Act::Resync` go to the open window with its state id; the pending click remembers its window; `Act::CloseWindow` closes it at once (the server answers nothing); a server `container_close` drops it.
- **The model** (`CraftingMenu`, read with `javap`): a table opens on `use_item_on` unless sneaking, with the next container id (1 to 100, round), `open_screen` and the whole menu. Slots: 0 the result, 1-9 the 3x3 grid, 10-45 the inventory (window 0's 9-44). The click rules are window 0's, over a slot map (`Loc`): the result from the generated recipes in a 3x3 grid; quick-move from the result into 10-45 from the hotbar's end, from the grid into 10-45, and from the inventory **into the grid first**, then main ↔ hotbar. Each menu keeps its own state id and remote slots.
- **Closing** (`removed`, `dropOrPlaceInInventory`): the grid and the cursor go back into the inventory (a live player keeps them; only what does not fit is dropped). 10b's model threw the cursor; the jar says otherwise, and window 0 now does the same. `stillValid`: more than 8 blocks from the table, or the table gone, closes it with `container_close`.

## Evidence
- Model tests: place, open, craft a wooden pickaxe in the 3x3 grid, close; closing puts the grid and the cursor back.
- Parity rows `t…-pickaxe` and `t…-stone` (place, open, craft a wooden pickaxe, or a stone pickaxe and a sword) match real ([[parity-table]]).

## Limits
- Only the crafting table's menu; furnaces are issue 13.

## See also
- [[crafting-window]] - the craft task.
- [[wire-container-click]] - the click and its replies.
