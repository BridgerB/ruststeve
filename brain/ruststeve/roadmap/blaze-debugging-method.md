---
title: Debugging a stuck point (the blaze method)
description: The method for any stuck point, worked on the blaze fight: the hypotheses, the evidence that confirms each, and the written diagnosis template.
type: how-to
tags: [blaze, debugging, combat, diagnosis]
aliases: [blaze fight, getting unstuck, diagnosis template, blaze hypotheses]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#The blaze fight is where every previous version got stuck, so it's the template for debugging any stuck point."
  - "ruststeve:FUNCTIONAL.md#These are common ways a bot fails against blazes. They are hypotheses, not findings. The recordings decide which ones apply."
  - "ruststeve:FUNCTIONAL.md#- Misconception, one of:"
related:
  - "[[ruststeve/roadmap/_moc|Roadmap]]"
  - "[[incident-to-regression]]"
  - "[[mc-blaze]]"
  - "[[cc-blaze-hunt]]"
---

# Debugging a stuck point (the blaze method)

The blaze fight is where every earlier bot got stuck, so it is the template for any stuck point: capture automatically, reproduce exactly, compare belief to truth, name the misconception in writing, guard it with a regression test, prove the fix closed-loop.

## Key files
- `FUNCTIONAL.md`, section 16.

## Hypotheses and their evidence
These are hypotheses; the recordings decide.

| Misconception | Evidence in the recording | Typical fix |
|---|---|---|
| Stale target position | `LookAt` lags later `EntityMoved`; attacks miss their expectation | re-resolve every tick, lead the aim |
| Out of reach | `Attack` while `target_dy` or `dist` exceeds reach | reposition, ranged option, wait for descent |
| Cooldown ignored | several attacks a second, small drops | gate melee on `attack_ready` |
| Standing in the volley | health drops in threes while `target_charging` and intent is hold/approach | break sight, strafe or block in the wind-up; strike in the rest |
| Pathing loop at the spawner | oscillation trigger; `Goto` `NoProgress` | precomputed covered standing spots |
| Target thrashing | target ids alternate tick to tick | hysteresis, commit for a minimum |
| Rods not collected | item entities after kills, no inventory change | a `PickUp` task; give up items over lava |
| Too far from the spawner | no new blazes appear | stand within its 16-block activation range |
| Wrong completion predicate | enough rods, phase does not advance | fix the predicate, unit test it |

## The workflow
1. Read `incidents/<tick>-watchdog-FightBlaze/INCIDENT.md`.
2. `bot-replay run <rec> --from-checkpoint <a> --until <b> --verify`.
3. Look: `rerun window.rrd`; `bot-replay events --kind CombatFrame ... --json | jq`.
4. Write the diagnosis.
5. `bot-replay cut ... -o recordings/regressions/<name>.jsonl`, add the property test.
6. Change the rules; `bot-replay diff --old <commit> --new HEAD <regression>` must diverge only where intended.
7. Run the blaze scenario batch before and after.

## Diagnosis template
Incident and ticks; **describe** in observable terms; **expected** behaviour; **beliefs** at the decisive tick (from `CombatFrame` and the snapshot); **truth** at that tick (later observations or a server check); **misconception**: a wrong belief, a wrong rule given correct beliefs, or an action not carried out as intended; **delimit** to one problem; **fix**; **evidence**: regression test name and scenario rate before → after.

## Limits
The game facts behind the hypotheses are in [[mc-blaze]]. What clojurecraft actually does at the blaze, and where it still fails, is in [[cc-blaze-hunt]]; those findings should replace the generic hypotheses wherever they disagree.

## See also
- [[rule-tables]] - the shape the blaze rules take.
- [[incident-to-regression]] - the general pipeline.
