---
title: Crafting by clicks
description: How clojurecraft crafts and smelts through windows (views, one click per round trip, nothing predicted, slot 0 verified, absolute deadlines), and its stale-window failure.
type: reference
tags: [oracle, crafting, windows, clicks]
aliases: [craft intent, smelt intent, window view, stale-window, junk crafts, slot 0]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/craft.clj#Rules that come from the siblings' failures: nothing is predicted; a click is never sent while"
  - "clojurecraft:src/clojurecraft/window.clj#\"Clicking in a window, as values. A view describes one window the way the click rules need it:"
  - "clojurecraft:src/clojurecraft/smelt.clj#The rules are the siblings' furnace lessons (brain sib-smelt-fuel-deadlock): coal first, never more"
  - "clojurecraft:src/clojurecraft/window.clj#(def max-lost 3)                      ; clicks the server may leave unanswered in one intent before it fails :stale-window"
  - "clojurecraft:src/clojurecraft/window.clj#(def answer-timeout 3000)             ; a click the server never answers fails the intent"
  - "clojurecraft:brain/bot/decisions/manual-clicks-not-place-recipe.md#Why the bot lays every craft with container clicks it computes itself instead of the recipe-book place-recipe packet, and what would make place-recipe the better choice."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[wire-container-click]]"
  - "[[hist-v1-load-bearing-fixes]]"
---

# Crafting by clicks

Crafting was the first wall for every bot. clojurecraft's rules are the sum of the siblings' failures.

## Key files
- `src/clojurecraft/craft.clj`: the `:craft` intent.
- `src/clojurecraft/smelt.clj`: the `:smelt` intent.
- `src/clojurecraft/window.clj`: views, `click`, `resync-click`, `junk-slot`, `tossable`.
- `src/clojurecraft/recipe.clj`: `match`, `clicks`, `needs`.

## How it works
- A **view** describes one window for the click rules: id, latest state id, its own slots (a grid, a furnace's input, fuel, output) and where each player slot appears in it. Window 0, a crafting table and a furnace share every rule.
- **Craft stages**: `:settle` (put a loaded cursor down, toss a junk stack if no slot is free, shift-click leftovers out of the grid, plan the clicks), `:click` (one click per round trip: send, then wait until the window's state id moves), `:verify` (only when the server says slot 0 holds exactly the expected result, shift-click it), `:take` (done when the inventory shows it; close a table window).
- **Rules**: nothing is predicted; no click while the previous is unanswered; slot 0 is never clicked unless it holds what the recipe makes (a stray plank turns a take into a button); every deadline is absolute.
- **Smelting**: fuel first, coal before wood, never more fuel than the batch burns (wood burned is wood the tools lack); a furnace's own ticks move the state id, so a load is judged by its slots.
- Crafts are laid by computed clicks, not the recipe-book `place-recipe` packet.
- A click unanswered past `answer-timeout` (3000 ms) is lost: the intent resyncs from the server's slots and restarts its stage. The `max-lost`-th (third) loss fails `:stale-window`. On a loaded server a click is answered late rather than never, which is why the first losses resync instead of failing.

## For ruststeve
Port the stages as a `Task` with its stage inside the variant. The 3 s unanswered click is an `Expectation` with a deadline ([[expectations-and-watchdog]]).

## See also
- [[wire-container-click]] - the packet underneath.
- [[mc-recipes]] - recipe facts and the junk-craft trap.
