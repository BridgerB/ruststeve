---
title: Combat and pacing
description: How the bot answers hostile mobs (plan::defend, issue 12d, the core of clojurecraft's combat.clj) - the defend always-row, a fight with swings paced by the weapon's cooldown and the look sent a tick before the swing, the danger line and the creeper under which it backs away instead, and the progress invariants that end a fight.
type: reference
tags: [plan, combat, defend, survival, always-rows, pacing]
aliases: [defend, Defend, Stance, danger line, danger_health, defend_radius, never_fight, creeper, swing pacing, cooldown, retreat, back away]
status: verified
lastUpdated: 2026-11-26
verifiedAgainst: ruststeve@34ec399 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/defend.rs#pub fn nearest_foe(w: &World, radius: f64) -> Option<(i32, EntityKind)> {"
  - "ruststeve:crates/plan/src/defend.rs#pub fn in_danger(w: &World, rules: &SurvivalRules) -> bool {"
  - "ruststeve:crates/plan/src/hunt.rs#pub fn swing_ticks(w: &World) -> f64 {"
  - "ruststeve:config/goals.toml#danger_health = 8.0"
  - "ruststeve:crates/plan/src/shield.rs#pub fn should_raise(w: &World) -> bool {"
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
- `neutral` kinds in the survival rules (`zombified_piglin`, `enderman`) are fought only once one of their kind has hit the bot within `PROVOKED_TICKS` 400 (`defend::provoked`, from the world's hit record): defending against a zombified piglin angered its group (9 of 112 deaths in the first nether race, 1 of 224 after). Piglins stay foes: with no gold worn they attack (32 of 224 deaths before the tunnels). Issue 17: an enderman is never a foe, provoked or not (`defend::NEVER_FOE`, in `foe` and `pressing`): 40 health and 7 a hit beat the bot in the open, and a provoked one is answered by a sealed pocket (`Plan::ender_threat`, [[planner]]).

## See also
- [[food-and-hunting]] - the hunt's strikes, paced the same way.

- **A foe no strike reaches** (blaze wall 15): a defend that never struck its foe, the bot no worse than at its start, leaves that foe alone for `FAILURE_MEMORY_TICKS` (any kind but `never_fight`; wall 30 had covered only shooters), and within `defend_radius` it is answered again only once its kind hits the bot.

- **A retreat that strikes** (overworld-leg wall 1): backing away over the danger line from a kind it does fight (not `never_fight`, not `ranged`; a fight that went nowhere), the bot strikes that foe when it is within `STRIKE_REACH` and a swing is ready (`Defend::retreat_strike`), the feet still backing away; the defend line then reads swings with `Retreat`. Under the danger line a retreat never strikes.
## Issue 16: shooters at the blaze spawner (docs/problems/blaze-fight.md)
- **Armour first**: in the Nether a held iron piece whose armour slot is empty is quick-moved on at the top of the always-rows (`unworn_armour`), since `equip` runs only when a task is chosen and a defend cuts in before that.
- **A shooter's fight** (`ranged` kinds) strikes whichever mob of the foe's kind is in reach, measured from the eye to its box and only in plain sight (`in_reach`, `box_distance`, `sight`), and is given up after `shooter_patience` (100 ticks) with nothing landed.
- **A shooter's retreat** starts at `shooter_retreat_health` (13: the danger line and one fireball's fire) and goes to cover, not to a distance: it ends where no shooter has a line to the bot's body, else walks to the nearest cell out of sight (`plan::cover`'s `shooters`, `seen_body` and `cover_cells`). A shooter is met with a retreat from the start at or under that health.
- **Touch and sweep**: with a mob of the foe's kind within `KEEP_OFF` 2.4 the fight steps away and strikes on (a blaze's touch is 3.6 in iron armour, once a second inside 2 blocks); it steps only onto floor with no lava or fire ahead (`safe_step`); and it holds its swing while a `neutral` mob stands within `SWEEP_NEAR` 2.0 of the foe (a sweep that struck a zombified piglin killed a bot).
- **The pressing foe**: every tick, while its own foe is farther than `defend_radius` or gone, a running defend turns to the nearest hostile within `defend_radius`, else to the nearest mob of a kind that hit the bot in the last 40 ticks (`defend::pressing`), with that foe's stance. A shooter whose feet are more than a strike's reach over the eye is not gone after from afar, and a shooter's fight with no step toward its foe goes to cover after 20 ticks.
- **The dodge**: in a shooter's Fight each small fireball whose line passes within `DODGE_CLEAR` 1.5 of the body's middle starts one 8-tick walk across its line (`dodge_side`; a fireball with no velocity sent is taken to fly at the body), the strikes going on.
- **Its notes** reach the ledger through `Defend::notes`: "struck N in reach, not M", "shooter fight given up", "retreat to cover at", "in cover at", "defend switched to"; the fight's closing note counts ticks kept off, sweeps held and fireballs dodged.
- **Wither skeletons are fought** (spawner wall 1, `docs/problems/spawner.md`): they were `never_fight` from 2026-11-20, chosen for bare bots; a wither skeleton walks as fast as the bot, so a retreat from one never reaches `flee_clear`, and two armoured bots backed away 700 and 900 ticks and died with no swing (race 45334573113). `never_fight` is the creeper alone again.
- **Cover and shelter** (`plan::cover`, `Plan::take_cover`; spawner wall 3): under `engage_health` in the Nether the bot rests only where no shooter sees its body (blazes and ghasts within 48, piglins and skeletons within 16; lines to its eye and middle) and nothing burns. Seen, it walks to the nearest cell out of sight within 8 on a found route; with none it builds cover where it stands, a held block into each cell a line of sight comes in through (`cover::shelter_block`, one every 10 ticks, cobblestone first). The range was 16 until three armoured bots rested on an open bridge 16 to 24 blocks from blazes and died there (race 45335627948).
- **Heal beyond the spawner's range** (spawner wall 5): a spawner runs only with a player within 16 blocks, so no rest and no cover cell lies within `cover::SPAWNER_QUIET` (18) of a spawner on record; a hurt bot inside it walks away from the spawner on a found route first ("leave the spawner at … to heal"). A cover cell stands on a full block, passes `seen_body` for a body standing in it, and is refused for 600 ticks once walked to.
- **Cover is never beside a drop, and a rest must be able to heal** (spawner walls 6 and 7): `cover::beside_drop` keeps cells at a bridge's edge out of `cover_cells` (two bots fell 17 blocks heading for one); `take_cover` rests only while `can_heal` holds (safe food held, or the food bar at 18+), else the bot goes on over `danger_health` (two bots sat 7,400 and 7,900 ticks at 16 health with their meat eaten).
- **The leave walk and the retreat line agree with the rest** (spawner wall 8, blaze-fight wall 13): the leave walk out of a spawner's range takes its goal 2 past the feet by the route's measure (octile plus height runs longer than the straight line `awake_at` uses; a goal of 20 was met at once 300 times while the spawner was 17.2 off); and a shooter's retreat line is `defend::retreat_line`: `shooter_retreat_health` while a rest can heal, else the danger line.

## Issue 16: the shield (docs/problems/shield.md)
- **What it is**: `plan::shield`. A shield in the off hand (window slot 45) is raised with `Act::UseOffHand` (`use_item`, hand 1) and lowered with `Act::ReleaseUse`. Raised 5 ticks or more it stops what comes from the front half of the look, a blaze's fireball whole: no damage and no fire.
- **When**: `threats` lists charged blazes within 48 that see the bot and small fireballs within 24 not flying away; `should_raise` and `facing` read it. The holder `Shield::step` raises `RAISE_AFTER` 45 ticks after a blaze's charge flag is first seen (the wind-up is 60; at 15 the shield never came down among three blazes) or at once for a fireball, faces fireballs first and then the blaze charged longest, sends `use_item` again every 20 ticks, and lowers 6 ticks after the last threat.
- **The call site**: `Plan::shield_step`, called once from `Plan::step` after the tick's choice. While the shield is up that tick's strike is dropped and the bot walks on only toward the threat (within 60 degrees), forward one tick in five (`shield::controls`: a client using an item cannot attack and moves at a fifth of its speed); else it stands and faces the threat; an escape, a meal, a hunt, a dig, a window or a block to place lowers it. A shield held elsewhere is swapped into the off hand first (`to_off_hand`: click mode 2, button 40).
- **Its notes**: "shield raised against blaze N / fireball N", "shield lowered after N ticks", and each fireball that ended within 3.5 blocks judged: "vanished with no damage, the shield up / down" or "hit the bot with the shield up / down", with running counts; "shield worn: N of 336 uses left" under a fifth.
- **The kit**: after the `nether-kit` row's tools, the `shields` or `shield` row (`shield::kit_row`: what the held iron and wood make) is worked where the bot stands; an act that would be a trip skips it with a note (`shield::kit_act`). A blocked fireball takes 6 of a shield's 336 uses.
- **In the Nether** (shield wall 3): a bot that came through before the kit made shields, holding iron and wood for one, makes it on arrival (`Plan::nether_shield`, fenced in `nether_task`): the needs planner asked for the shield row alone, work where it stands only, at most 12 tasks; then the fuel left cooks the raw meat held (`shield::cook_choice`, two rounds), the table is dug again, and the note "shield stop over: …" ends it.
- **Measured** (development start, races 45343415088 and 45345386828): 0.4 fireball hits a thousand ticks behind the shield against 3.1 after it broke; with the timed raise 5 rods in 12,000 bot-ticks, both bots alive.
- **Seven rods, fire, touch and safe ground** (blaze-fight walls 17 to 20): with 7 rods held blazes are no foe and the bot walks to `LEAVE_RANGE` 50 from the spawner before the aim is met; a deck cell with fire in or beside it is no post (`defend::fire_near`); a blaze in touch keeps the tick from the shield (`defend::blaze_in_touch`); the Nether shield is made only where `cover::unsafe_to_work` is `None`.
- **The shield and the fight** (shield wall 4): `Shield::step` takes a `Duty`. It yields to the strike (lowered at once, "shield yields to the strike") while `shield::striking` names a blaze in touch or, in a fight, the foe or a mob of its kind within 4 of the eye when that kind is melee; a shield up already stays up for a fireball landing in front within 6 ticks. A strike the raised shield drops is given back to the defend (`shield::SwingCount`: its count and cooldown). A click to place a block waits while a threat is due ("placing held for the shield") and is made with the shield down otherwise. Model, 32 seeds: shield loop 29 to 30 alive with 7 rods, wither-skeleton loop 23 to 26 (probe 45364552425).
- **Issue 18**: the hunt strikes endermen from its burrow ([[pearl-hunt]]); its reach is measured to the enderman's 2.9-high box (`defend::box_distance_of`), not the blaze's 1.8. `NEVER_FOE` stays: the defend row never takes one. **Issue 20g**: the ender dragon joins `NEVER_FOE`; it is the dragon fight's own (`plan::dragon`, [[planner]]), struck only from behind its body.
