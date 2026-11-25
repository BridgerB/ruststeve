# Endermen: measure and observe (issue 17a and 17b, agent E1)

The orchestrator's index is `docs/problems/endermen.md`. This file holds what E1 measured and built so the stage can be seen before any behaviour changes (observe-first).

## 17a: what was measured

All in one world (seed `typecraft`); probes 45472883583 (snapshots, biome JSONs, `locate biome`, strip map) and 45472888528 (enderman census), jar classes from E4's probe 45472254333.

- **The three real `rods_7` snapshots** (player `.dat`, `results player-dat`): every one holds 2 stone pickaxes, one at damage 118 (13 uses left) and one fresh (131): 144 uses, not 262. Lane 0 of 45358767468: health 17.1, food 17, saturation 0, on fire, armour damage 29, a raw porkchop. Lane 1 of 45363542882: health 19.0, food 19, armour 20, no food. Lane 2 of 45363542882: health 16.2, food 20, saturation 0.8, armour 17, a raw porkchop, a shield at damage 288 of 336.
- **Geography** (development knowledge only; no code reads it): from the spawner (129, 64, 159) the nearest warped forest is at (481, 64, -129), 454 blocks; a soul sand valley 32 blocks west, basalt deltas 45 north-east, crimson forest 143 south-south-west; the spawner stands in nether wastes. Along the +x axis the only warped forest within 600 is 352 to 384 blocks out (48 wide) behind 300 blocks of basalt deltas.
- **Spawn lists** (26.1.2 biome JSONs, monster weight): warped forest enderman 1 of 1 (group 4); soul sand valley enderman 1 of 71; nether wastes 1 of 168; crimson forest and basalt deltas none.
- **Spawning** (NaturalSpawner, DistanceManager, ChunkMap): the mob cap is 70 × 289 / 289 = 70 for a lone player (the spawn-chunk counter is Chebyshev 8 whatever the simulation distance), so the cap does not bind; spawns happen only in ticking chunks (7 × 7 at distance 3), at least 24 blocks from the player and in chunks within 128 of one.
- **Despawn** (Mob.checkDespawn): beyond 128 at once; beyond 32 after 600 idle ticks at 1 in 800 a tick; only for ticking mobs, so a frozen enderman stays.
- **Our evidence** (617 runs' senses, `results endermen`): an enderman within 32 in 0.39% of Nether seconds (268 of 68,888, 10 runs), 1.75% of overworld cave seconds (y < 50), 0.15% of overworld surface seconds. All Nether time so far is portal, fortress and spawner. No row so far carries the day clock.
- **The leave** (ledgers): all three rod bots walked 45 to 47 blocks south by the route to about (130, 61, 206) and ended `rods met`; lane 0 burned to 9.2 health on the way; none saw an enderman after rods.

## 17b: what was built

- `Stage::Endermen` (`endermen`, between `rods_7` and `pearls_12`): 7 rods held and the feet within `ENDERMAN_NEAR` (32) of an enderman that ticks (`stage::enderman_near`). At simulation distance 3 anything within 32 ticks.
- `world::ticks(w, pos)`: Chebyshev chunk distance to the body's chunk within the simulation distance.
- `World::feet_biome()`: from the `registry_data` biome list (now decoded, `S2c::RegistryData`) or the generated table; `biome_at` private. Tested on clojurecraft's real Nether columns.
- Memory: biome signs per column (`record_sign`, `biome_signs_in_column`), `forest_known`, `soul_valley_known`, `seen_columns`.
- Ledger facts `Biome`, `Enderman`, `EndermanStill`, `ForestSeen`, `SoulValleySeen`; a new feet biome or a known forest or valley is progress. The senses row adds the feet biome, the nearest ticking and frozen enderman, pickaxe uses and filler.
- Coverage: `configuration/registry_data` is decoded now, so it left the skipped list; entity metadata, `set_simulation_distance` and `chunks_biomes` were already decoded.

## Known gaps

- Soul soil the bot places (it carries 50) counts as a soul sign, and a fortress's nether wart rooms stand on soul sand.
- Sign faces on a column's edge towards a column not yet loaded are not re-checked when it loads.
- `chunks_biomes` updates are counted, not applied (only `/fillbiome` sends them).
- The heading's projection field in the senses row is left to E2.
