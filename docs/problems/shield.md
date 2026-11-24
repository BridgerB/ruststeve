# A shield and cooked food (issue 16, the `rods_7` stage)

Status: open. The shield's value is being measured on a development start; nothing here is issue-16 evidence yet.

## Needs
The fight for 7 blaze rods costs more health than a bot can heal (`docs/problems/blaze-fight.md`, "What a fireball costs"): a fireball that lands is 2.9 in iron armour and then 3 to 6 fire ticks armour does not stop, 6 to 9 health a hit; healing 13 health took 1,000 ticks and five raw porkchops; a bot arrives with about 8 raw meat. Five rods across six armoured bots, none over 2, every bot dead (races 45337168104 and 45338756013). A player does two things there the bot could not do at all: raise a shield, and eat cooked meat.

## What the game does (26.1.2)
- A shield is used with the hand that holds it: `use_item` with hand 1 for the off hand, held until `release_use` (`player_action` status 5). The server takes the hand from the packet (`handleUseItem`), so a sword in the main hand stays there.
- It blocks only after 5 ticks up (the `blocks_attacks` component: `block_delay_seconds` 0.25), and only what comes from the front half of the look (`horizontal_blocking_angle` 90 either side, on the plane: the pitch does not count).
- A blocked hit deals nothing. A small fireball sets its target alight before the damage and puts the fire back as it was when the damage did not land (`SmallFireball.onHitEntity`), so a blocked fireball leaves no fire either. Damage types in `bypasses_shield` go through; `fireball` is not one of them.
- An axe disables a shield for 5 seconds (piglin brutes; wither skeletons and blazes hold none).
- The slowdown while an item is used (a fifth of the speed, no sprint) and "no attack while using" are the client's rules, not checked by the server. The bot is held to them anyway: while the shield is up it stands and strikes nothing.
- A blaze sets its charge flag, waits 60 ticks, shoots three fireballs 6 ticks apart, then rests 100: the flag is up for about 78 ticks in 178. A small fireball starts slow (0.1 a tick faster each tick, drag 0.95): about 8 ticks for the first 3 blocks.

## What the bot had
- `Inventory` already holds window slot 45 (the off hand); nothing put an item there.
- `Act::UseItem` sends hand 0 only; `Act::ReleaseUse` exists (eating).
- `Entity::blaze_charged` and `Entity::velocity` exist; nothing but a counter in the fight read the flag.

## What is built
- `world::Act::UseOffHand`: `use_item` with hand 1 along the body's look.
- `plan::shield` (pure): `threats`, `should_raise`, `facing`, `held`, `to_off_hand` (a swap click, mode 2 button 40), and the holder `Shield` (`step`): up `RAISE_AFTER` 15 ticks after a blaze's charge is first seen or at once for a fireball within 24 blocks not flying away; `use_item` again every 20 ticks; down `HOLD_TICKS` 6 after the last threat, or at once when the hands are needed.
- One call site: `Plan::step` calls `Plan::shield_step` after the tick's choice (fenced `>>> shield` in `crates/plan/src/lib.rs`). With the shield up the controls stand and face the threat and any strike of that tick is dropped. An escape, a meal, a hunt, a dig, a window or a block to place lowers it.
- Ledger notes: "shield to the off hand from slot N", "shield raised against blaze N / fireball N at D", "shield lowered after N ticks", and for every fireball that ended within 3.5 blocks of the bot "fireball N vanished with no damage, the shield up / down" or "fireball N hit the bot with the shield up / down", with the running counts.
- Unit tests: `crates/bot/tests/shield.rs` (nine, on a hand-built world).
- `dev-shield` (`config/starts.toml`): a development start that puts a shield in the off hand of the snapshot bot.

## Not built
- Crafting the shield in the race's gear (a goal row: 1 iron ingot, 6 planks) and its place in the overworld flow.
- Cooked food.
