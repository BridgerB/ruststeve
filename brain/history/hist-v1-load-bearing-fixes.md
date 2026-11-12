---
title: What v1 learned the hard way
description: The protocol and game facts ruststeve v1 paid for in races (attack packet, swing pacing, click sync, stale windows, grid ingredients, feet row, portal collision, own-portal memory), each a gotcha ruststeve must not relearn.
type: reference
tags: [history, ruststeve-v1, gotchas, protocol]
aliases: [v1 fixes, load-bearing mechanisms, attack packet, wait_real_ms, feet row, portal collision, mixed planks]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve-v1@9e1567f
sourceRefs:
  - "ruststeve-v1:CLAUDE.md#Melee (bot/mod.rs::attack): protocol 775 has a dedicated attack packet (0x01, {entityId}); interact is right-click only."
  - "ruststeve-v1:docs/report-2026-10-10.md#Crafting searched the grid for ingredients."
  - "ruststeve-v1:docs/report-2026-10-10.md#Generated block data gives portals a full collision cube; vanilla has none."
  - "ruststeve-v1:docs/report-2026-10-10.md#Feet from floor(y) read one row low when standing on a block top (y = 89.9999): every Nether tunnel step tested the floor row."
  - "ruststeve-v1:docs/report-2026-10-10.md#Own-portal knowledge is not X-ray: remember the frame you lit and walk back without line of sight."
  - "ruststeve-v1:CHANGES.md#place_crafting_table took only state 0 as an empty cell, and cave_air is 15293, so in a carved cave no cell qualified."
  - "ruststeve-v1:CHANGES.md#so its free-space test ran near the world origin. Fixed."
  - "ruststeve-v1:src/bot/inventory.rs#// Send EMPTY changedSlots so the server ALWAYS sees a prediction mismatch and"
related:
  - "[[history/_moc|History]]"
  - "[[wire-actions]]"
  - "[[wire-container-click]]"
  - "[[hist-ruststeve-v1]]"
---

# What v1 learned the hard way

Each of these stopped a race or a gym until found. They are facts about the game and the protocol, so they carry over even though v1's code does not.

| Symptom | Cause | Fix |
|---|---|---|
| Melee deals zero damage | in 775 melee is the `attack` packet (entity id only); `interact` is right-click only | send `attack` ([[wire-actions]]) |
| Attacks rejected as out of reach | the server judged reach against a stale pose | send position and look before the attack |
| Only the first of many swings lands | tick-paced waits collapse under a packet flood into the 10-tick hurt invulnerability | pace swings by wall clock (650 ms) |
| Crafts fail under load | optimistic click prediction drifts | empty `changedSlots`, apply server slots verbatim ([[wire-container-click]]) |
| Reply lag of one click; an oak button | the ack wait returned on the first packet of several | read until the window is quiet 100 ms |
| `bot.craft` Ok on a craft that never happened | clicks into a table window the server had closed | close it first; drop the window on respawn |
| Mixed planks made an oak button every time | the ingredient search scanned the grid and picked a just-placed plank back out | search the inventory section only |
| Bot stops flush against its own portal | generated block data gave portals a full collision cube; vanilla has none | patch portal collision to none |
| Every Nether tunnel step tested the floor | feet from `floor(y)` read a row low at y = 89.9999 | `floor(y + 0.01)` for the feet row |
| Bot stands beside its own portal unable to enter | `enter_nether` wanted line of sight | remember the lit frame and walk back to it |
| A crafting table could not be placed in caves (one trial looped 1,944 times) | placement accepted only state 0 (`air`), not `cave_air` | accept any `*air` |
| Physics lift tested near the world origin | a port bug in the out-of-liquid lift | test collision at the body |

## See also
- [[hist-walls]] - the walls these sat on.
- [[cc-window-clicks]] - clojurecraft's click rules, which reached the same answers.
