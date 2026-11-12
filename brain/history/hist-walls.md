---
title: The walls, in order
description: The sequence of walls every bot hit on the way to the dragon (crafting, water, iron, the portal cast, entering, the fortress, blazes, pearls, the End), with how far each bot got and the measured cost of each.
type: reference
tags: [history, walls, roadmap, races]
aliases: [race walls, where bots get stuck, furthest progress, open problems]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve-v1@9e1567f steve@46c1264 clojurecraft@be85a92
sourceRefs:
  - "ruststeve-v1:docs/report-2026-10-10.md#| 1 | find_fortress: Nether wander without finding a fortress | i11 2.45 h (arrival pocket, fixed), i12 3.02 h (out of cobble, netherrack fix unmeasured) | ~164 | water ~419 (now ~0) |"
  - "steve:docs/report-2026-10-09.md#No steve bot stood in the Nether this cycle by its own portal."
  - "steve:docs/report-2026-10-09.md#Refill reachability on natural pools."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| 1–16: wood, tools, stone, furnace, iron, buckets, water, flint and steel, obsidian cast, portal, Nether entry, fortress | passed (proven repeatedly from spawn by Oct 31) |"
related:
  - "[[history/_moc|History]]"
  - "[[cc-roadmap-stages]]"
  - "[[port-phases]]"
---

# The walls, in order

Every bot meets the same walls in the same order. A wall moves only when the one before it falls.

| # | Wall | What made it hard |
|---|---|---|
| 1 | Wood and the crafting kit | windows and clicks: grid items, stale windows, junk crafts, drops out of reach ([[cc-window-clicks]]) |
| 2 | Water | no buoyancy; drowning in caves and aquifers; v1 measured about 419 bot-minutes a race on it before fixing it ([[mc-water]]) |
| 3 | Iron and smelting | fuel deadlocks (planks burned as fuel), a furnace batch forgotten |
| 4 | Bucket, water, flint and steel | finding reachable surface water |
| 5 | The portal cast | lava refills on natural pools, lava deaths at the frame, frames abandoned ([[mc-portal-cast]]) |
| 6 | Entering the Nether | walking back into one's own portal |
| 7 | Finding the fortress | Nether wander; v1's top cost at about 164 bot-minutes a race |
| 8 | Blaze rods | burning, volleys, fights in the open ([[cc-blaze-hunt]]) |
| 9 | Ender pearls | reaching ticking endermen at all, then fighting them; where clojurecraft stopped after four weeks ([[cc-stage18-lessons]]) |
| 10 | Stronghold, End, dragon | code exists in clojurecraft; never reached in a race |

## Where each bot stands (at the pins)
- **steve** (main 46c1264, cycle 5): no bot in the Nether by its own portal; natural portal 0/63. Its cycle-7 branch (PR #7, not merged at the pin) reports 22 of 140 race bots into the Nether.
- **ruststeve v1** (cycle 6): seven own-portal Nether entries, best 78 minutes; no fortress reached in a race. Next wall: wall 7.
- **clojurecraft** (be85a92, shut down 2026-11-12): stages 1 to 16 passed from spawn by Oct 31, stage 17 (blaze rods) passed at 7.3% real, stage 18 (pearls) never passed. Its wall was wall 9 ([[cc-handoff]]).

## For ruststeve
Phase 4's gate is wall 6 from a fresh spawn at clojurecraft's rate; phase 5 is walls 7 and 8 ([[port-phases]]).

## See also
- [[cc-roadmap-stages]] - each stage's last failure in clojurecraft.
- [[hist-v1-load-bearing-fixes]] - the gotchas found on these walls.
