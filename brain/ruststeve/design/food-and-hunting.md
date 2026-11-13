---
title: Food and hunting
description: How the bot keeps fed (plan::eat and plan::hunt, issue 12c, ports of the core of clojurecraft's eat.clj and forage.clj) - the eat and hunt always-rows, the survival numbers as data, choosing safe food from the generated food components, prey derived from the generated mob loot, strikes paced by the weapon's cooldown, the pickup, and the model's mob drops.
type: reference
tags: [plan, food, eat, hunt, survival, always-rows, loot]
aliases: [eat, hunt, Eat, Hunt, hungry_below, unsafe_foods, hunt_radius, starving_radius, prey, game animals, MOB_DROPS, mob_drops_of, meals, kills, survival rules]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@b86d924 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/eat.rs#pub fn best_food(w: &World, rules: &SurvivalRules) -> Option<ItemKind> {"
  - "ruststeve:crates/plan/src/hunt.rs#pub fn game(kind: EntityKind, rules: &SurvivalRules) -> bool {"
  - "ruststeve:crates/plan/src/lib.rs#fn choose_prey("
  - "ruststeve:config/goals.toml#hungry_below = 14"
  - "ruststeve:crates/bot/tests/food.rs#fn hungry_with_nothing_it_hunts_a_cow_and_eats_the_beef() {"
  - "clojurecraft:src/clojurecraft/eat.clj#(def hungry-below 14)"
  - "clojurecraft:src/clojurecraft/forage.clj#(def hunt-radius 24)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[escapes]]"
  - "[[planner]]"
  - "[[server-model]]"
  - "[[datagen]]"
---

# Food and hunting

Issue 12c. An hour of play spends the food bar (jumps, digging, healing), and on Normal an empty bar starves the bot down to 1 health, where any hit kills: the bot eats, and with nothing to eat it hunts.

## Key files
- `config/goals.toml`: the `eat` and `hunt` always-rows after the escapes, and `[survival]` (`hungry_below` 14, `unsafe_foods`, `hunt_radius` 24, `starving_below` 6, `starving_radius` 48), read into `make::SurvivalRules`.
- `crates/plan/src/eat.rs`, `crates/plan/src/hunt.rs`; `Plan::always` and `Plan::choose_prey` in `crates/plan/src/lib.rs`.
- `crates/bot/tests/food.rs`: eating bread, and a cow hunted, its beef picked up and eaten, on Normal against the model.

## How it works
- **Eat** (in force when the food the server sent is under `hungry_below` and safe food is held): the most nutritious food not in `unsafe_foods` (values from the generated `food` components); hold it, `use_item`, keep the hand still; done on the server's entity event 9 or the food rising; no answer in the eating time and 3 s fails (and releases the use).
- **Hunt** (hungry, nothing safe held, prey in view): prey are creatures whose generated loot holds a safe food (cows, pigs, sheep, rabbits; chickens are left, raw chicken being unsafe); within `hunt_radius`, `starving_radius` at 6 food or under. The nearest of the first three a route reaches (the churn invariant); walk to within 2, aim again when it moves 2; from 3 blocks (eye to its middle) face it, hold the hardest-hitting weapon, strike when a full charge is ready (20 / (4 + the weapon's speed) ticks, by the slowdown, plus one); after the kill pick up its food drops within 4 of where it died. A minute at most. A hunt picks up all it dropped before the meal: only an escape cuts it.
- A failed eat or hunt is not chosen again for `RETRY_TICKS`; a failed hunt's animal is refused.
- **The model** drops a mob's unconditioned loot on a kill (counts rolled in their range, `spawnAtLocation`'s push and 10-tick pickup delay); a parity row's mob is summoned without.

## Evidence
- Model tests: both pass.
- **Real, locally on Normal** (the bot on `--goal hold`, a hunger effect, a stone sword, a cow with no AI summoned four blocks off): it hunted the cow and ate its beef, hunted a wild cow (3 swings) and ate again.
- The parity server runs peaceful, which keeps the food full, so eat and hunt are not parity rows; the real race is their measure.

## Limits
- Raw meat only: cooking is issue 13's furnace.
- No forage walk for animals out of view (clojurecraft's `:forage`).
- Hostile mobs while hunting are 12d's.

## See also
- [[escapes]] - the always-rows ahead of these.
