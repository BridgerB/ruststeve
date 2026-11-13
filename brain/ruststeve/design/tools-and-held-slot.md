---
title: Tools and the dig rule
description: Where a tool's speed, harvest and durability come from (the reports' default item components, the tool rules evaluated per block in datagen), every item's max stack size, and the one dig rule the bot and the server model share (data::dig).
type: reference
tags: [data, tools, dig, items, components]
aliases: [item_props, MAX_STACK, MAX_DAMAGE, TOOLS, Tool, tool_of, max_stack, data::dig, progress, harvests, best_tool, break_ticks, tool component, default components]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@34b2252 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/datagen/src/model.rs#fn read_item_props("
  - "ruststeve:crates/data/src/dig.rs#pub fn progress("
  - "ruststeve:crates/data/src/dig.rs#pub fn best_tool("
  - "ruststeve:crates/data/src/types.rs#pub struct Tool {"
  - "clojurecraft:src/clojurecraft/dig.clj#(defn best-tool"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[datagen]]"
  - "[[mc-dig-timing]]"
  - "[[digging]]"
---

# Tools and the dig rule

Issue 11a. A tool's speed and drops, and every item's stack size, come from the game's own data; one rule turns them into a dig time for both the bot and the model.

## Key files
- `crates/datagen/src/model.rs` (`read_item_props`) → `crates/data/src/generated/item_props.rs` (`MAX_STACK`, `MAX_DAMAGE`, `TOOLS`).
- `crates/data/src/dig.rs`: `speed`, `harvests`, `progress`, `break_ticks`, `best_tool`.

## How it works
- **The source**: the server's `--reports` write every item's default components to `reports/minecraft/components/item/<item>.json`: `max_stack_size`, `max_damage` and the `tool` component's rules (a block tag, one block or a list; a speed and/or `correct_for_drops`), `default_mining_speed`. A JNI probe was tried first and dropped: in 26.1.2 item components are bound lazily, and binding them needs the data-driven registries (`Components not bound yet`, then a missing damage-type tag).
- **Evaluated as vanilla's `Tool` does**, per block, in datagen: the speed is the first rule with a speed whose blocks hold the block (else the default); the drops are got when the first rule with `correct_for_drops` holding it says so. 38 tools; a sword's `Float.MAX_VALUE` on bamboo is emitted as `f32::MAX`.
- **`data::dig`** (`Player.getDestroySpeed`, `getDestroyProgress`): the held tool's speed (1.0 for anything else), × 0.2 with the eyes in water, ÷ 5 off the ground, over the hardness, over 30 when the hand or the tool harvests the block (`!requiresCorrectToolForDrops || isCorrectForDrops`), else 100. `break_ticks` is the fewest n with `progress × n ≥ 1` in `float`, the server's own comparison (`ceil(1/p)` in doubles came out a tick high).
- Numbers: stone by hand 150 ticks and no drop; a wooden pickaxe 23 ticks, cobblestone; a stone pickaxe 12; a log by hand 60. A wooden pickaxe stacks to 1 and lasts 59 blocks; a stone one 131; an ender pearl stacks to 16.

## Holding (issue 11b)
`plan::hold::toward(item)`: `Held` when the selected hotbar slot holds it; else the hotbar slot that does is selected (`Act::Hold`), or the item is swapped into the hotbar's last slot through the cursor; `Wait` while a click is in flight or a window is open. The dig task holds `data::dig::best_tool` (the fastest held tool that gets the block's drops) before START and waits for the world to show it.

## Limits
- Enchantments, haste, mining fatigue and the `mining_efficiency` attribute are not modelled.
- Tool wear is not in the model yet (a wooden pickaxe lasts 59 blocks, past issue 11's digs).

## See also
- [[mc-dig-timing]] - the server's FINISH rule.
- [[digging]] - the bot's dig task that uses it.
