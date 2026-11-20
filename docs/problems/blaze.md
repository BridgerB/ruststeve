# Issue 16: blaze rods (one bot first)

## Needs
From the frontier fortress snapshot, one real bot holds 7 blaze rods (`rods_7`), its snapshot published and made the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`: a death ends the attempt (Bridger, 2026-11-20: the compute stays in the Nether).

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: rods_7`, and the race's release carries its `snapshot-rods_7-…` asset.

## Risks
Blazes hover out of reach; burning (45% of clojurecraft's lost health; no water in the Nether); volleys in the open (9 of 20 of clojurecraft's deaths); the fortress maze between the gate and the spawner; tick-paced swings bunching on a loaded server; no food to heal with.

## 16a: food first, then a fresh frontier
- **Why**: the frontier fortress snapshot (race 44678197603) came from a bot that started beside a built portal with no food; healing between blaze fights needs food over 18. Bridger (2026-11-20): the bot gathers enough food before the Nether, and a fresh snapshot is taken once it is ready.
- **Built**: `food_reserve` 32 hunger points in the survival rules; `eat::food_points`; the `hunt` row also fires while the `race`/`nether` aim is short of the reserve in the overworld; with a portal to enter and the reserve short, the bot explores for animals before stepping in ("provisions N of 32"); `snap.toml` lists the held items.

## 16b to 16d: the first cut
- **The `spawner` stage** (`world::stage::Stage::Spawner`, `at_spawner`): the feet within `SPAWNER_NEAR` 8 of a spawner seen exposed, between `fortress` and `rods_7`; the race snapshots it like any stage.
- **`spawner_task`** (the `race` aim once the fortress is reached): 7 rods held is the aim met; a rod lying within 8 on a found route is picked up first; at the spawner the bot holds its ground; a spawner seen is walked to on a found route, else tunnelled toward; with none, the nearest brick not within 8 of the trail, else explore.
- **Blazes as shooters**: `blaze` joins the `ranged` list, so `defend` answers one within 16, closes in and strikes; a shooter beyond `defend_radius` is engaged only at `engage_health` 18 or more.

## Walls

### 1. Under the spawner, no way up (race 44781414716, 44886cb, frontier fortress snapshot, 7 lanes x 1, cap 24,000)
- **What happened**: 0 of 7 reached the spawner; 5 of 7 died (fire 2, `onFire.player blaze`, `fireball blaze`, `arrow piglin`). Every bot saw the spawner at (129, 64, 159) from its first ticks ("tunnel toward the spawner at (129, 64, 159)" 45 times), then "stairs failed: StepFailed" 29 times, explored off and came back.
- **Read** (the case files' last positions): the bots ended at (130.5, 43, 159.7), (133.5, 43, 159.8), (140.7, 43, 156.4): right under the spawner, 21 blocks below it. `dig_toward` cut a level tunnel to a target above the feet, and stairs went only level or down.
- **Fix**: `Stairs::up` (`stairs::up_step`): a step up along the heading opens the room over the head, then the next column's head and feet one up, its floor the block ahead at the feet's height; the same refusals (no fluid, no lava beside, nothing falling over the top). `dig_toward` takes stairs up for a target more than a block over the feet. A unit test holds the step's cells with the race's numbers.
- **Raced** (2443cf3, race 44789031743, frontier fortress snapshot, 7 x 1): **3 of 7 reached the spawner** ("stairs up toward the spawner" 21 times; `spawner` snapshots from lanes 1, 2 and 4). All 7 died: wither skeletons 5 (`mob wither_skeleton` 3, `wither.player wither_skeleton` 2), blazes 2.

### 2. Bare bots against wither skeletons (the same race: 5 of 7 dead)
- **Read**: "defend WitherSkeleton" 8 times in 7 runs; the bots wear nothing and fight with a stone sword or the iron pickaxe; a wither skeleton hits 8 on Normal and withers.
- **Fix** (Bridger, 2026-11-20: both): wither skeletons join `never_fight` (backed away from along a route, as creepers are); and a `gear` row before the Nether: an iron helmet, chestplate and leggings and an iron sword (22 raw iron, 6 coal wanted), the race aim working it before the portal row, the armour put on by a quick move (`Plan::equip`). The fresh frontier is a fortress snapshot of a bot that is geared and fed.
