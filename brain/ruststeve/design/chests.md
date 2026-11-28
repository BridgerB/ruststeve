---
title: Chests
description: How ruststeve knows a chest (placed by the bot or seen exposed, never X-rayed), what it believes is inside (only what the chest's window showed), how the server model answers a chest's menu (ChestMenu.quickMoveStack from the 26.1.2 jar), and the Stash and Fetch tasks that move whole stacks and check each click by the window's slots (issue 77).
type: reference
tags: [chests, window, memory, sim, plan, inventory]
aliases: [chest, Chest, ChestSeen, Memory::chests, chest_contents, fold_chest, Window::block, used_on, ChestMenu, quick_move_target, Stash, Fetch, Transfer, generic_9x3, generic_9x6, isChestBlockedAt]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@bd9b55c
sourceRefs:
  - "ruststeve:crates/memory/src/lib.rs#pub struct ChestSeen {"
  - "ruststeve:crates/world/src/lib.rs#fn fold_chest(&mut self) {"
  - "ruststeve:crates/sim/src/chest.rs#pub fn quick_move_target(slot: usize) -> (std::ops::Range<usize>, bool) {"
  - "ruststeve:crates/sim/src/survival.rs#fn chest_quick_move(&mut self, i: usize, l: Loc, stack: Stack) {"
  - "ruststeve:crates/plan/src/stash.rs#pub struct Transfer {"
  - "ruststeve:crates/plan/tests/stash.rs#fn a_stash_then_a_fetch_round_trips_the_counts() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[entities-and-inventory]]"
  - "[[memory-sightings]]"
  - "[[smelting]]"
  - "[[table-window]]"
  - "[[wire-container-click]]"
---

# Chests

Issue 77. A chest lets the bot leave what it does not need yet and come back for it. It is the first block whose contents the bot keeps in memory, so the no-X-ray rule covers what is inside as well as where the chest is.

## Key files
- `crates/memory/src/lib.rs`: `ChestSeen`, `Memory::chests`, `chest_contents`, `chest`, `chests`.
- `crates/world/src/lib.rs`: `Window::block`, `World::used_on`, `fold_chest`.
- `crates/sim/src/chest.rs`: the block entity, `ChestMenu`, `quick_move_target`, `open_chest`. `crates/sim/src/survival.rs` sends a chest's menu through the same `Loc` routing the furnace uses.
- `crates/plan/src/stash.rs`: `Transfer`, `Stash`, `Fetch`. `crates/plan/tests/stash.rs` runs the tasks against the model.

## Known, never guessed
- `BlockKind::Chest` is one of memory's `WATCHED` kinds. A chest becomes known the way a table does: its cell is recorded once a face is exposed ([[memory-sightings]]). A chest the bot places counts too, through the block update that follows the place. When its cell is seen holding something else, the chest is forgotten.
- `ChestSeen { seen_tick, contents, opened_tick }`. `contents` stays `None` until the chest's window has been open, and then it is exactly what the window showed, summed by kind. `ChestSeen::count(kind)` gives `None` for a chest never opened, never 0.
- `Memory::chest_contents` turns away any cell memory does not already hold as a chest, so a window can never put a chest on record that was not first seen.

## The window says nothing of its block
`open_screen` carries only an id and a menu type. The world uses `World::used_on`, the block named by the last `Act::UseOn`, and `OpenScreen` stores it in `Window::block`. On every `container_set_content` and `container_set_slot` for a `generic_9x3` or `generic_9x6` window that has a block, `fold_chest` passes the window's own slots (all but the last 36) to memory. The window fold itself is unchanged: a chest window is 27 (or 54) slots followed by the inventory's 36, mirrored into window 0 as any window is ([[table-window]]).

## The server's rules (the 26.1.2 jar)
- `ChestMenu.quickMoveStack`: a slot below 27 moves to `27..63` in reverse (the inventory's end first). Any other slot moves to `0..27` forward (the chest's start first). Each pass fills stacks of the same item before empty slots. What does not fit stays in the clicked slot.
- `ChestBlock.isChestBlockedAt`: a redstone conductor over the chest stops it opening. The model and the task both count a full collision cube as a conductor.
- `ChestMenu.removed`: the chest keeps its slots and the cursor returns to the inventory. `stillValid` uses the same 8-block rule as the table.
- The model's chests are single. A double chest's `generic_9x6` is folded into memory, but the model does not build one.

## The tasks
`Stash` and `Fetch` wrap a single `Transfer` with a `Way`. Each is made only for a chest memory holds in the bot's dimension, and is `None` for any other cell.
- **Approach** to within reach (`Walk::to_block`), **Open** with `use_item_on` on the top face (a try every `OPEN_TICKS`, `MAX_OPENS` tries; the smelt's numbers), then **Settle** (the cursor put down), then **Move**.
- **Move**: one shift-click per round trip. Before each click the window must show room on the far side. A click that changes nothing gets no reply from the server ([[wire-container-click]]), so that click is never sent, and the kind ends there with "the chest is full" or "the inventory is full".
- **Checked by the slots**: after each answer, the chest's count of the kind is compared with its count before the click. That difference is what moved.
- **Whole stacks**: a stash never moves more than asked. It takes the largest stack that fits in what is left, and a stack bigger than the remainder stays held (the ledger says so). A fetch takes stacks until its count is reached, so the last one may bring more.
- **Close** when the task ends, done or failed.
- **Fails** with `Unopened`, `Blocked`, `Gone` (the chest left memory), `StaleWindow` or `TooManyClicks` (`MAX_CLICKS` 120).
- **Ledger**: `Transfer::notes` gets a line for every open (window id, slots, what it shows), every move (count, slot, the chest's new count), every kind that ends (none left, no room, a stack too big), every put-down, and every close (what moved against what was asked, and what the chest shows). The planner empties `notes` as it does a shield step's.

## Tests
- `memory` `a_chest_is_known_from_its_cell_and_its_contents_only_from_its_window`: a buried chest is never seen; a seen chest has no contents; a window's stacks are summed; a chest dug is forgotten.
- `sim` `quick_move_goes_chest_to_inventory_end_and_inventory_to_chest_start`: the jar's rule.
- `plan/tests/stash.rs`: a chest placed by `use_item_on`, then 74 cobblestone (64 + 10, never the other 64) and 3 string stashed and fetched back. The counts are checked in the model's chest, in the bot's pack and in memory. A second test fills the chest: 27 stacks go in and the task says the chest is full.

## Limits
- No real-server run yet. The probe planned for issue 77 places a chest, stashes, fetches and reads the counts back.
- The planner that crafts and places the home chest and decides what to stash and when is 77-stash's part (the planner section to come).
