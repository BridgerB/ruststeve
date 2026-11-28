---
title: Chests
description: How ruststeve knows a chest (placed by the bot or seen exposed, never X-rayed), what it believes is inside (only what the chest's window showed), how the server model answers a chest's menu (ChestMenu.quickMoveStack from the 26.1.2 jar), and the Stash and Fetch tasks that move whole stacks and check each click by the window's slots (issue 77).
type: reference
tags: [chests, window, memory, sim, plan, inventory]
aliases: [chest, Chest, ChestSeen, Memory::chests, chest_contents, fold_chest, Window::block, used_on, ChestMenu, quick_move_target, Stash, Fetch, Transfer, generic_9x3, generic_9x6, isChestBlockedAt]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@4936cc2
sourceRefs:
  - "ruststeve:crates/memory/src/lib.rs#pub struct ChestSeen {"
  - "ruststeve:crates/world/src/lib.rs#fn fold_chest(&mut self) {"
  - "ruststeve:crates/sim/src/chest.rs#pub fn quick_move_target(slot: usize) -> (std::ops::Range<usize>, bool) {"
  - "ruststeve:crates/sim/src/survival.rs#fn chest_quick_move(&mut self, i: usize, l: Loc, stack: Stack) {"
  - "ruststeve:crates/plan/src/stash.rs#pub struct Transfer {"
  - "ruststeve:crates/plan/tests/stash.rs#fn a_stash_then_a_fetch_round_trips_the_counts() {"
  - "ruststeve:crates/plan/src/stash.rs#pub fn stash_gate(&mut self, w: &World, chosen: Task, tick: u64) -> Task {"
  - "ruststeve:crates/plan/src/stash.rs#pub fn stash_list("
  - "ruststeve:crates/plan/src/stash.rs#pub fn fetch_list("
  - "ruststeve:crates/plan/src/stash.rs#fn home_chest_task(&mut self, w: &World, tick: u64) -> Result<Task, String> {"
  - "ruststeve:config/stash.toml#name = \"nether\""
  - "ruststeve:config/goals.toml#id = \"home-chest\""
  - "ruststeve:crates/plan/tests/stash_planner.rs#fn before_the_nether_the_home_chest_is_made_and_the_surplus_for_the_leg_stashed_then_fetched()"
  - "ruststeve:config/moments.toml#id = \"k00-half\""
  - "ruststeve:crates/results/src/parity_notes.rs#pub fn check(moments: &[Moment], results: &[MomentResult]) -> Vec<String> {"
related:
  - "[[keep-rule]]"
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

## The planner
`Plan::stash_gate` is the one call site. It runs as the planner chooses an idle task (`step_inner`) and takes the task chosen, so every leg's own planner stays as it is.
- **The legs** are data, in `config/stash.toml` (compiled in): each leg's name, the dimension it is entered from, its portal block, and the stages it spans. The Nether runs `nether` to `pearls_12`, and the End runs `end` to `win`. Item counts are never written there: `config/keep.toml` alone says how many of an item a stage needs, through `Keep::need_in(kind, stages)` (the largest count any row names in the span, including the `during` rows that only need_in reads; [[keep-rule]]). The `Needs` trait is that call, implemented by `Keep`. The planner asks `Plan::keep.table`, and a table that could not be read stashes and fetches nothing, noted.
- **The trigger**: the task chosen is an `Enter` from the leg's dimension into its portal block, and the trigger has not yet run for that leg (`StashState::legs`). It runs once per leg, and every outcome writes a note: the stash begun, nothing to put away, skipped for danger (under `danger_health`, or a foe in range: the stash is a trip, never made under danger), skipped with no home chest, or skipped with no keep table. Then the `Enter` goes ahead.
- **What is stashed** (`stash_list`): each kind in the pack (slots 9 to 44, what a chest window shows) that a stage after the leg needs. The count is the smaller of that need and what is held less what the leg itself needs. A kind needed by no stage after the leg is never stashed: it is either the leg's own or junk. With keep.toml as it stands, the bow's feathers and string go into the chest before the Nether, and the leg's 64 cobblestone stay held. The End has no stage after it, so its trigger puts nothing away and says so.
- **The home chest**: when there is something to stash and no home chest within `TABLE_SEARCH`, the chest is made where the bot stands, by the goal rows `home-chest` (provides `block:chest`) and `place-chest` (config/goals.toml), through the needs planner. Only a planks craft, the chest at a table at hand, or putting down a table or the chest is taken. A gather, a mine or a smelt would be a trip: the stash is skipped and noted. The chest is begun only with wood to spare: planks plus 4 a log, less 4 a log of `world::stage::READY_LOGS` (the wood the leg carries), at least `CHEST_PLANKS` 8, and 4 more for a table when none is held or at hand. At most `CHEST_TASKS` 8 tasks. The chest put down becomes the home chest: the nearest chest memory holds that it did not hold before the place (`StashState::home`, its dimension and cell). A table put down for it is dug and taken along again before the stash.
- **After a death and a respawn**, `Plan::revived` sets `fetch_due`. At the next choice, `fetch_list` gives what a stage still ahead (`World::stage_reached` on) wants of what the home chest last showed, less what is held. That is fetched. The fetch is skipped and noted with no home chest in the dimension, a chest never shown open, or nothing wanted. Under danger it waits, noted at most every `FAILURE_MEMORY_TICKS`.
- **The task loop**: `Task::Stash` and `Task::Fetch` sit beside `Smelt` in every match. `Plan::chest_step` steps the transfer, sends its acts, drains its notes into the ledger each tick (as a shield step's are), and on its end notes "stash done" or "fetch done" (with the items moved) or the failure. A chest found gone or covered is no home any more.
- **The development aim `chest-trip`** (`Aim::ChestTrip`, `--goal chest-trip`, a moment's `plan:chest-trip`) is for parity only, never a frontier start nor a race's evidence. It puts the held chest down, stashes `TRIP` (74 of 138 cobblestone held as 64, 64 and 10, and 3 string), then fetches it back, each step through the planner's own `Task::Place`, `Task::Stash` and `Task::Fetch`, each noted (`chest trip: ...`). It is met once the fetch has ended.
- **Tests** (`plan/tests/stash_planner.rs`): the lists on the compiled-in keep table (`stash_list`, `fetch_list`, `wood_to_spare`, the legs parse). Against the server model: planks and a table held, so the table goes down, the chest is crafted and put down, the table comes back, and the feathers and string are stashed while the stone stays; then a respawn fetches them back. With only the leg's 4 logs, no chest is begun and the skip is noted. The `chest-trip` aim runs place, stash, fetch and ends with the pack as it began. A chest buried under the bot (no face exposed) is never remembered and never taken for the home chest; the one put down is. In `stash.rs`'s own tests a bot walks to a real nether portal block with the bow's feathers and only the leg's 4 logs: no chest is begun, the skip is noted with the wood count, the walk in keeps its own reason and goes on, and the trigger stays quiet after. With nothing to stash it says so and begins no chest.

## Parity rows
`config/moments.toml` `k00-half` and `k00-round` (`Script::Chest`, `chest:<in>:<out>` in `crates/bot/src/script.rs`). Each gives a chest, two stacks of 64 cobblestone, one of 10 and 3 string. The chest is placed east of the feet and opened. Four inventory stacks are shift-clicked in, then two (half) or four (round) chest slots are shift-clicked out. The outcome's counts show the server's order. In half, the two stacks fetched from chest slots 0 and 1 are the two 64s if the server fills the chest from its start, as the model does. These rows click a fixed sequence and do not run the `Stash`/`Fetch` tasks. The tasks themselves run in `k01-task`, below.

Parity run 46120883534 (6b6cc39f, `only=c00-`), on the race seed's vanilla 26.1.2 server: real and model agree in every field. `k00-half` (then named `c00-half`) ends with 128 cobblestone held on both sides (the two 64s came back, so the server fills the chest from slot 0 and keeps the 10 in a slot of its own). `k00-round` (then `c00-round`) ends with 138 cobblestone and 3 string on both sides. The run reports FAIL only because `only` was given without a `tolerance`, so the full table's 179 applied to 3 rows; it also took in the older `c00-planks`, which is why the rows were renamed to a `k0` prefix. The evidence run is `-f only=k0 -f tolerance=3`, with the task row `k01-task` (on the planner's dev aim `plan:chest-trip`, 77-stash's) once that aim lands.

`k01-task` (`plan:chest-trip`, the planner's development aim) runs the planner's own Place, Stash and Fetch tasks. It gives the same kit on a laid 5x5 stone floor with air over it. The planner puts the chest down, stashes 74 cobblestone and 3 string, and fetches them back; the row is met when the fetch ends. It is the first real-server run of the `Stash` and `Fetch` tasks.

The evidence run is parity run 46128707215 (831dbe72, `only=k0 tolerance=3`), on the race seed's vanilla 26.1.2 server. It passed: 3 of 3 rows match in every field. `k00-half` holds 128 cobblestone after, `k00-round` 138 cobblestone and 3 string, and `k01-task` 138 cobblestone and 3 string with the chest put down; real and model agree on all three. The real bot's ledger for `k01-task`, in order (plan ticks). It was read locally, not on CI: the run's artifact downloaded with `gh run download 46128707215`, `parity-46128707215/real/k01-task.real.rsr` decompressed with `zstd -dc`, and the planner's note strings taken out with `strings | grep`. No repository code ran. A CI read with `replay ledger <rsr> --force` on probe.yml would give the same lines with the replay's decoders:
- `chest trip: place the chest` (tick 29), then `chest trip: the chest at (21, 80, 20) is down`;
- the stash: `chest opened ... it shows nothing`, `stash moved 64 cobblestone by slot 55`, `stash moved 10 cobblestone by slot 57` (the other 64 kept), `stash moved 3 string by slot 58`, `chest closed ... 74 of 74 cobblestone, 3 of 3 string; it shows 74 cobblestone, 3 string`;
- the fetch: `chest opened ... it shows 74 cobblestone, 3 string`, `fetch moved 64 cobblestone by slot 0`, `fetch moved 10 cobblestone by slot 1`, `fetch moved 3 string by slot 2`, `chest closed ... it shows nothing`;
- `chest trip met (1 stashes and 1 fetches ended)` at tick 44. The aim met, the planner row ends (`Ending::Cap` on `p.finished()`): the run ended about 60 server ticks in, not at its 600-tick cap.

**The path, not only the end** (`crates/results/src/parity_notes.rs`). A row's `notes` is a list of `[substring, count]` pairs: each ledger substring must fire exactly that many times in the real bot's planner notes and in the model's. `k01-task` asks for `stash done:` and `fetch done:` (the planner's ends of the two tasks) once each, `chest closed at` twice, and `chest trip met` once. The first run of the check (parity 46130761031) asked for `stash done` once and found it twice on both sides: the window's close note (`stash done, 74 of 74 ...`) and the planner's end note (`stash done: 77 items ...`) both contain it. The substrings now tell those two notes apart. Parity run 46132490442 (fd33b26a, `only=k0 tolerance=3`) passed: 3 of 3 rows match in every field, and the compare reports "notes: every note of 1 rows fired as asked". That run is the citable CI evidence for the `Stash` and `Fetch` tasks on a real server. Every `MomentResult` now carries its planner notes in a new field, `notes` (each note's `what`, in order). The real side's come from the run's ledger and the model's from `Plan::events_since` each tick, so neither side is capped. A count that differs on either side is a miss. `moments compare` prints the misses under the table, and any miss fails the table. A bot that only put the chest down ends with the same counts as a full trip, but it would miss every one of these notes.

## Limits
- The chest rows and the task row agree with the real server (parity 46128707215). A double chest, a chest across a dimension change, and the planner's own triggers in a race are not yet run on one.
- The home chest is made at the Nether portal, the first leg with something to stash. A respawn far from it relies on the walk's route there. A walk that fails is noted, and the run goes on without the fetch.
