---
title: Combat and pacing
description: How the bot answers hostile mobs (plan::defend, issue 12d, the core of clojurecraft's combat.clj) - the defend always-row, a fight with swings paced by the weapon's cooldown and the look sent a tick before the swing, the danger line and the creeper under which it backs away instead, and the progress invariants that end a fight.
type: reference
tags: [plan, combat, defend, survival, always-rows, pacing]
aliases: [defend, Defend, Stance, danger line, danger_health, defend_radius, never_fight, creeper, swing pacing, cooldown, retreat, back away]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@b86d924 clojurecraft@be85a92
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
- **Back away** (straight away from it, jumping what is in the way): at or under `danger_health` (two zombie hits on Normal and a margin), from a `never_fight` kind at any health, and when a fight lands nothing for 10 s or runs 30 s (the progress invariants).
- **Over**: the mob is dead, gone, or `defend_radius` + 4 away.

## Evidence
- Model tests: three pass.
- Real, locally on Normal (`--goal hold`, a stone sword, a zombie and a husk with AI summoned 5 blocks off): both killed, the husk in 5 swings and the zombie in 3; the two together cost 9 health.

## Limits
- Skeletons shoot from beyond `defend_radius`: no answer until one comes near.
- No shelter yet: whether one is needed is read from the hour race's deaths (12e) before it is built.
- The model's mobs neither move nor hit; the real race is the measure.

## See also
- [[food-and-hunting]] - the hunt's strikes, paced the same way.
