---
title: Recipes and crafting
description: How a crafting grid matches a recipe, where recipes come from in 26.1.2, and the junk-craft trap of leftover planks.
type: reference
tags: [game, crafting, recipes]
aliases: [recipe match, crafting grid, junk craft, button, pressure plate, recipes.edn]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/recipe.clj#Grids are {slot item-name} with slot 1..size² in reading order (slot 0 is the result); the 2x2"
  - "clojurecraft:src/clojurecraft/craft.clj#slot 0 is never clicked unless it holds what the recipe makes (a stray plank turns a take into a button)"
  - "clojurecraft:brain/game/mechanics/mc-recipes-in-jar.md#The downloadable server.jar is a bundler."
related:
  - "[[game/_moc|Game]]"
  - "[[cc-window-clicks]]"
  - "[[cc-plan-and-make]]"
---

# Recipes and crafting

## How it works
- Recipes are not in the server's `--reports`; they are JSON files in the inner server jar's datapack (`data/minecraft/recipe/`), alongside tags, loot tables and worldgen. clojurecraft's datagen resolves tags and writes `recipes.edn`.
- A grid is slots 1 to n² in reading order, slot 0 the result: the player's 2×2 grid (window 0, no table needed) or a crafting table's 3×3.
- The server computes the result from what lies in the grid; the client only lays items and takes the result.

## Gotchas
- **Junk crafts**: one stray plank left in the grid turns a take into a button or a pressure plate. Verify slot 0 holds exactly the expected result before taking it, and clear the grid before laying.
- Mixed plank species in one craft defeated v1's ingredient search ([[hist-v1-load-bearing-fixes]]).

## See also
- [[cc-window-clicks]] - the click rules.
- [[cc-plan-and-make]] - recipes as producer rows.
