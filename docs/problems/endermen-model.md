# Problem: endermen in the model (issue 17f)

**Status: done for issue 17** (agent E4): the model, its tests and four parity rows that match a real server. The index of issue 17's walls is [`endermen.md`](endermen.md).

## What the bot needs
Issue 17 is done when one real bot comes within 32 blocks of an enderman that ticks (its chunk within the server's simulation distance of the player's). The heading (E2), the stage and the feet biome (E1), and the stare filter (E3) each need a closed-loop test that says so in minutes, not a race: a Nether test world whose columns carry the Nether's biomes, and endermen that stand, wander, freeze, anger and teleport as the 26.1.2 server's do.

## Risks
- **A kinder model than the game** (clojurecraft's largest sim/real gap was ticking mobs to 6 chunks where the server ticked 3: frozen endermen looked alive and alive ones frozen). Every rule here is read from the 26.1.2 jar with `javap`, and the freeze reads the same simulation distance (`config/race.toml`) the race servers use.
- **Sim numbers are development evidence only** ([measurement-rules](../../brain/ruststeve/decisions/measurement-rules.md)): a green model test is never issue 17's evidence; a race is.
- The parity rows need a non-peaceful real server (an enderman is a monster: peaceful removes it at once), and the parity server runs peaceful. The enderman rows run apart, at Normal.

## Done, in world terms
- A model test world in the Nether can lay a warped forest, a crimson forest, a soul sand valley; the bot's world, fed the model's columns, reads that biome at its feet.
- An enderman placed in the model: frozen beyond the simulation distance, ticking within it; a stare angers it after the goal's delay; teleport rules as the jar's.
- Parity rows that compare the model with a real server: an enderman beyond 3 chunks frozen, one at 30 blocks ticking, a stare angers.

## The read (javap of the 26.1.2 server jar)
Probe 45472254333 (`javap -c -p -constants` of `EnderMan` and its four goals, `LivingEntity`, `Mob`, `Entity`, `Monster`, `NeutralMob`, `PathfinderMob`, `NaturalSpawner` and `$SpawnState`, `SpawnPlacements`, `DistanceManager`, `ServerLevel`, `ChunkMap`; `warped_forest.json` and `crimson_forest.json`):
- **Ticking.** `DistanceManager.getPlayerTicketLevel` = `max(0, ChunkLevel.byStatus(ENTITY_TICKING) - simulationDistance)`; `inEntityTickingRange` = the simulation tracker's level is entity ticking. `ServerLevel.tick`'s entity loop runs `checkDespawn`, then returns for any non-player entity out of that range; `ChunkMap.tick` sends an entity's changes only in that range. With ENTITY_TICKING at 31 and the tracker adding one a chunk, the player's chunk is 28 at distance 3, and a chunk ticks entities at a Chebyshev distance of 3 or less. (`ChunkLevel` and the tracker's neighbour rule were not in this probe's classes: the square is the chunk trackers' known rule, and E1 reads the same.)
- **The stare.** `isBeingStaredBy(player)` = `PLAYER_NOT_WEARING_DISGUISE_ITEM` (the head slot not in `#gaze_disguise_equipment`) and `isLookingAtMe(player, 0.025, true, false, [getEyeY()])`: the player's view vector against the unit vector from the player's eye to the enderman's eye, `dot > 1 - 0.025 / length`, and `player.hasLineOfSight(enderman, COLLIDER, NONE, eyeY)` (a clip eye to eye that misses every collision shape, within 128). The eye alone; no body or leg height.
- **The look goal** (`EndermanLookForPlayerGoal`, target selector 1): `canUse` is overridden without the random interval: the nearest player within the follow range (64) who stares. `start`: `aggroTime = adjustedTickDelay(5)` (3 for a goal not ticked every tick), `teleportTime = 0`, `setBeingStaredAt` (data 18). `canContinueToUse` while pending: the stare holds. `tick`: `--aggroTime <= 0` makes the pending player the target (`setTarget`: data 17 true, speed +0.15). With a target: stared and `distanceToSqr < 16` → `teleport()`, `teleportTime = 0`; else `distanceToSqr > 256` and `teleportTime++ >= adjustedTickDelay(30)` (15) → `teleportTowards`. `EndermanFreezeWhenLookedAt`: a player target within 16 who stares stops its navigation.
- **Teleports.** `teleport()`: x and z `± 32` (`(rand - 0.5) × 64`), y `nextInt(64) - 32`. `teleportTowards(e)`: `v` = unit (this - e, from `getY(0.5)` to its eye); x `+ (rand - 0.5) × 8 - v.x × 16`, y `+ nextInt(16) - 8 - v.y × 16`, z likewise. `teleport(x, y, z)`: down while the block does not block motion; refused on water; `randomTeleport(x, y, z, true)`. `hurtServer`: a source whose entity is not a `LivingEntity` teleports it; a living attacker's never does.
- **Numbers.** `createAttributes`: health 40, speed 0.3, attack 7, follow 64, step 1. Goals: `FloatGoal`, the freeze, `MeleeAttackGoal(1.0)`, `WaterAvoidingRandomStrollGoal(1.0, 0)`, look goals, leave and take block; targets: the look goal, `HurtByTargetGoal`, endermites, `ResetUniversalAngerTargetGoal`. Persistent anger 20 to 39 s; daylight teleport only when bright outside.
- **Spawning.** `NaturalSpawner`: `MIN_SPAWN_DISTANCE` 24, `SPAWN_DISTANCE_BLOCK` 128; `spawnCategoryForPosition`: no spawn inside a redstone conductor; 3 packs; `ceil(nextFloat × 4)` tries; a step of `nextInt(6) - nextInt(6)` on x and z; the nearest player more than 24 away; a group count from `minCount` to `maxCount`; the spawn cost (`SpawnState.canSpawn`: `PotentialCalculator.getPotentialEnergyChange(pos, charge) <= energyBudget`); a pack stops at `getMaxSpawnClusterSize`. `warped_forest.json`: monsters, enderman alone, weight 1, 4 to 4; `spawn_costs` enderman charge 1.0, budget 0.12. (`PotentialCalculator`'s sum of charge over distance is the known rule, not read here.)

## Walls

## What exists (bb982b2 and on)
- `crates/sim/src/enderman.rs`: `Brain::Enderman`, `Sim::summon_enderman`, `Sim::ender(id)`, `Sim::stares_at(feet)`, the tick (anger, teleports, chase, stroll, fall, blow), natural spawning in warped forests and soul sand valleys (`Life::ender_spawns` counts them; a valley's packs are drawn skeleton 20, ghast 50, enderman 1, and only the endermen are spawned; nether wastes draw an enderman 1 pack in 168). Natural spawning is opt-in, `Sim::with_natural_spawns`: the closed-loop worlds of issue 16 are all nether wastes and keep their mobs.
- `Sim::ticking(pos)`: Chebyshev chunk distance to the player's chunk within `Config::simulation_distance`. Every mob brain is frozen beyond it (walkers, blazes, knocks, falls), not only endermen.
- `sim::nether::Biome`, `Stage::biome(x, z, biome)`, `biome_id`, `biome_index`; `ModelWorld::biome_at`. A stage's default biome is now `nether_wastes` (it was index 0, `badlands`).
- Tests: `crates/sim/tests/endermen.rs` (biomes decode, frozen and ticking, starts ticking as the player comes, the stare's delay and the look aside, the pumpkin, teleports toward and away, a hit never teleports, natural spawning, none on nether wastes or peaceful) and the closed loop `crates/bot/tests/endermen_model.rs` (the bot reads the warped forest at its feet; a frozen enderman starts ticking as the bot walks into range; the bot comes within 32 of it ticking; a staring look angers it and the bot sees it creepy; a look 6 degrees under its eye does not).

## The parity rows
`config/moments.toml` rows `e00-stare`, `e01-frozen`, `e02-ticks`, `e03-aside` (the parity-table note's section). A row's `difficulty = "normal"` runs it after the peaceful rows with the real server set to Normal by RCON; `mobs` places mobs with their AI; the class gains `mobs` (seen, moved, creepy).

### The first parity run (parity 45483333265, 764d798)
**164 of 164** moments match in every field, the four new rows included: `e01-frozen` (an enderman 64 east, 4 chunks out) stays where it was put in real and in the model; `e02-ticks` (30 east) falls in both; `e00-stare` turns creepy in both and stays penned; `e03-aside` stays neutral in both. The real server ran them at Normal by RCON after the peaceful rows.

### The whole suite before and after (probe 45477950396)
Release, every test with `--include-ignored`: main 5657899 451 passed, 8 failed; 764d798 466 passed (the 15 new), the same 8 failed (issue 16's ignored fight and rods walls). Every `SEED`, `SEEDS` and `MEASURES` line is byte-identical: freezing every mob beyond the simulation distance changed no issue-16 arena, all of which sit within 3 chunks.
