---
title: Food and hunting
description: How the bot keeps fed (plan::eat and plan::hunt, issue 12c, ports of the core of clojurecraft's eat.clj and forage.clj) - the eat and hunt always-rows, the survival numbers as data, choosing safe food from the generated food components, prey derived from the generated mob loot, strikes paced by the weapon's cooldown, the pickup, and the model's mob drops.
type: reference
tags: [plan, food, eat, hunt, survival, always-rows, loot]
aliases: [eat, hunt, Eat, Hunt, hungry_below, unsafe_foods, hunt_radius, starving_radius, prey, game animals, MOB_DROPS, mob_drops_of, meals, kills, survival rules]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4 clojurecraft@be85a92
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
- `config/goals.toml`: the `eat` and `hunt` always-rows after the escapes, and `[survival]` (`hungry_below` 14, `unsafe_foods`, `hunt_radius` 24, `starving_below` 6, `starving_radius` 48, `food_reserve` 32), read into `make::SurvivalRules`.
- `crates/plan/src/eat.rs`, `crates/plan/src/hunt.rs`; `Plan::always` and `Plan::choose_prey` in `crates/plan/src/lib.rs`.
- `crates/bot/tests/food.rs`: eating bread, and a cow hunted, its beef picked up and eaten, on Normal against the model.

## How it works
- **Eat** (in force with safe food held when the food the server sent is under `hungry_below`, or to heal: health under `engage_health` with food under 18, `eat::eat_to_heal`): the most nutritious food not in `unsafe_foods` (values from the generated `food` components); hold it, `use_item`, keep the hand still; done on the server's entity event 9 or the food rising; no answer in the eating time and 3 s fails (and releases the use).
- **Hunt** (prey in view, and either hungry with nothing safe held or provisioning): prey are creatures whose generated loot holds a safe food (cows, pigs, sheep, rabbits; chickens are left, raw chicken being unsafe); within `hunt_radius`, `starving_radius` at 6 food or under. The nearest of the first three a route reaches (the churn invariant); walk to within 2, aim again when it moves 2; from 3 blocks (eye to its middle) face it, hold the hardest-hitting weapon, strike when a full charge is ready (20 / (4 + the weapon's speed) ticks, by the slowdown, plus one); after the kill pick up its food drops within 4 of where it died. A minute at most. A hunt picks up all it dropped before the meal: only a row ahead of it (an escape, `defend`) cuts it. `Hunt::extra` adds drops picked up besides the food (the bow line's feathers and string, [[end-bow]]; the junk list keeps 16 feathers and 8 string).
- **Provisioning** (issue 16a, `Plan::provisioning`): aiming at the Nether or the race, in the overworld, with the safe food held (`eat::food_points`, nutrition times count) under `food_reserve`. Animals in view are hunted then without being hungry; with a portal to enter and the reserve short the bot explores for animals before stepping in, and `Plan::clean_flow`'s stocking phases explore for them until `provision_ticks` times three has passed.
- A swim off an island is not cut by hunger or a hunt: they wait for the far shore.
- A failed eat or hunt is not chosen again for `RETRY_TICKS`; a failed hunt's animal is refused.
- **The model** drops a mob's unconditioned loot on a kill (counts rolled in their range, `spawnAtLocation`'s push and 10-tick pickup delay); a parity row's mob is summoned without.

## Evidence
- Model tests: both pass.
- **Real, locally on Normal** (the bot on `--goal hold`, a hunger effect, a stone sword, a cow with no AI summoned four blocks off): it hunted the cow and ate its beef, hunted a wild cow (3 swings) and ate again.
- The parity server runs peaceful, which keeps the food full, so eat and hunt are not parity rows; the real race is their measure.

## Limits
- Raw meat only: cooking is issue 13's furnace.
- No forage walk for animals out of view when only hungry (clojurecraft's `:forage`); only provisioning explores for them.
- Hostile mobs while hunting are 12d's.

## See also
- [[escapes]] - the always-rows ahead of these.

- **A stock of cooked meat before the Nether** (docs/problems/overworld-leg.md wall 4): with `meat_stock` set (30 pieces) the race hunts on the surface for one `provision_ticks` before it goes down, the bot resumed from a `ready` snapshot too, then cooks every raw piece (`Flow::Cook`, `cook_goal`: planks for the whole cook first, then a furnace made and placed where it stands, a batch a kind through the smelt task). A kind whose meat cooks into safe food is game, so chickens are hunted; raw chicken is never eaten. `eat::meat_pieces` counts pieces, `eat::food_points` each item's own hunger, so cooked pieces count 8.
- **The kit's iron and shields at the cook site** (overworld-leg walls 4 and 8): after the meat, `cook_goal` smelts held raw iron until `kit_ingots` (3) ingots are held, a shield made counting as its ingot, then crafts shields at the cook's table up to `kit_shields` (2) from the ingots held ("cook-shield"; planks from held logs; ledger "shields at the cook site: …"). The "cooked" note counts ingots and shields.
- **Warped and crimson wood is no fuel, its sticks are** (issue 19d): the jar's `#non_flammable_wood` holds every warped and crimson item, but not `stick`, which burns 100 ticks whatever planks made it. `cook_goal` counts sticks over `STICK_RESERVE` as fuel, and its fuel row counts only planks that burn; with nether wood held it names the planks of a held log that burns ("item:oak_planks"), else sticks ("item:stick", crafted from the warped planks). Test `warped_wood_burns_as_sticks_and_the_cook_asks_for_them` (plan home).
- **The cook's hut** (overworld-leg wall 11): before the first smelt on the surface the bot walls itself in with held cobblestone (`cover::hut_block`, `Plan::hut_step`), and hunts nothing while walled in; the fuel row holds the shields' planks as well.
- **A meal has a cap, and the dragon cuts one** (issue 20g, end-dragon walls 39 and 44): a running eat is cut, its use let go, past `dragon::MEAL_CAP_TICKS` (100: the 32 of a meal and the 60 of its answer), in any dimension, and in the End at once unless the dragon is in its holding pattern more than `MEAL_CLEAR` (40) off and no cloud is on the feet (`Plan::meal_cut`, `dragon_holds_meal`; ledger "eat cut: …"). An eat held off by a row froze for 2,360 ticks (dev race 46048455555 lane 1).
