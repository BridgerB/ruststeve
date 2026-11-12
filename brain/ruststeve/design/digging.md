---
title: The dig task
description: How the bot digs a block and collects its drop (plan::dig) - START, FINISH at the break time times 1.15 times the measured slowdown plus 2 ticks, confirmation by the block update, retries, the collect walk - and why logs are dug from beside them (the pickup goal).
type: reference
tags: [plan, dig, gather, collect, pickup]
aliases: [Dig, DigStage, DigFailure, Taken, Gone, crowded, CROWD_RADIUS, break_ticks, slowdown, FINISH margin, collect, pickup goal, Goal::Pickup, to_dig, DropUnreachable, DropWalkFailed, Uncollected, Refused]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@8df1710 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/dig.rs#pub fn break_ticks(kind: BlockKind, eye_in_water: bool, on_ground: bool) -> Option<u64> {"
  - "ruststeve:crates/plan/src/dig.rs#pub fn slowdown(w: &World) -> f64 {"
  - "ruststeve:crates/plan/src/dig.rs#pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, DigStatus) {"
  - "ruststeve:crates/plan/src/walk.rs#pub fn to_dig(target: Cell, tick: u64) -> Self {"
  - "ruststeve:crates/plan/src/lib.rs#pub fn crowded(w: &World, (x, y, z): Cell) -> bool {"
  - "ruststeve:crates/plan/src/path.rs#Pickup { pos: Cell, range: f64, rise: i32 },"
  - "clojurecraft:src/clojurecraft/path.clj#(defmethod goal-done? :pickup"
  - "clojurecraft:src/clojurecraft/intent.clj#(def finish-margin 1.15)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[mc-dig-timing]]"
  - "[[wire-actions]]"
  - "[[goal-table-and-needs]]"
  - "[[walk-and-watchdog]]"
---

# The dig task

Issue 10c. `plan::dig::Dig` breaks one block from where the bot stands and collects its drop, believing only the server.

## Key files
- `crates/plan/src/dig.rs`: `Dig`, `break_ticks`, `slowdown`.
- `crates/plan/src/path.rs` (`Goal::Pickup`) and `walk.rs` (`Walk::to_dig`).

## How it works
- **Where to stand**: a log is walked to with clojurecraft's `:pickup` goal: a neighbouring column (diagonals too) with the log at the feet's height or up to 2 above, so the drop falls inside the player's touch box (grown 1 sideways, 0.5 up). Walking only "within reach" (eye to centre 4.0) left drops 2-3 blocks off, and a trunk's lowest log drops under the trunk, where no one can stand.
- **Start**: `Act::Dig { Start }`; the break time by hand is `ceil(1 / progress)` with the server's progress rule ([[mc-dig-timing]]): a log 60 ticks, 300 with the eyes in water.
- **Finish**: at `ticks × 1.15 × slowdown + 2` (clojurecraft's margin and its 100 ms); `slowdown` is our ticks over the server's once 200 server ticks are heard (20 per `set_time`), between 1 and 8. An early FINISH would still break the block later (a delayed destroy), but the task does not count on it.
- **Confirm**: the block update to air; past `waited + 60` ticks without it the dig is tried again, and the third try fails `Refused`.
- **Collect**: done when the inventory holds more of what the block drops (its first sure loot). The drop is waited for 20 ticks where it fell; then the bot walks to beside it (`Near`, range 1); no route fails `DropUnreachable`, a failed walk `DropWalkFailed`, 300 ticks `Uncollected`. A failure notes where the drop and the feet were.
- **Other players** (docs/problems/wood.md): a log with another player within 2 blocks of its column is not chosen (`crowded`); a drop that vanishes without this bot's own `take_item_entity` and with no rise in its count fails at once `Taken` (after 10 ticks for the bot's own pickup to show); a block already gone when the dig begins fails `Gone`, never "dug".
- **Failures** go to the planner's failure memory: the block is refused a while.

## Evidence
- Sim, 320 captured landings, `--goal table`, 6,000 ticks: 193 of 200 qualifying made the kit. Before digging from beside, three of a landing's first four digs failed to collect; a drop that drifted sideways sank into the higher ground beside a trunk until the model collided items sideways ([[server-model]]).
- `break_times_by_hand` (unit), `the_bot_makes_the_kit_from_nothing` (closed loop on the plains fixture).

## Limits
- By hand only: tools (speed, the right tool for drops) are issue 11; no pillaring to logs above reach (issue 11).
- The task does not look at the block; the server does not check the look for digging.

## See also
- [[mc-dig-timing]] - the server's rule, read from the jar.
- [[goal-table-and-needs]] - why a log is gathered.
