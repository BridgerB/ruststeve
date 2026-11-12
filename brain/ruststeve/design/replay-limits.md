---
title: Replay is open-loop
description: Why a recording cannot test a changed brain past its first changed decision, and which tool answers which question instead.
type: explanation
tags: [replay, testing, open-loop, closed-loop]
aliases: [open loop, closed loop, bot-replay diff, scenario tests]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#This is the most important subtlety in the whole guide. A recording is open-loop."
  - "ruststeve:FUNCTIONAL.md#Run a pinned-version local server with a fixed world seed and operator permissions for the test harness."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[recordings]]"
  - "[[testing-kinds]]"
  - "[[cc-sim]]"
---

# Replay is open-loop

A recording holds what the world did in response to the *old* brain's commands. Change the brain and its commands differ, and the real world would have answered differently, so every recorded input after the first changed decision is no longer a valid test of the new behaviour. The guide calls this the most important subtlety in the whole design.

## Key files
- `FUNCTIONAL.md`, sections 9.4 and 9.6.

## How it works

| Tool | Valid for | Not valid for |
|---|---|---|
| Exact replay (`verify`) | determinism; reproducing a bug exactly; beliefs and decisions at any past tick | testing a changed brain |
| Open-loop replay with a new brain (`bot-replay diff`) | the first decision that changes, and checking a fix changes behaviour at the right tick and nowhere earlier | anything after the first changed command |
| Closed-loop scenarios on a local server | whether a fix works: a success rate over many runs | exact reproduction |
| `bot-sim` (later) | fast closed-loop tests of one tactic | fidelity to real physics |

A scenario is data: setup commands (`/tp`, `/give`, `/effect`, mobs, time), the phase and task to start in, a time limit and a success predicate on the world model, run many times on a pinned server with a fixed seed.

## Gotchas
- clojurecraft closes the loop without a server by running the bot against a pure server model (`sim`), and keeps it honest against real races. That is more than the guide's optional `bot-sim`, and it is how clojurecraft tests changed behaviour cheaply ([[cc-sim]]).

## See also
- [[recordings]] - the format being replayed.
- [[testing-kinds]] - where each kind of test runs.
