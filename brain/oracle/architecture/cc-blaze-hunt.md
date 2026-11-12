---
title: The blaze hunt
description: What clojurecraft actually does to get blaze rods (cover, meals, rests, fights paced by the clock, the spawner camp, pockets, dodging) and what its recordings measured, the real answers to the guide's blaze hypotheses.
type: reference
tags: [oracle, blaze, combat, nether]
aliases: [blaze rods, :blaze-rods, camp, pocket, dodge, combat, fireballs, retreat-health]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/blaze.clj#Blaze rods (issue #262): the :blaze-rods goal, done when rods-wanted are held: six give the"
  - "clojurecraft:src/clojurecraft/combat.clj#ruststeve's swings, paced by ticks that collapsed under the packet flood, sent forty in 66 ms, all but one inside the mob's 10-tick hurt invulnerability."
  - "clojurecraft:src/clojurecraft/dodge.clj#Read from blaze-race-l race 2's recording (volleys.ts): 212 small fireballs spawned within 30 blocks of the bot, 24 hit it; every one that hit passed within 1.4 blocks of its body, none of the 76 whose line passed 1.5 or more away did."
  - "clojurecraft:src/clojurecraft/pocket.clj#Blaze-race-g and -i, 20 deaths: nine fought or backed off in the open while burning or under a volley, three stood frozen in a fire or a blaze's reach, and fireballs hit 3 to 10 times a run with the burning after them taking 7 to 45."
  - "clojurecraft:src/clojurecraft/fire.clj#The real numbers: race f8b28e3, lane 2, 45 recordings, 3599 blazes in view (test/clojurecraft/real_blaze.edn): the burn was 1.0 per hurt in 1778 hurts, 45% of all health the blaze fights cost, and a small fireball's hit sets fire for five seconds (SmallFireball)."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[blaze-debugging-method]]"
  - "[[mc-blaze]]"
  - "[[cc-pearls-rule-table]]"
---

# The blaze hunt

The guide's blaze section lists generic hypotheses. clojurecraft has measured answers; they replace the guesses wherever they disagree.

## Key files
- `src/clojurecraft/blaze.clj`: the `:blaze-rods` goal (rods-wanted: six for twelve eyes, one spare), the `:camp` and `:rest` executors.
- `src/clojurecraft/combat.clj`: `:fight`, `:back-off`, the `:defend` row, swing pacing.
- `src/clojurecraft/pocket.clj`: a fighting pocket. `src/clojurecraft/dodge.clj`: fireball lines. `src/clojurecraft/fire.clj`: burning.

## How it works
The `:blaze-rods` row, in order: hurt under `combat/retreat-health` → into a cell out of the watching blazes' sight; eat when food is under the healing level; back away from the nearest watcher; rest out of sight until health is back (only when the rest can heal); watched by a volley → cover whatever the health; pick up a rod lying near (a kill drops one half the time, so none is no failure); fight the nearest blaze within fight-radius; put out a fire by the spawner; walk or tunnel to the remembered spawner and camp it; with none known, the fortress row's spiral search. A blaze within 4 is fought first when not hurt.

## What the recordings measured
- **Swings by the clock**: ruststeve's tick-paced swings collapsed under the packet flood (forty in 66 ms, all but one inside the 10-tick hurt invulnerability). clojurecraft swings every 650 ms by the clock, aims at the body as it hovers, and sets the look a tick before the attack because the server judges reach from the last pose.
- **Burning is the killer**: over 45 real recordings, burning was 45% of all health the blaze fights cost; a small fireball's hit sets fire for five seconds; no water works in the Nether.
- **Fireball lines**: of 212 small fireballs within 30 blocks, 24 hit; every hit passed within 1.4 blocks of the body, none of 76 passing 1.5 or more away hit. A fireball arrives 300 to 750 ms after spawning, so a step or two clears most.
- **Deaths in the open**: of 20 deaths, nine fought or backed off in the open while burning or under a volley, hence the pocket: one opening, solid elsewhere, so at most one blaze has a line.

## Gotchas
- The blaze's charge flag is set 1.4 to 3.5 s before its fireballs spawn, with no direction in it (`dodge.clj`).
- A zombified piglin that has struck is backed away from, never rested beside.

## See also
- [[blaze-debugging-method]] - the guide's method.
- [[mc-blaze]] - the game facts.
