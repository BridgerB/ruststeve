---
title: Races and gyms
description: How clojurecraft measures itself on real servers (gyms of one goal judged by RESULT and an RCON truth read; races of many bots on pregenerated landing sets), and what ruststeve's scenario tests should copy.
type: reference
tags: [oracle, gym, race, measurement]
aliases: [gym.clj, race.clj, landings, RESULT line, truth command, landing sets, paired runs]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:brain/bot/tooling/race.md#Game time, not the clock, because the real servers tick far slower than 20 a second:"
  - "clojurecraft:brain/bot/tooling/race.md#Every lane carries one server with 6 GB of heap:"
  - "clojurecraft:src/clojurecraft/gym.clj#\"The gym: real runs of one goal on GitHub runners, each on its own vanilla server and its own"
  - "clojurecraft:src/clojurecraft/gym.clj#A run that never got going (:harness) or lost its connection (:disconnect) says nothing about the goal and is not counted."
  - "clojurecraft:src/clojurecraft/landings.clj#\"Landing sets for the gym: fixed, pre-checked places in a pregenerated world, so run k of every"
  - "clojurecraft:src/clojurecraft/race.clj#docs/RICH-HICKEY.md, Conveyance: each bot is its own atom and loop (main/run-bot!, the same life a lone bot lives), and bots share nothing but one bounded channel, the claims of landing cells, folded by the pure claims reducer."
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[replay-limits]]"
  - "[[testing-kinds]]"
---

# Races and gyms

## Key files
- `src/clojurecraft/gym.clj`, `land.clj`, `landings.clj`, `fixture.clj`, `harness.clj`, `rcon.clj`.
- `src/clojurecraft/race.clj`, `race_facts.clj`, `claims.clj`, `start.clj` and `resources/clojurecraft/starts.edn`, `gyms.edn`.
- `ci/*.edn`: gym, race, lanes, fleet configuration as data.

## How it works
- **A gym** is real runs of one goal, each on its own vanilla server and landing, judged twice: by the bot's `RESULT` line and by the server's own answer to a truth command over RCON. `gyms` is a registry table (which goal, the `--until`, what to give first, the time limit, the truth commands). Judging is one pure function; the report is a pure fold. A run that never got going or lost its connection is not counted.
- **Landing sets**: fixed, pre-checked spots in a pregenerated world, so run *k* of every batch lands on the same spot and builds compare like with like.
- **A race**: many bots on one server, each its own atom and loop, sharing only a bounded channel of landing-cell claims folded by a pure reducer. A bot the job's end stops is cut, never failed. Milestones are facts printed as lines.
- **Starts** (`starts.edn`): spawn, or prepared starts in the Nether at a pad beside a fortress or in a warped forest, with a kit, so late stages can be worked before early ones reach them.
- The fixture is its own process speaking to the bot through a queue (stdin), never sharing state.

## For ruststeve
The guide's scenario tests are this: data-described setups, truth checked by the server, run many times, reported as rates. Copy the truth-read and paired-landing ideas; both cost little and remove most measurement noise.

## Gotchas
- Real servers tick far slower than 20 a second under load (one lane: 79 bots stopped by an hour of the clock had seen under 12 game minutes), so a start's cap is in server ticks: the bot counts 20 per `set-time` heard and stops at the cap ([[measurement-rules]]).
- Race servers run simulation distance 3, which beat 4 in an A/B; mobs farther than that are frozen ([[mc-mob-facts]]).
- One server per runner and 14 bots per server was the measured best: two servers on a runner's cores ticked 80–300 ms against 30–70 for one; 18 bots ticked about 3x slower than 14.
- All of it ran on GitHub runners, as ruststeve will ([[runners-not-the-mac]]).

## See also
- [[replay-limits]] - closed-loop vs replay.
- [[cc-sim]] - the same runs with no server.
