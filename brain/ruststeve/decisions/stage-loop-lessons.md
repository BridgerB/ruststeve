---
title: Stage loop lessons
description: What issue 16's night of 2026-11-23 taught about working a wall - the order of tests (unit test, probe on a recorded world, short race from a snapshot right before the wall), a fix is not done until its own ledger line is counted, every new code path writes a ledger note, senses rows round and cells floor, one world per snapshot tunes fixes to one place, and a chain of stages is split across agents with development starts.
type: decision
tags: [decision, loop, testing, evidence, ledger, races]
aliases: [stage loop lessons, test order, a race is the last check, count the ledger line, senses round cells floor, one fix per race, lessons of issue 16]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4
sourceRefs:
  - "ruststeve:crates/plan/src/path.rs#pub fn feet_cell([x, y, z]: [f64; 3]) -> Cell {"
  - "ruststeve:crates/plan/src/lib.rs#self.note(tick, \"escape still trapped\".to_owned(), why);"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[observe-first]]"
  - "[[development-starts]]"
  - "[[one-bot-first]]"
  - "[[cc-stage18-lessons]]"
---

# Stage loop lessons

## The choice
Issue 16 spent about twelve hours on 2026-11-23 shipping one fix a race at the Nether walk, and its record misread its own evidence twice (`docs/problems/blaze.md`, the corrections under walls 22 and 23; `docs/problems/blaze-log.md`). These rules come from that night. They sharpen [[observe-first]]; they do not replace it.

1. **Test in this order; a race is the last check, not the first.**
   - A unit test on a hand-built world (pure functions; `sim::flat_world`, `set_block`, `bot::lockstep::run`): minutes in CI.
   - A probe on a recorded world: `why` on a lane's `.rsr` at the tick, or a pure function run on a recording's checkpoint world.
   - A short race from a snapshot taken right before the wall, with a short cap (12,000 ticks is ten minutes). Not a three-hour race from two stages back to reach a wall in its last minute.
2. **A fix is not done until its own ledger line is counted** in a race, and the stage's outcome did not drop. "No deaths so far" is not a count. If it did not fire, or made things worse, revert it and write that down.
3. **Every new code path writes a ledger note when it fires** (`self.note(tick, what, why)`). A path that writes only to a side structure cannot be counted: the lava seal wrote to the escape's trail, which reaches the ledger only in a stall note or the "escape still trapped" note, so five firings read as zero and the record called a working fix dead.
4. **Senses rows round, cells floor.** A senses row prints the position with no decimals (rounded); `path::feet_cell` floors each coordinate. A row "30,31,-18" was the feet cell (30, 31, -19), and a block view centred on the rounded position showed lava "beside" a bot that was standing in it. Ask `why` for the exact position before naming a cell.
5. **One world per snapshot.** Every race from a snapshot replays one cave, one portal exit, one set of trees. Fixes get tuned to that place, and one unlucky place (five creepers under the start) taxes every race. Take several snapshots in different worlds, and say in the wall which world a fix was judged in.
6. **Split a chain of stages; do not walk it serially.** When the stages after the wall have never been reached, their walls are unknown and the wall in front hides them. Give each stage to its own agent with its own problem file, from a real snapshot where one exists and a development start where none does ([[development-starts]]), and integrate. Development runs find and fix; only a real bot from a real snapshot proves.
7. **Read the code for the cause before the next race.** Walls 22 and 23 were two faces of one fact (the route finder has no start at a lava-sea portal exit), found in an hour of reading after two races had each patched a symptom.
8. **Check the harness's own claims the same way.** "The servers ran 14 ticks a second" was arithmetic on a progress line; the lane logs said 20.0 and the job's step times showed the lost hour was a verify step. A number about the tooling needs a line read from a file, like any other.

## What was rejected
- **One fix, one long race, read at the end.** It is the loop that produced a day with no stage gained. A race costs hours and answers one question; a unit test costs minutes.
- **Judging a fix by the absence of the failure.** Two of three bots living says nothing about a seal nobody could count.

## What would change the answer
A sim with a standing parity table for the stage ([[measurement-rules]]) moves step 3 of the order into CI; the real race still decides.

## See also
- [[observe-first]] - the coverage gate and "a fix counts its firings".
- [[cc-stage18-lessons]] - clojurecraft's four weeks on one stage, the same mistakes at length.
- [[development-starts]] - what a made-up start may and may not prove.
