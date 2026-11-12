---
title: Casting the Nether portal
description: How every bot builds the Nether portal without a diamond pickaxe (casting obsidian from lava and water in a mould), and the rules each learned.
type: reference
tags: [game, portal, obsidian, nether]
aliases: [portal cast, obsidian cast, mould, mold, lava bucket, build plan]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:src/clojurecraft/build.clj#casts the frame as a player does (issue #124): the rock is the mould."
  - "clojurecraft:src/clojurecraft/build.clj#a dig never takes obsidian (ruststeve dug its own frame and restarted forever)."
  - "clojurecraft:src/clojurecraft/build.clj#Nothing about progress is stored: the op to work on is the first one not satisfied, so a build resumes after a death, a chunk reload or a relaunch by reading the world again."
  - "ruststeve-v1:CLAUDE.md#The portal is cast, not mined:"
related:
  - "[[game/_moc|Game]]"
  - "[[hist-walls]]"
  - "[[cc-goals-and-intents]]"
---

# Casting the Nether portal

No bot mines obsidian; that needs a diamond pickaxe. Each casts it: lava poured into a cell, then water poured onto the lava source, turns it to obsidian. Ten cells make a frame, lit with flint and steel.

## How it works (clojurecraft)
- The frame's ten cells are holes cut a block deep into a rock face beside any lava pool, at any depth: the rock is the mould. Lava is poured into a hole, water into the cell directly over it (it lands on the source: obsidian) and scooped straight back; the lava bucket is refilled at the pool.
- The build is a plan of ops as data (`:place :dig :pour :scoop :use-on :stand-in`), each with a done predicate over the world. Nothing about progress is stored: the op worked on is the first not satisfied, so a death or relaunch resumes by reading the world.
- A dig never takes obsidian.

## Rules the earlier bots paid for
- ruststeve v1 dug its own frame and restarted forever; never dig obsidian.
- v1 cast with a template mould, a sealed scoop station for refills, and per-bot frame anchor files; steve's frame order and lighting are in its code ([[hist-walls]]).
- Lava refills and lava deaths at the frame were the dominant costs in both earlier bots.

## See also
- [[hist-walls]] - the portal as a wall.
- [[cc-roadmap-stages]] - the portal stage's last failure.
