---
title: Combat and pacing
description: How the bot answers hostile mobs (plan::defend, issue 12d, the core of clojurecraft's combat.clj) - the defend always-row, a fight with swings paced by the weapon's cooldown and the look sent a tick before the swing, the danger line and the creeper under which it backs away instead, and the progress invariants that end a fight.
type: reference
tags: [plan, combat, defend, survival, always-rows, pacing]
aliases: [defend, Defend, Stance, danger line, danger_health, defend_radius, never_fight, creeper, swing pacing, cooldown, retreat, back away]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@ddae609 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/defend.rs#pub fn nearest_foe(w: &World, radius: f64) -> Option<(i32, EntityKind)> {"
  - "ruststeve:crates/plan/src/defend.rs#pub fn in_danger(w: &World, rules: &SurvivalRules) -> bool {"
  - "ruststeve:crates/plan/src/hunt.rs#pub fn swing_ticks(w: &World) -> f64 {"
  - "ruststeve:config/goals.toml#danger_health = 8.0"
  - "ruststeve:crates/bot/tests/defend.rs#fn under_the_danger_line_it_backs_away_instead() {"
  - "clojurecraft:resources/clojurecraft/goals.edn#:goal/doc \"answer a hostile mob within 4 blocks\""
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[escapes]]"
  - "[[food-and-hunting]]"
  - "[[wire-actions]]"
---

# Combat and pacing

Issue 12d. A bot idle at night was killed by a zombie 0.4 blocks off: the `defend` always-row answers a hostile mob near, ahead of eating and hunting and behind the escapes.

## Key files
- `config/goals.toml`: the `defend` row, and `[survival]`'s `defend_radius` (4), `danger_health` (8) and `never_fight` (`creeper`).
- `crates/plan/src/defend.rs`; the swing pacing and weapon choice shared with the hunt (`hunt::swing_ticks`, `hunt::weapon`).
- `crates/bot/tests/defend.rs`: a zombie fought and killed; under the danger line and against a creeper, backed away from (mobs in the model stand still and do not hit).

## How it works
- **In force**: a live mob of the game's `monster` category within `defend_radius` of the feet, or a defence already running.
- **Fight**: close in facing it; from 3 blocks (eye to its middle) hold the hardest-hitting weapon and strike when a full charge is ready (20 / (4 + the attack speed) ticks, by the slowdown, plus one), never in the first tick it is faced (the look goes out a tick before the swing: v1's lesson).
- **Back away** (along a route to beyond the reach plus the margin, `Goal::Away`, searched again when the foe moves 3 blocks; straight away from it, jumping, only when no route is found: issue 13b, a creeper fled into a tunnel's end): at or under `danger_health` (two zombie hits on Normal and a margin), from a `never_fight` kind at any health, and when a fight lands nothing for 10 s or runs 30 s (the progress invariants).
- **Over**: the mob is dead, gone, or `defend_radius` + 4 away.

## Evidence
- Model tests: three pass.
- Real, locally on Normal (`--goal hold`, a stone sword, a zombie and a husk with AI summoned 5 blocks off): both killed, the husk in 5 swings and the zombie in 3; the two together cost 9 health.

## Limits
- No shelter yet: whether one is needed is read from the hour race's deaths (12e) before it is built.
- The model's mobs neither move nor hit; the real race is the measure.

## Issue 13b
- **`flee_clear`** (20): a retreat from a `never_fight` kind holds until past it (a creeper follows from 16; retreats that stopped at 12 walked back to its ore, wall 29), and the planner keeps off its ground 600 ticks.
- **Unreachable ranged foes**: one that a fight landed nothing on is left alone 600 ticks unless it comes within `defend_radius` (one bot fought one skeleton 51 times with no swing, wall 30).
- **Ranged** kinds (`ranged`: skeleton, stray, bogged) are answered within `ranged_radius` (16), not only `defend_radius` (wall 16).
- **`never_fight`** kinds (creeper) are answered first and from `flee_radius` (8), backed away from until past it and the margin: from 4 the fuse was already lit (wall 21).

## Issue 15: neutral mobs
- `neutral` kinds in the survival rules (`zombified_piglin`, `enderman`) are fought only once one of their kind has hit the bot within `PROVOKED_TICKS` 400 (`defend::provoked`, from the world's hit record): defending against a zombified piglin angered its group (9 of 112 deaths in the first nether race, 1 of 224 after). Piglins stay foes: with no gold worn they attack (32 of 224 deaths before the tunnels).

## See also
- [[food-and-hunting]] - the hunt's strikes, paced the same way.

- **A foe no strike reaches** (blaze wall 15): a defend that never struck its foe, the bot no worse than at its start, leaves that foe alone for `FAILURE_MEMORY_TICKS` (any kind but `never_fight`; wall 30 had covered only shooters), and within `defend_radius` it is answered again only once its kind hits the bot.

- **A retreat that strikes** (overworld-leg wall 1): backing away over the danger line from a kind it does fight (not `never_fight`, not `ranged`; a fight that went nowhere), the bot strikes that foe when it is within `STRIKE_REACH` and a swing is ready (`Defend::retreat_strike`), the feet still backing away; the defend line then reads swings with `Retreat`. Under the danger line a retreat never strikes.
