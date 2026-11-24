# A shield and cooked food (issue 16, the `rods_7` stage)

Status: shipped, unproven. The shield's worth is measured on a development start (two races); no real bot from a real snapshot has carried one yet, so nothing here is issue-16 evidence. Cooked food moved to the overworld flow's owner.

## Needs
The fight for 7 blaze rods costs more health than a bot can heal (`docs/problems/blaze-fight.md`, "What a fireball costs"): a fireball that lands is 2.9 in iron armour and then 3 to 6 fire ticks armour does not stop, 6 to 9 health a hit; healing 13 health took 1,000 ticks and five raw porkchops; a bot arrives with about 8 raw meat. Five rods across six armoured bots, none over 2, every bot dead (races 45337168104 and 45338756013). A player does two things there the bot could not do at all: raise a shield, and eat cooked meat.

## What the game does (26.1.2)
- A shield is used with the hand that holds it: `use_item` with hand 1 for the off hand, held until `release_use` (`player_action` status 5). The server takes the hand from the packet (`handleUseItem`), so a sword in the main hand stays there.
- It blocks only after 5 ticks up (the `blocks_attacks` component: `block_delay_seconds` 0.25), and only what comes from the front half of the look (`horizontal_blocking_angle` 90 either side, on the plane: the pitch does not count).
- A blocked hit deals nothing. A small fireball sets its target alight before the damage and puts the fire back as it was when the damage did not land (`SmallFireball.onHitEntity`), so a blocked fireball leaves no fire either. Damage types in `bypasses_shield` go through; `fireball` is not one of them (read from the jar, probe 45343432067: the tag is `bypasses_armor` and cactus, campfire, dry_out, falling_anvil, falling_stalactite, hot_floor, in_fire, lava, lightning_bolt, sweet_berry_bush; `on_fire` is in `bypasses_armor`, so fire already burning is never blocked, only never lit).
- An axe disables a shield for 5 seconds (piglin brutes; wither skeletons and blazes hold none).
- The slowdown while an item is used (a fifth of the speed, no sprint) and "no attack while using" are the client's rules, not checked by the server. They are the client's code (`LocalPlayer.aiStep`), not in the server jar. The bot is held to them anyway: while the shield is up it strikes nothing and walks at a fifth of its speed.
- A blaze sets its charge flag, waits 60 ticks, shoots three fireballs 6 ticks apart, then rests 100: the flag is up for about 78 ticks in 178. A small fireball starts slow (0.1 a tick faster each tick, drag 0.95): about 8 ticks for the first 3 blocks.

## What the bot had
- `Inventory` already holds window slot 45 (the off hand); nothing put an item there.
- `Act::UseItem` sends hand 0 only; `Act::ReleaseUse` exists (eating).
- `Entity::blaze_charged` and `Entity::velocity` exist; nothing but a counter in the fight read the flag.

## What is built
- `world::Act::UseOffHand`: `use_item` with hand 1 along the body's look.
- `plan::shield` (pure): `threats`, `should_raise`, `facing`, `held`, `to_off_hand` (a swap click, mode 2 button 40), `controls`, and the holder `Shield` (`step`): up `RAISE_AFTER` 45 ticks after a blaze's charge is first seen, or at once for a fireball within 24 blocks not flying away; `use_item` again every 20 ticks; down `HOLD_TICKS` 6 after the last threat, or at once when the hands are needed. Fireballs are faced first (the nearest), then the blaze charged longest.
- One call site: `Plan::step` calls `Plan::shield_step` after the tick's choice (fenced `>>> shield` in `crates/plan/src/lib.rs`). With the shield up any strike of that tick is dropped; the bot walks on, forward one tick in five, when its tick's way is within 60 degrees of the threat, else stands and faces it. An escape, a meal, a hunt, a dig, a window or a block to place lowers it.
- Ledger notes: "shield to the off hand from slot N", "shield raised against blaze N / fireball N at D", "shield lowered after N ticks (… M blocks heard in all)", "shield worn: N of 336 uses left, K spare shields held", and for every fireball that ended within 3.5 blocks of the bot "fireball N vanished with no damage, the shield up / down" or "fireball N hit the bot with the shield up / down", with the running counts.
- The kit: `config/goals.toml` `nether-kit` provides two shields after the pickaxes and the table (1 iron ingot and 6 planks each, at a table). With the makings not held the needs planner gathers for them, bounded by the kit's 16 tasks.
- Tests: `crates/bot/tests/shield.rs` (hand-built worlds), `crates/plan/tests/shield_row.rs` (the real goal table), and `no_fireball_of_five_volleys_lands` in `crates/bot/tests/nether_fight.rs` (the fight model: five volleys of a shooting blaze, none lands), which needed the swap click in the model (`crates/sim/src/survival.rs` `clicked`).
- `dev-shield` (`config/starts.toml`): a development start that puts a shield in the off hand of the snapshot bot, a spare and 32 cooked beef in its inventory.

## Walls

### 1. Up nearly always, striking nothing (race 45343415088, 5118f50, `dev-shield`, 2 x 1, cap 6,000)
- **What happened**: the shield worked and the fight stopped. Lane 0 stood at (129.6, 61, 171.4) from tick 1,040 to 1,900 with the shield up 866 ticks on end, cooked beef in its main hand, 6 to 12 blocks from three blazes; up 1,866 of its first 2,420 ticks. Its shield was gone at tick 2,420 ("tool gone: shield with 6 uses left"). One rod in the race; lane 1 died at 4,660.
- **What it proved**: "heard item.shield.block" and "fireball N vanished with no damage, the shield up" with health unchanged and no fire: 25 and 35 judged blocked. Lane 0 took 1 fireball hit in its 2,420 ticks behind the shield (the tick it was lowered for a fire escape) and 11 in the 3,580 after it was gone: 0.4 against 3.1 a thousand ticks, the same bot in the same place.
- **Read**: the raise came 15 ticks into every charge and held to its end (78 ticks of each blaze's 178), and with three blazes one was always charged; while up the bot stood and dropped every strike. A blocked fireball costs the shield 6 uses of 336.
- **Hits with the shield up** (lane 1, 6): a fireball first seen 3 blocks off met a shield up less than the 5 ticks the server asks, and fireballs from blazes behind the 180 degrees faced.
- **Fix** (e05e666): the raise 45 ticks into a charge (the wind-up is 60) and at once for a fireball in flight; the blaze charged longest faced; walking on behind the shield, slowed; two shields in the kit and a note when one is under a fifth.
- **Raced** (45345386828, e05e666, the same start with a spare shield, 2 x 1, cap 6,000): both bots alive at the cap (20.0 and 18.7 health). **5 rods** (lane 0 four, lane 1 one), 0.42 a thousand ticks against 0.09. Blows landed 72 against 36. Judged blocked 51 and 52; fireball hits 2 and 5 (one a ghast's), fire ticks 7 and 18 (18 hits and 98 fire ticks before). Shield up 2,126 and 2,147 of 6,000 ticks in 34 and 71 raises; the first shield at 66 of 336 uses at ticks 4,460 and 3,420 ("shield worn" fired once a lane). Cooked beef eaten: 3 and 4 of 32 (10 and 6 before).
- **Against the fight without a shield** (`docs/problems/blaze-fight.md`, race 45338756013, the same snapshot, 3 x 1): 3 of 3 dead, 2 rods, 13 fireball hits and 42 fire ticks in 10,600 bot-ticks.

### 2. Not built, known
- **Blazes on both sides**: the shield stops 90 degrees either side of the look. Standing with a wall behind (a corner, a tunnel's mouth) would put every blaze in front; it needs a cell choice from `plan::cover` (another owner's).
- **The slowdown is a duty cycle**, forward one tick in five. A use-item slowdown in `crates/physics` would be the real thing.
- **Judging is coarse**: the server's time reaches the bot twenty ticks at a time, so a hit and a fireball's end in the same twenty ticks are matched loosely; the block sound count was 0 in race 45345386828 for the same reason (counted per server tick; fixed after it).
- **No real bot has carried one**: the kit row has run only in the planner's tests.
