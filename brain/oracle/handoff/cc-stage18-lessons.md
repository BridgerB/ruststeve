---
title: Why stage 18 stalled
description: The structural causes that kept clojurecraft's ender-pearl stage from passing for four weeks (issues as a log, sim-real gaps, throughput, resource-burning loops, replay drift, a CPU-bound planner), each with its lesson for ruststeve.
type: explanation
tags: [oracle, handoff, lessons, stage-18, pearls]
aliases: [stage 18, what stalled us, section 4, pearl stage lessons, sim real parity, loops without progress]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Lesson: issues are a plan, not a log."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Lesson (ML principle 1 in LOOP.md, honoured too late): sim and real must agree before a sim number counts."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Lesson: from the first day, every stage has a checkpoint start, and every lane can play any stage."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Lesson: loops that make no progress are the most expensive bug class in an autonomous bot."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A recording folded with newer code drifts."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Lesson: give each bot a CPU budget per tick and measure it in CI."
related:
  - "[[oracle/handoff/_moc|Handoff]]"
  - "[[measurement-rules]]"
  - "[[progress-invariants]]"
  - "[[cc-handoff]]"
---

# Why stage 18 stalled

Over 100 stage-18 PRs merged and the stage never passed. The fixes were local answers to local symptoms while structural problems sat underneath. The handoff calls this the single most useful part: almost every mistake here is as easy to make in Rust.

## The causes, in rough order of cost
1. **Too much in too few issues, too many issues.** Issue #10 held nine stages; #263 held the whole pearl hunt. Meanwhile the churn auto-filed one issue per failure signature (256 by the end), and agents fixed symptoms in parallel in the same code. The pearl rule table grew to about 60 rows nobody could hold in their head; a fix to one row moved deaths to another (heal-in-cover cut fireball hits while healing from 22 to 4 and moved the deaths to magma cubes). **Lesson:** issues are a plan, not a log; failure signatures go to the results page and case files, never the tracker ([[issues-are-a-plan]]).
2. **Sim and real disagreed, found late.** Sim forest start ~30%, real ~3%. Causes: the race servers' simulation distance 3 froze mobs beyond about 48 blocks while the sim ticked them to 6 chunks; magma cubes hit through ceilings in the sim; no skeletons in the sim's Nether; wall-clock caps gave real bots a fifth of the game time (a real server under 14 bots ticks at 150–300 ms); the bot JVM took about 3.3 of 4 cores and starved the server. **Lesson:** a sim number counts only after a parity table against real at the same moments; the model's knobs come from the race's own config file ([[measurement-rules]]).
3. **Throughput.** Every race bot had to win stage 17 first, so stage 18 got 30–150 attempts per commit. Checkpoint starts fixed it in a day. Even then, comparisons of 9 runs a side, and two "fixes" identical to main run for run because their code never fired. **Lesson:** every stage has a checkpoint start from day one; judge on tens of paired runs; count the new path's firings before reading a result.
4. **The bot burned its own resources in loops**, visible only by folding real recordings: break-out tunnels turned 90° every 30 s and dug squares (877 of one bot's 1,244 digs, about six pickaxes); heading legs paced an old corridor on a 1-wide causeway; building blocks spent sealing and unsealing (about 600 by minute 40, then 37 minutes with nothing to bridge); a craft and the hunt preempted each other 410 times (grid items did not count as held); exit targets flipped between two cells. **Lesson:** a progress invariant in every long intent; measure resources spent per unit of progress ([[progress-invariants]]).
5. **Replays drifted when folded on newer code**: the bot's own choices move it, so a newer fold leaves the world the server saw. **Lesson:** fold at the recording's commit; call new functions as pure functions on those worlds ([[recording-format]]).
6. **The planner was the CPU**: about 93% of the race JVM's CPU; one check scanned 5,675 fortress bricks several times a tick (33% of samples), fixed by chunk buckets. **Lesson:** a CPU budget per bot per tick, measured in CI; spatial indexes from the start.
7. **Smaller:** parallel tests that redefine globals race; agents report confidently and are sometimes wrong (verify against one bot's log lines); model-only fixes grounded in the vanilla jar are cheap and safe (read the classes with `javap` before guessing mob behaviour).

## See also
- [[cc-handoff]] - where clojurecraft got.
- [[mc-mob-facts]] - the vanilla facts learned the hard way.
- [[cc-unmerged-leads]] - the fixes left unmerged.
