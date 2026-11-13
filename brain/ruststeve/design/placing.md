---
title: Placing and using blocks
description: How the bot places a held block or opens one (use_item_on, Act::UseOn), the packet's fields, and the model's handleUseItemOn - reach, a crafting table opening, BlockItem.place in the clicked cell when replaceable else beside it and never in the player, the two block updates and the ack.
type: reference
tags: [wire, place, use_item_on, model]
aliases: [use_item_on, UseItemOn, Act::UseOn, place_held, BlockItem.place, handleUseItemOn, replaceable]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@7e42e00 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/protocol/src/packets.rs#/// (a crafting table). Field order from the game's `ServerboundUseItemOnPacket`; the cursor is"
  - "ruststeve:crates/sim/src/survival.rs#pub(crate) fn on_use_item_on("
  - "ruststeve:crates/sim/src/survival.rs#fn place_held("
  - "ruststeve:crates/sim/tests/table.rs#fn a_held_table_is_placed_on_the_face_clicked() {"
  - "clojurecraft:src/clojurecraft/packet.clj#[:play :c2s :use-item-on] [[:hand :varint] [:pos :position] [:face :varint] [:cursor-x :f32]"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[wire-actions]]"
  - "[[table-window]]"
  - "[[server-model]]"
---

# Placing and using blocks

Issue 11a. `Act::UseOn { pos, face }` right-clicks a block's face with the main hand: `use_item_on` (hand, position, face, cursor x/y/z as floats, inside, world-border-hit, sequence), the cursor at the middle of the face, the dig sequence counted as for digs.

## How the server answers (`handleUseItemOn`, read with `javap`)
- The sequence is acknowledged whatever happens; out of reach (eye to the block's box over 4.5 + 1) nothing else does.
- A crafting table opens its menu ([[table-window]]) unless the player sneaks.
- Else a held block item is placed (`BlockItem.place`): in the clicked cell when it is replaceable (air, water, plants, snow), else in the cell beside it on the face; never in a cell the player's box overlaps; the held stack loses one.
- Always: block updates for the clicked cell and its neighbour on the face, so a refused placement is corrected on the client.

## The place task (issue 11b)
`plan::place`: the item held first (`plan::hold`: selected if in the hotbar, else swapped into the hotbar's last slot through the cursor), then a floor cell beside the feet, at the feet's height, a block up or a block down (in a staircase the walls are solid at the feet; the stair just come down is open a block up), empty, a full block under it, not in the body; the click on the top of the block under it; done when the block update shows it there; another cell after 40 ticks; three tries.

## Evidence
- Model tests: a table placed on the face clicked, the hand emptied, and never placed where the player stands.
- Parity rows `t…` place a given table on stone laid east of the feet in real and model.

## Limits
- Facing-dependent states (stairs, logs' axis) are placed in their default state; issue 11 places only tables and cobblestone.

## Issue 13b
- With no free cell, a wall block beside the feet is dug for room first, at most two ([[veins-and-flint]]).

## See also
- [[wire-actions]] - the other client actions.
