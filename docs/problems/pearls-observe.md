# Pearls: measure and observe (issue 18a and 18b, agent P1)

The orchestrator's index is [`pearls.md`](pearls.md). This file holds what P1 measured (18a.1 to 18a.3, later 18a.5) and what it built so the hunt can be seen before it is built (18b). One world throughout: seed `typecraft`, the race server's.

## 18a.1: the two real `endermen` snapshots (probe 45529738504, `results player-dat`)

| | lane 3 (the frontier, `rs-l03-b00`) | lane 2 (`rs-l02-b00`) |
|---|---|---|
| where | (13.70, 84.04, 71.72), soul sand valley, server tick 3,400 | (130.50, 61.0, 167.21), nether wastes at the spawner, tick 40 |
| health | 12.47 | 16.3 |
| food, saturation, exhaustion | 17, 0, 2.79 | 20, 0, 0.14 |
| iron sword | damage 89 of 250: **161 uses left** | damage 74: **176 left** |
| stone pickaxes | damage 118 (13 left) and fresh (131): **144** | the same: **144** |
| armour (iron helmet, chest, legs) | damage 21 each (144, 219, 204 left) | damage 17 each |
| shield | none | off hand, damage 288 of 336: **48 left** |
| food held | none | 1 raw porkchop |
| fuel | coal 1 | none |
| other | raw iron 7, sticks 3, 7 rods, furnace, flint and steel, 2 water buckets, bucket, arrows 2, gold nuggets 6, soul soil 50, leather 4; filler 126 cobblestone, 64 cobbled deepslate, 89 granite, 297 netherrack, 24 dirt, 23 tuff, 8 gravel | the same kit, granite 64, netherrack 322, no arrows |

What follows (FoodData, probe 45529798802): lane 3 at food 17 never heals (natural healing needs food 18). Lane 2 at food 20 with saturation 0 heals 1 every 80 ticks at 6 exhaustion a point (1.5 food), so about 2 points before food falls under 18, then the porkchop (3 food) about 2 more.

## 18a.2: geography from the frontier (probe 45529738504; development knowledge only, no code reads it)

`locate biome` from the frontier (8, 85, 81), the same at y 64:
- warped forest (488, 85, -111): **516 blocks**, east-north-east (dx 480, dz -192);
- crimson forest (8, 85, 17), 64 north; nether wastes (-24, 85, 113), 45; basalt deltas (168, 85, 49), 163 east; the frontier itself is in a soul sand valley.

From lane 2's spawner point (130, 61, 167): warped forest (482, 61, -121), **454**; soul sand valley 32 west, basalt deltas (194, 61, 135) 71, crimson forest (98, 61, 295) 131.

The strip map, every 16 blocks to 640 on 8 axes from (8, 81) (the biome at y 64 of each point):
- **No axis crosses a warped forest within 640.**
- E: soul sand valley to 144, then **basalt deltas from 160 to 640** without a break.
- NE: soul sand valley to 112, nether wastes 128 to 304, crimson forest 320 to 640.
- N: valley, crimson 48 to 80, wastes to 528 (valley 192 to 208), deltas from 544.
- SE: valley to 80, wastes to 336, deltas 352 to 464, wastes to 640.
- S: valley to 64, wastes, crimson 208 to 416, wastes, deltas from 576.
- SW: wastes to 176, deltas 192 to 336, wastes, crimson from 432.
- W: wastes to 336, valley 352 to 544, wastes, crimson from 624.
- NW: wastes to 384, deltas 400 to 528, wastes.
- Nearest warped forest seen from the E axis points: 32 blocks from (504, 81) (at (504, 49)), 45 from (472, 81); from the NE axis points never under 204. **The forest lies north of the E axis around x 470 to 540, z -127 to 49**, all behind the E axis's 300+ blocks of deltas; the NE heading reaches crimson forest instead.

## 18a.3: the 26.1.2 jar (probe 45529798802, `javap`; the EnderMan goal classes from 45472254333)

**Melee reach.** `Mob.isWithinMeleeAttackRange`: with no `attack_range` item component, the mob's own box `inflate(r, 0, r)` with `DEFAULT_ATTACK_REACH = sqrt(2.04) - 0.6 = 0.828`, intersecting the target's hitbox (no vertical growth; line of sight also needed to strike). Centre to centre at 0.6 widths: 0.3 + 0.828 + 0.3 = **1.43**, the plan's number.

**MeleeAttackGoal.** `canUse` every 20 ticks: a live target and `createPath(target, 0)` not null; **no `canReach` check, so a partial path is accepted**. With `followingTargetEvenIfNotSeen = false` (the enderman's) it continues while the path is not done. Repath when the target is seen and moved 1+ (or 5% a tick) every 4 to 10 ticks, +10 past 32 blocks, +5 past 16. Attack interval 20 ticks; an attack needs the cooldown, the reach and line of sight.

**Pathing for a 2.9-high mob.** `NodeEvaluator.prepare`: cells `floor(w + 1) x floor(h + 1) x floor(w + 1)` = **1 x 3 x 1 for the enderman**; a node needs all three cells passable, so a 2-high mouth is no node. Node budget `FOLLOW_RANGE x 16` = 1,024, max path length 64. An unreachable target still returns the path to the best node (least heuristic distance): **it walks to the closest reachable cell, O**, then stands (each `canUse` makes the same path).

**EnderMan.** Health 40, speed 0.3 (+0.15 with a target), attack damage 7, follow range 64, step 1.0. Data indexes 16 carried block, **17 creepy**, 18 stared at. `hurtServer`: a projectile (or potion) teleports it up to 64 tries and does no harm; any other source whose entity is not living teleports it 9 times in 10; **a melee blow from a player never teleports it.** Water or rain hurts it 1 (no entity: teleports). `LookForPlayerGoal.tick`: stared at within 4 (distSqr 16) it teleports away; with its target past 16 (distSqr 256) it `teleportTowards` the target every 30 ticks. `teleportTowards`: 16 blocks along the line to the target plus up to 4 sideways and 8 up or down; a landing must be on a motion-blocking block, not water.

**Taking blocks** (`EndermanTakeBlockGoal`): with `mobGriefing`, one chance in `reducedTickDelay(20)` a tick, a random cell in x and z within 2 of it and y from its feet to 2 above, by a clear clip from its block, and only `#enderman_holdable`. Holdable: the dirt, mud, moss, grass-block and small-flower tags, sand, red sand, **gravel**, mushrooms, TNT, cactus, clay, pumpkins, melon, **crimson and warped nylium, fungi and roots**. **Not holdable: cobblestone, netherrack, cobbled deepslate, deepslate, granite, tuff, soul soil, soul sand, warped wart, stems.** Its feet's own floor is below its range (y starts at its feet), but a burrow wall or roof of dirt or gravel within reach is takeable: build only from cobblestone, netherrack, deepslate, granite, tuff.

**Loot** (`entities/enderman.json`): one ender pearl at count uniform 0 to 1 (half the time), plus looting; **no killed-by-player condition.**

**Items.** A mob drop's pickup delay is **10 ticks**; it despawns at age **6,000**. The player picks up whatever touches its box `inflate(1.0, 0.5, 1.0)` (1.3 from its centre sideways).

**Player.attack.** Damage `x (0.2 + s^2 x 0.8)` with `s = getAttackStrengthScale(0.5)`, `clamp((ticker + 0.5) / (20 / attack_speed))`: an iron sword (1.6) is full at 12.5 ticks. A crit (x1.5) needs `s > 0.9`, falling (`fallDistance > 0`), not on ground, not climbing, not in water, no blindness, not riding and **not sprinting**. Knockback 0.4 base (in `LivingEntity`) plus the attribute and 0.5 for a sprint at full strength. A hit costs 0.1 exhaustion.

**FoodData.tick.** Every 4.0 exhaustion takes a saturation point, else a food point. With saturation, food 20 and hurt: heal `min(sat, 6) / 6` every 10 ticks. Else food 18 or more and hurt: heal 1 every 80 ticks for 6.0 exhaustion. Food 0: 1 damage every 80 ticks, down to 1 health on normal.

**Biomes** (features and spawns):
- warped forest: monsters enderman only (weight 1, groups of 4), spawn cost charge 1.0, budget 0.12; features warped fungi, vegetation, sprouts, twisting vines and the `_normal` mushrooms only (chance 256 and 512 at the top heightmap: on the roof). **No usable mushrooms in the warped forest.**
- nether wastes and basalt deltas: `brown_mushroom_nether` and `red_mushroom_nether` (chance 1 in 2 a chunk, 96 tries spread 7 sideways and 3 up or down, any height, in air). **Mushrooms (stew) are a find in the wastes or deltas on the way, never in the forest.** Wastes spawn ghast 50, zombified piglin 100, magma cube 2, enderman 1, piglin 15; deltas ghast 40, magma cube 100.
- soul sand valley: no mushrooms; skeleton 20, ghast 50, enderman 1 (costs 0.7 / 0.15 each).
- crimson forest: zombified piglin 1, hoglin 9, piglin 5; only the `_normal` mushrooms.

**Piglins.** `PiglinAi.isWearingSafeArmor`: any armour slot item in `#piglin_safe_armor` (the item tag's contents were not dumped: by its name, golden armour); without it a piglin targets the nearest player it can see (piglin follow range 16, damage 5). `angerNearbyPiglins` reaches 16 around the player, anger 600 ticks. Zombified piglins are neutral until hurt; then they alert others within their follow range and 10 up or down every 4 to 6 seconds, angry 20 to 39 seconds.

## 18b: what was built (commit c876918)

- `Stage::Forest` (`forest`: 7 rods, `World::feet_biome` warped_forest) and `Stage::Pearls6` (`pearls_6`: 7 rods, `PEARLS_HALF` 6 pearls) between `endermen` and `pearls_12`; `pearls_12` needs the 7 rods too. The race names their snapshots `snapshot-forest-…` and `snapshot-pearls_6-…` from `Stage::name` (a test holds every name asset-safe).
- Ledger facts (accretion): `EndermanAngry {id, d, ticking}` (index 17 turning true), `EndermanHit {id, before, after}` (index 9 falling), `EndermanKilled {id, by_bot}` (entity event 3, `by_bot` from `struck`), `PearlDropped {id, pos}` (an ender pearl item within 8 of a death of the last 40 ticks), `PearlPicked {id, amount}` (`take_item_entity` for a pearl item known on the ground), `BlockTaken {cell}` (a variant for the burrow's owner to emit). The bot's kill and a pearl picked are progress (`Gain::Hunt`); `config/race.toml [progress] items` adds `["ender_pearl", 12]`.
- Senses row (`Senses::hunt`): ` pearls N kills K angry T ticking, F frozen swords U`.
- The race summary prints `pearls gained: 0 x5, 12 x1 (6 runs)` once a run reached the forest or gained a pearl (batch rows keep `pearls`).
- Coverage: `entity_event`, `take_item_entity` and `set_entity_data` were already decoded (no `config/observe.toml` change); nothing the hunt needs is thrown away.

## 18a.5: the forest supply (race 45531362383, not done: wall 1)

Two idle bots from the frontier snapshot, put down at `dev-forest` (488.5, 70, -110.5, spread 12), cap 12,000. The `forest` stage held at tick 80 for both, and the race published `dev-snapshot-forest-…` for each lane (the stage name works as an asset name). The bots did not stand under a roof: an idle bot is not given one.

**Wall 1: dev-forest stands on a fortress.** Both bots landed on nether bricks and fences at (493 to 496, 65 to 70, -113 to -120): blazes at 4 to 8 blocks, wither skeletons at 19 to 29. Both burned to death, lane 0 at tick 720 (9 hits: 2 fireballs and the burning) and lane 1 by tick 260. That 12,000-tick count was never made. A warped forest with a fortress in it is no base at that cell: the burrow site needs nether bricks never within 8 (plan 18e), and a start must be placed off the fortress.

**What was seen in the 34 s:**
- 11 distinct endermen within 128 in lane 0, 15 in lane 1, nearly all ticking (one frozen).
- The nearest ticking one was at 25 to 26 blocks.
- Within 32 and ticking: 1 in lane 0, 4 in lane 1.
- None turned angry, though three idle-looking "facing" signs were noted.
- This is the supply the plan expected (more than one enderman per spawn group within 64), but it was not counted per second or per minute.

## Known gaps

- `PearlDropped` needs the item's data (index 8) to say ender pearl; a pearl whose data comes later than 40 ticks after the death is not called a drop.
- `PearlPicked` is told only for a pearl item seen on the ground within 128; the inventory count (`Item` fact) still shows every gain.
- Kills per 6,000 ticks in the race summary is not built (batch rows carry no kill count yet).
