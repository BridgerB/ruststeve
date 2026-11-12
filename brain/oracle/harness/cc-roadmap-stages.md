---
title: clojurecraft's stages and their last failures
description: Every stage of the run as clojurecraft states it in problems.edn, with the last recorded failure of each; where clojurecraft stands, and where ruststeve's parity gates will be measured.
type: reference
tags: [oracle, roadmap, stages, failures]
aliases: [problems.edn, problem statements, last failure, hammock, stages]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:resources/clojurecraft/problems.edn#;; Problem statements, one per stage of the run, written before the stage is built (docs/RICH-HICKEY.md,"
  - "clojurecraft:resources/clojurecraft/problems.edn#\"The last failure was a ping-pong of 352 one-move tunnels in the Nether, the retreat count living in the intent rather than the target.\""
  - "clojurecraft:resources/clojurecraft/problems.edn#\"The last failure was an intent chosen again every 50 ms while the bot stood still; the planner now counts that churn and a guard checks it.\""
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| 1–16: wood, tools, stone, furnace, iron, buckets, water, flint and steel, obsidian cast, portal, Nether entry, fortress | passed (proven repeatedly from spawn by Oct 31) |"
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[port-phases]]"
  - "[[hist-walls]]"
---

# clojurecraft's stages and their last failures

`problems.edn` holds one problem statement per stage, written before the stage was built: the problem, the information it needs, the risks, what done means in world terms, and the last recorded failure. A test requires one for every target goal.

## Stages and last failures (at be85a92)

| Stage | Last recorded failure |
|---|---|
| wood | drops left out of reach (under a trunk, behind a leaf, over a ledge) |
| kit | the only log spent on the table before the sticks, nothing left to climb with |
| pickaxe | a snowfield where every route was `:none` over one-layer snow |
| stone-tools | a collect walked off a one-block stair rim into a cave lake |
| water-and-lava | under a frozen sea, dug the ice overhead and floated with no bank |
| stone-pickaxe | dug itself into a pit it could not climb out of |
| iron-kit | a smelt cut short by foraging; the planner mined for iron already in the furnace |
| portal | a lava refill with no bucket; a station with no way to it |
| nether-and-fortress | a pickaxe worn out every few minutes in one cavern; a full pack |
| blaze-rods | a ping-pong of 352 one-move tunnels, the retreat count living in the intent |
| ender-pearls | bots digging down through an open trench under fireballs |
| eyes-of-ender | none; 6/6 with twelve eyes 20 to 30 s after landing |
| stronghold | a dry tunnel blocked by water near room height ended `:no-way` |
| end-portal | none in the gym; the way to a real stronghold room is open |
| dragon | tunnel searches that timed out the keep-alive; a fight begun at low health |
| race | an intent chosen again every 50 ms while the bot stood still (churn) |
| survival | no eating until hurt or started hungry |

## Where clojurecraft stands (at shutdown)
Stages 1 to 16 passed from spawn in the real and sim races by Oct 31; stage 17 (blaze rods) passed at 7.3% real; stage 18 (pearls) never passed; stages 19 to 25 have code but were never reached in a race ([[cc-handoff]]). The design essay's own "Status" section (prepared starts only) is older than this and out of date.

## For ruststeve
Phase 4's parity gate is measured against clojurecraft's spawn-to-Nether baseline, so measure it first ([[port-phases]]). Port the problem statements with the stages: they are the written diagnosis the guide asks for, already done.

## Limits
These are the rows at one commit; clojurecraft merges several fixes a day. Re-read `problems.edn` at a newer pin before relying on a row.

## See also
- [[hist-walls]] - the same walls as the earlier bots met them.
- [[cc-goals-and-intents]] - the goals each stage covers.
