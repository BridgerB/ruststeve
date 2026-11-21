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

### 3. Hurt, back to the spawner, dead (race 44804470273, 8d8c3c3, the lane-1 `spawner` snapshot of 44789031743, 7 x 1)
- **What happened**: 7 of 7 died to `fireball blaze` within 16 to 20 seconds; 6 hits each in the last minute. (The case files said "0 swings", but `race` wrote that field as a literal 0 until 2026-11-21; swings were not counted.)
- **Read** (the case notes): each bot began at 6 or 7 health (the snapshot bot had been hurt on its way in), backed off once ("defend Blaze … Retreat … health 6"), then "walk to the spawner" again, and was shot down.
- **Cause**: nothing sends a hurt bot out of the blazes' sight to heal; the spawner walk resumes as soon as a retreat ends. (The snapshot lineage also has no food and no armour: the geared and fed spawn race is the real one.)
- **Fix**: `plan::cover` (shooters within 16, `seen_at` by exact rays to the eye and the middle, `cover_cells` within 6): under `engage_health` the spawner task takes cover on a found route to the nearest unseen cell, else rests where it is unseen; the eat row also fires to heal (hurt, food under 18). A model test holds a cell behind a wall unseen and one in the open seen.

### 4. A dungeon spawner counted as the blaze spawner (geared spawn race 44804466844, 8d8c3c3, 8 x 14 from spawn, cap 216,000)
- **What happened**: the summary showed 1 of 84 at `spawner` (and so at `iron_kit`, `nether`, `fortress`); its snapshot's `snap.toml` put rs-l01-b02 in the overworld at y -21 holding a wooden pickaxe, at server tick 11,540.
- **Cause**: `at_spawner` counted any spawner block: a dungeon's monster spawner underground. Stages are cumulative, so the lower ones counted too. The real result: **no bot reached `iron_kit`**; gear came first and took the run: iron helmet 30 bots, chestplate 9, leggings 7, sword 4, all four 4 (rs-l01-b01, rs-l01-b07, rs-l03-b03, rs-l05-b07), none further by the cap; lanes 0 and 2 cut at the job's 240 minutes.
- **Fix**: `fortress` and `spawner` count only in the Nether (`world::stage::in_nether`); a `gear` stage between `stone_pickaxe` and `iron_kit` (helmet, chestplate, leggings, sword held or worn), so a geared bot is snapshotted and the chain resumes from it.

### 5. A bucket under the pool, three hours, starving (race 44919152494, 50c573b, the lane-2 `gear` snapshot of 44840801185, 14 x 1, cap 216,000)
- **What happened**: 13 of 13 counted runs stayed at `gear`; 11 capped, 2 died to creepers. 12 of 13 crafted a bucket within the first ~4,800 plan ticks and then never scooped: "walk toward water at" 415 times, "explore to … failed: NoPath", "no scoop routes: 2 sources within 24" 350 times. 11 of 13 ended at health 1 or under (starving, food 0).
- **Read** (three case files, lanes 3, 6 and 11, and `why --at 215000` on lane 3, probe 44942277164): the bot stands at (38, 9, -149) with food 0 and health 0.76; it remembers sources at (52 to 55, 25, -154), 16 blocks over its feet; every ~20 ticks it walks toward them (NoPath), says "no scoop routes … the stands round them unreachable or blind", and explores away (NoProgress). Its sightings put the surface near y 66.
- **Cause**: `stairs_to_bank` passed `above_ok = false` to `dig_toward`, so water over the feet was refused outright, a rule older than `Stairs::up` (wall 1). Nothing else takes a bot underground up to water it has seen.
- **Fix**: `stairs_to_bank` takes stairs up toward a bank over the feet (`dig_toward(…, true, …)`), so the bot cuts up to the pool it remembers; the unit test `water_above_the_feet_takes_stairs_up`.
- **Raced** (ad295ef, race 44942537213, the same snapshot, 14 x 1; cut at 60 minutes because `-f minutes` was left at its default, so no stage counted): **13 of 14 held a water bucket** (0 of 13 before), each after "stairs up toward the bank" 3 times and a swim; then they walked to logs on the surface. The next race runs the full 240 minutes.
- **Also seen, not fixed here**: the bots eat their three beef and then starve for hours; provisions come after the cast's kit. The next race reads whether the water fix gets a bot to the provisions step before food decides it. (`iron_kit` asks for an iron pickaxe the race plan never makes, since the portal is cast; the next stage the plan reaches is `nether`.)

## The fight record (2026-11-21, the porting guide's CombatFrame, summed per fight)
- Case files counted no swings: `race` wrote `swings_sent` and `swings_landed` as a literal 0. They now come from the plan, summed over every fight.
- Each `defend` ends in a note with what it saw: swings, swings landed, the stance, steps held back, the foe's kind, the bot's health at the start and end, the foe's highest point over the eye, and the ticks a blaze foe was charging (`world::Entity::blaze_charged`: its metadata index 16, bit 0, as clojurecraft read it on 26.1.2). The next blaze wall is read from these before any fix.
