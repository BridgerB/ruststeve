---
title: The craft task
description: How the bot crafts in window 0 (plan::craft) - clojurecraft's settle, place, verify and take stages as a state, one click per round trip, the 3 s missed-click rule with a resync, the result checked before it is taken, and the click budget as its progress invariant.
type: reference
tags: [plan, craft, window, clicks]
aliases: [Craft, Stage, CraftFailure, StaleWindow, WrongResult, TooManyClicks, Missing, layout, Act::Resync, resync click, put_down]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@34b2252 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/craft.rs#pub fn step(&mut self, w: &World, tick: u64) -> (Option<Act>, CraftStatus) {"
  - "ruststeve:crates/plan/src/craft.rs#pub fn layout(r: &Recipe) -> Vec<(i16, &'static [ItemKind])> {"
  - "ruststeve:crates/plan/src/craft.rs#pub const CLICK_PATIENCE_TICKS: u64 = 60;"
  - "ruststeve:crates/world/src/lib.rs#    Resync,"
  - "clojurecraft:src/clojurecraft/window.clj#way to learn the cursor after a lost click, since click forgets it as it sends."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[wire-container-click]]"
  - "[[cc-window-clicks]]"
  - "[[goal-table-and-needs]]"
---

# The craft task

Issue 10c. `plan::craft::Craft` makes one run of a 2x2 recipe in window 0, from what the server says the window holds ([[wire-container-click]]).

## Key files
- `crates/plan/src/craft.rs`: `Craft`, `Stage`, `layout`.
- `crates/world/src/lib.rs`: `Act::Click`, `Act::Resync`, the pending click.

## How it works
- **One click per round trip**: a click is sent only when no click is pending; the world clears the pending click when window 0's state id moves. A click that would change nothing is never sent (it draws no reply).
- **Settle**: the cursor put down (on a same-item stack with room, else an empty slot in 9-44), then every grid slot shift-clicked out.
- **Place**: `layout` lays the recipe at the grid's top left (a shaped pattern by its rows, a shapeless one in slots 1-4). For the next empty cell: pick up a stack that fits it, right-click one item in, and put the cursor back when it holds what no cell still wants. The grid is never cleared here.
- **Verify**: slot 0 must hold exactly the recipe's result and count; 20 ticks without it fail `WrongResult` (the planner settles and chooses again).
- **Take**: shift-click slot 0; done when the inventory outside the grid holds more of the result.
- **A missed click**: unanswered for 60 ticks (3 s) → `Act::Resync` (a click on slot -1 with a state id the server never has, which draws the whole window) and back to Settle; the third miss fails `StaleWindow`.
- **The progress invariant**: 40 clicks at most for one run (`TooManyClicks`); a planks craft takes 3, sticks 5, a table 7.

## Evidence
- The sim measure's first run failed 2,376 crafts `TooManyClicks`: leftover planks went back to Settle, which emptied the grid, and the craft began again. Place now puts the cursor down itself.
- Parity rows `craft:planks|sticks|table` match real on a runner ([[parity-table]]); `the_bot_makes_the_kit_from_nothing`.

## Limits
- Window 0 only: the crafting table's 3x3 window is issue 11.

## See also
- [[cc-window-clicks]] - clojurecraft's rules this follows.
