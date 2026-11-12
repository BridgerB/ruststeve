---
title: Stall detection and budgets
description: The numbers clojurecraft uses to decide it is stuck (attempts, stall and stuck windows, wait limits, failure memory) and to bound searches, which become ruststeve's watchdog Config.
type: reference
tags: [oracle, watchdog, thresholds, config]
aliases: [stuck-ms, stall-ms, max-attempts, wait-ticks, failure memory, churn, A* budget]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/plan.clj#(def max-attempts 3)                  ; consecutive failed intents before the plan fails"
  - "clojurecraft:src/clojurecraft/plan.clj#(def stuck-ms 600000)                 ; ms without progress (progress-key changed, or a move of stuck-extent) after which the bot is made to go elsewhere: 10 min"
  - "clojurecraft:src/clojurecraft/plan.clj#(def wait-ms 20000)                   ; and for this long on the bot's clock: a loop behind its schedule fires its missed ticks back to back"
  - "clojurecraft:src/clojurecraft/path.clj#The search is plain A* with a budget in node expansions, never in time, so a recording replays to the same route on any machine."
  - "clojurecraft:src/clojurecraft/churn_guard.clj#The invariant that catches CHURN once, for every intent, instead of one call site at a time (issue #615): in any state of the bot, the intent the shipped goal table chooses is not already over after one tick of running."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[expectations-and-watchdog]]"
  - "[[cc-goals-and-intents]]"
---

# Stall detection and budgets

The guide says to take watchdog thresholds from clojurecraft where it has them. These are they, all `def`s at the top of `plan.clj` with a comment each.

## Key files
- `src/clojurecraft/plan.clj`: the constants, `watch`, `unstick`, `progress-key`, `remember-failure`, `area-blocked?`.
- `src/clojurecraft/path.clj`: the A* budget.
- `src/clojurecraft/churn_guard.clj`: the churn invariant.

## How it works

| Constant | Value | Meaning |
|---|---|---|
| `max-attempts` | 3 | consecutive failed intents before the plan fails |
| `stall-ms` | 900000 (15 min) | no intent done, out of attempts → dead end |
| `stuck-ms` | 600000 (10 min) | no progress (progress key unchanged, no move spanning `stuck-extent` 16 blocks) → the bot is sent elsewhere (`explore-distance` 48) |
| `recover-cooldown-ms` | 10000 | between two recoveries |
| `wait-ticks` / `wait-ms` | 400 / 20000 | a goal with nothing to do this long has failed; both, because a late loop fires missed ticks back to back |
| `failure-memory-ms`, `blocked-after` | 30000, 3 | a choice failing 3 times in 30 s is refused anywhere |
| `area-blocked-after`, `area-radius` | 3, 6.0 | 3 `:blocked` failures in one area → leave it |
| `avoid-ms`, `avoid-radius` | 300000, 16.0 | the watchdog's spot is avoided 5 min |
| `max-unwalkable` | 12 | consecutive `:no-path` walks before the plan fails |

- Each goal row may override: `:goal/progress` (the terms compared), `:goal/stuck-after`, `:goal/facts-only?` (walking is not progress).
- **Searches are budgeted in node expansions, never time**, so a recording replays to the same route on any machine.
- **Churn**: the intent the table chooses must not be already over after one tick. When the planner's choice and the intent's end check disagree, the same intent restarts every 50 ms while the bot stands still; `churn_guard` checks the invariant over recorded snapshots.

## For ruststeve
These become `Config` fields of the watchdog and planner, converted from ms to ticks ([[cc-main-loop]]). The churn invariant is a property test: for any world, the chosen task's first step does not end it. Issue 09a ported `failure-memory` (600 ticks), `blocked-after` (3), `area-radius` (6), `max-unwalkable` (12) and the walk's own constants, and the search budgets in expansions ([[planner]], [[pathfinding]]).

## See also
- [[expectations-and-watchdog]] - the Rust mechanism.
- [[hist-walls]] - why the A* budget is counted in expansions (v1's sliced A*).
