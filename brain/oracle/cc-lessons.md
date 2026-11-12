---
title: The lessons register (draft for LESSONS.md)
description: The lessons the Clojure rewrite learned, each with where it shows in clojurecraft and the Rust mechanism that keeps it; the seed of docs/LESSONS.md, pending Bridger's confirmation.
type: reference
tags: [oracle, lessons, porting, hickey]
aliases: [LESSONS.md, lessons register, what Clojure taught, Hickey lessons]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#The Clojure rewrite produced roughly 10 to 20 concrete lessons."
  - "clojurecraft:docs/RICH-HICKEY.md#What clojurecraft got right. The world is one immutable map in one atom, and only the loop thread writes it."
  - "clojurecraft:docs/RICH-HICKEY.md#Where it is still TypeScript. Honesty is the point of this section, so here is the list, roughly in the order he would raise it."
  - "clojurecraft:docs/RICH-HICKEY.md#A failure is a value, and a fix's check is a validation over it:"
related:
  - "[[oracle/_moc|Oracle]]"
  - "[[port-phases]]"
  - "[[cc-to-rust-map]]"
---

# The lessons register (draft for LESSONS.md)

The guide asks for a register of the Clojure rewrite's lessons, confirmed by Bridger before any Rust. This is the draft, each row read from clojurecraft's code and its design essay (`docs/RICH-HICKEY.md`).

| # | Lesson | Where it shows in clojurecraft | Rust mechanism |
|---|---|---|---|
| 1 | All state is one value, changed in one place | one atom, only the loop writes ([[cc-world-value]]) | one owned `Brain`, `&mut` for one step ([[one-owner-no-shared-mutation]]) |
| 2 | Decisions are pure functions of state and input | `(step world event)`; I/O in three namespaces | sans-IO `bot-core` ([[sans-io-core]]) |
| 3 | Time and randomness are inputs | `:event/now`, `:event/rand` on the tick | `Input.tick`, seeded RNG ([[cc-randomness-on-the-tick]]) |
| 4 | Every run can be replayed and verified | `record/verify` ([[cc-record-replay]]) | recordings + `bot-replay verify` in CI ([[recordings]]) |
| 5 | Effects are data, performed by the loop | `:bot/effects` | `Output.commands` with reasons |
| 6 | Write the information model down | `model.clj`, a test per attribute | the types themselves, documented |
| 7 | Goals are data; done is re-derived every tick | `goals.edn`, `plan-tick` ([[cc-goals-and-intents]]) | goal/phase table, predicates on the world model |
| 8 | Predict nothing; believe only the server | clicks, placement, digs judged by the server's answer ([[wire-container-click]]) | expectations with deadlines ([[expectations-and-watchdog]]) |
| 9 | Memory is facts with time, never overwritten | DataScript sightings and intentions ([[cc-world-memory]]) | append-only fact log + indexes |
| 10 | Grow by accretion, never break | superseded goal rows; `accretes?` | add variants, `serde(default)`, upcasters |
| 11 | Tactics as rule tables over named facts, kept small | `pearls-rules.edn`, which grew past what anyone could hold ([[cc-pearls-rule-table]]) | small `Rule` tables; the pearl table is not ported ([[rule-tables]]) |
| 12 | Model only what you use; count the unknown | packet decode ignores tails, `:unknown` counted ([[wire-packets-as-data]]) | `Unknown` variants, counters |
| 13 | Searches are budgeted in work, never time | A* in node expansions ([[cc-stall-and-budgets]]) | the same, in `bot-path` |
| 14 | Pace real-time actions by the clock | swings every 650 ms ([[cc-blaze-hunt]]) | shell-side pacing, recorded |
| 15 | Close the loop without a server | the server model and sim race ([[cc-sim]]) | a sim `Body` |
| 16 | A failure is a value; a fix is judged on it | snapshots recur on base, not on fix | regression recordings ([[incident-to-regression]]) |
| 17 | Write the problem before the stage | `problems.edn` ([[cc-roadmap-stages]]) | `docs/diagnoses/` and problem statements |
| 18 | Observers never touch the loop | watches on the atom, senses one line a second | `watch` snapshots, read-only endpoint ([[observability]]) |
| 19 | No X-ray | memory holds only seen faces ([[hist-no-xray]]) | perception typed by what was seen |
| 20 | Unhandled dispatch hides gaps | open multimethods let a case silently do nothing | exhaustive enums ([[enums-for-closed-sets]]) |
| 21 | Issues are a plan, not a log | #10 held nine stages; 256 auto-filed issues ([[cc-stage18-lessons]]) | twenty planned issues ([[issues-are-a-plan]]) |
| 22 | Sim and real must agree first | sim 30% against real 3% on the forest start | a parity table before any sim number ([[measurement-rules]]) |
| 23 | Measure the stuck stage directly | checkpoint starts lifted stage 18 from 30–150 attempts a commit | a checkpoint start per stage from day one |
| 24 | Loops without progress are the costliest bugs | tunnels in squares, paced corridors, blocks placed and dug back | progress invariants; resources per progress ([[progress-invariants]]) |
| 25 | Replays fold at their own commit | newer folds drifted and misled diagnoses | commit-pinned recordings ([[recording-format]]) |
| 26 | The planner is the CPU | 93% of a race JVM; one scan 33% | a CPU budget per tick in CI; spatial indexes |
| 27 | Caps in game ticks | real servers tick at 150–300 ms under load | server-tick caps ([[sync-core-and-time]]) |

## Limits
Unconfirmed by Bridger. Rows 1 to 5 match the guide's own starting rows; 6 to 20 are read from clojurecraft's code and essay; 21 to 27 are the closing handoff's lessons ([[cc-stage18-lessons]]).

## See also
- [[port-phases]] - the phase-0 gate this feeds.
- [[cc-decisions]] - clojurecraft's own decision notes.
