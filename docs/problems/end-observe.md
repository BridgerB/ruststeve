# The End: measure and observe (issue 20a.1 to 20a.3, 20b; agent E1)

The orchestrator's index is [`end.md`](end.md). This file holds what E1 measured and what it built so that the End can be seen before it is played. Everything here is for one world: seed `typecraft`, the race server's. The numbers live in [`config/end.toml`](../../config/end.toml), each with its source.

## 20a.1: the frontier's portal room
Sources: probe 45863427332 (RCON on a server started from the snapshot's world, plus the snapshot's `memory.rsw`) and probe 45864386183 (the snapshot's region files).

- **One frame of 12 holds an eye: (1627, 19, -32), facing north.** The other 11 are empty: (1627..1629, 19, -36) south, (1628..1629, 19, -32) north, (1626, 19, -35..-33) east, (1630, 19, -35..-33) west. The bot holds 13 eyes, 2 more than it needs.
- Memory's states of the 12 frames match RCON's `eye=` exactly: 9469 is an eye facing north, 9473 to 9476 are empty. So `memory::frame_has_eye` decodes from memory alone.
- **The lava pool** is x 1627..1629, z -35..-33 at y 17, open at y 18.
- **Lava at the floor's level** lies only by the door: (1635..1637, 17, -38), (1635..1640, 17, -30), (1638..1639, 17, -29), (1638, 17, -28).
- **The spawner** is at (1632, 19, -34) on a step, with stairs at x 1633..1634. RCON reads it as silverfish, delay 200..800, player range 16.
- The floor is y 16, so the feet stand at y 17. The platform under the ring is y 17 and the cells under the frames (y 18) are air.
- **The stand cells**: an air cell for the feet and the head over a solid floor, outside the ring's 5 by 5, with no lava within 1, and the eye within 4.5 of the frame's top face. The room's box has 215 standable cells, and each empty frame has 25 to 38 of them in reach. The nearest for each:
  - south frames: (fx, 17, -37), 1.56 off;
  - north frames: (fx, 17, -31), 1.56;
  - west frames: (1625, 17, fz), 1.56;
  - east frames: the spawner step's top (1631, 20, fz), 2.07 to 2.30, or the floor's corners (1631, 17, -36) and (1631, 17, -32), 1.85.
- **Not measured: the way back up**, the bot's own stairs to the surface at about (1526, 68, 45). Memory keeps no air cells, so the stairs are not in `memory.rsw`'s report, and the region files cannot tell the bot's tunnel from a cave. The trip up is E2's, read from its recording.

## 20a.2: the End of this seed
Source: probe 45863427332, the snapshot's world, with the End forceloaded over x -64..127, z -64..63 and its region files read. This is development knowledge: Bridger's ruling allows the geography, and no bot code reads RCON.

- **The platform**: obsidian 5 by 5 at y 48, x 98..102, z -2..2. The bot lands at (100.5, 49, 0.5) facing west (from the jar's EndPortalBlock).
- **The island's edge along -x** (looking at z -1..3):
  - The end stone ends at x 72 (at z -2 it ends at 66, at z -3 at 64).
  - x 68..72 is a lip one block thick at y 52.
  - x 65 holds end stone from y 50 to 54, and x 63..64 from y 48.
  - The island is solid from x 62 inward, with its top at 56.
- **The gap** is x 73..97: 25 cells of void.
- **The lip's top is 4 above the platform's.** A bridge at the platform's level meets the lip's underside at x 65 (end stone at y 50, where the head is) and then the island's side at x 63..64.

**The spikes.** Every crystal is at the bedrock top + 1, at the centre + 0.5, with fire in its cell. The obsidian runs from y 0 to the top - 1. Ground is the median end stone top 1 to 3 out from the spike.

| centre | bedrock top | radius | caged | ground |
|---|---|---|---|---|
| (12, 39) | 76 | 2 | | 61 |
| (-34, 24) | 79 | 2 | **yes** | 62 |
| (42, 0) | 82 | 2 | **yes**, beside the bridge's line | 58 |
| (-34, -25) | 85 | 3 | | 62 |
| (-42, -1) | 88 | 3 | | 62 |
| (33, -25) | 91 | 3 | | 57 |
| (33, 24) | 94 | 4 | | 59 |
| (-13, -40) | 97 | 4 | | 61 |
| (-13, 39) | 100 | 4 | | 62 |
| (12, -40) | 103 | 5 | | 57 |

- **The cages**: iron bars over the centre ±2 on x and z, from the top's y to the top + 3, 73 bars each.
- **The fountain was not generated**: no player has been in the End, so there is no dragon fight yet. From the jar's EndPodiumFeature placed at the heightmap of (0, 0), where the end stone top is 62:
  - the origin is y 63;
  - the bowl's floor is bedrock at y 62;
  - the portal is at y 63 within 2.5 of the centre, and the rim's bedrock at y 63 out to 3.5;
  - the pillar is bedrock from y 63 to 66.
  - **The pillar's top (66) and the perch height (about 67) are derived, not observed.** E6's parity or a dev start should confirm them.
- **The island**: end stone tops within 30 of (0, 0) run 57 to 64, median 63. The radius is about 64 on the -x, +z and -z axes and 72 on +x.

## 20a.3: the jar
Source: probe 45863431767, `javap -c -p -constants` of 26.1.2 and its data tags.

- **`EndCrystal.hurtServer`**: any damage not from the dragon removes the crystal, then an explosion of power 6 goes off at its position, unless the damage was itself an explosion. So a crystal set off by another crystal's blast does not explode.
- **`handleAttack`** (26.1 has a separate `ServerboundAttackPacket`): the only test is `isWithinAttackRange(main hand, the target's box, buffer 3.0)`. **There is no line-of-sight test.** The server accepts a strike through blocks, which is the premise of the pillar-and-wall strike.
- **The explosion's damage**: `(impact² + impact) / 2 * 7 * 12 + 1`, where `impact = (1 - feet distance / 12) * seen`.
  - `seen` is the share of rays left open, from a grid over the body's box (about 45 points) to the centre, under `ClipContext.COLLIDER`.
  - Full blocks shut a ray; iron bars' thin shape leaves gaps.
  - Fully seen at 3 off, the damage is about 56, so the wall must shut every ray.
- **The dragon's damage taken** (`EnderDragon.hurt(part)`):
  - the head takes it all; every other part takes `damage / 4 + min(damage, 1)`;
  - anything under 0.01 is nothing;
  - only a player's damage or `#is_explosion` hurts it;
  - the sitting phases set arrows and wind charges on fire and take 0 from them.
- **A perch ends** when `sittingDamageReceived` passes 0.25 × 200 = 50; then the dragon goes to TAKEOFF.
- **`checkCrystals`**: the dragon gains 1 health every 10 ticks (2 a second) from the nearest crystal, searching within 32.
- **`checkWalls`**: the head, neck and body break every block that is not air, not `#dragon_transparent` and not `#dragon_immune` (with mob griefing on).
  - `#dragon_immune` holds end stone, obsidian, iron bars, bedrock, crying obsidian, the frames and the portals.
  - Cobblestone and netherrack are not in it.
- **The phases**: 0 holding, 1 strafe, 2 landing approach, 3 landing, 4 takeoff, 5 sitting flaming, 6 sitting scanning, 7 sitting attacking, 8 charging, 9 dying, 10 hover.
  - When a holding path is done, the dragon lands if `nextInt(crystals + 3) == 0` and strafes if `nextInt(crystals + 2) == 0`.
- **The parts**, for a perched dragon not turning, from its position and yaw t:
  - head at (6.5 sin t, -1, -6.5 cos t), box 1 by 1;
  - neck at 5.5 the same way, box 3 by 3;
  - body at (0.5 sin t, 0, -0.5 cos t), box 5 by 3;
  - wings 4.5 across and 2 up, box 4 by 2; tails 2 by 2.
  - The parts' ids are the dragon's id + 1 to + 8, in that order.
- **The head's and neck's blow**: 10.0 (`mobAttack`) to everything in their boxes inflated by 1.0. Through iron armour (`CombatRules`, toughness 0):
  - 6.8 with no boots (armour 13): clojurecraft's number;
  - **6.0 with iron boots (armour 15)**;
  - so danger 8 plus two blows is 20.0, exactly full health.
- **The wings** (only while the dragon's hurtTime is 0): their boxes are inflated by (4, 2, 4) and moved down 2. Anything inside is pushed 4 out and 0.2 up, and takes 5.0 when the dragon is not sitting.
- **The death**: the killing blow goes through `LivingEntity.die` (entity event 3). Then 200 ticks of `tickDeath`, then `setDragonKilled` (the boss bar is hidden) and the removal.
- **The clouds**:
  - a `DragonFireball` leaves a cloud of radius 3.0 that grows to 7.0 over 600 ticks, with instant damage;
  - the sitting-flaming breath is a cloud of radius 5.0 for 200 ticks.
- **The enderman's stare**: `isLookingAtMe(player, 0.025, scaled by distance, eye y)`, true when the dot product exceeds 1 - 0.025 / distance with a line of sight. A worn carved pumpkin excuses it.
- **The End's portals**:
  - Entering an end portal in the overworld leads to the End's spawn point and its platform.
  - Entering one in the End calls `ServerPlayer.showEndCredits`: the player is removed from the End and `WIN_GAME` is sent.
  - The client's `ClientCommand PERFORM_RESPAWN`, with `wonGame` set, respawns it in the overworld (`PlayerList.respawn`).
- **Water**: each tick in water resets the fall distance, and `checkFallDamage` skips a body in water. So water in the landing cell cancels the fall.

## 20b: what was built

**Stages** (`crates/world/src/stage.rs`; `Stage::ALL` is 24):
- `armed` comes before `end`; `island`, `crystals` and `win` come after it.
- `armed`: in the overworld, holding an iron sword, 2 pickaxes (stone or better) with 260 uses in all, 24 cooked food, eyes at least the empty frames (12 less those on record with an eye), 128 blocks and a water bucket (`config/end.toml [kit]`).
- `island`: the feet on end stone within 30 of the fountain.
- `crystals`: the feet within 60 of the fountain, every spike's column loaded and no end crystal alive. Within 60, every spike is within 102, inside the tracking distance of 112, so any crystal still alive has been sent.
- `win`: `w.won` with `w.dragon_dead`.
- The constants equal `config/end.toml`, held so by a test.

**World** (fenced): `dragon_dead` (the tick of the dragon's entity event 3, or its boss bar removed in the End), `attacks_sent` and `last_attack`.

**Memory**:
- watches end stone, iron bars and bedrock; iron bars and bedrock are indexed;
- `memory::frame_has_eye(state)`.

**Facts** (`observe`, accreted): `EyePlaced`, `PortalEntered`, `CrystalSeen`, `CrystalGone {how}` (struck, blast, far, gone), `PillarUp`/`PillarDown`, `Strike {target, reach}`, `DragonPhase`, `DragonHit {part, health}`, `DragonBlow`, `Breath`, `GazeNearMiss {stare}`, `DragonDead`, `Credits`.

**Senses** (in the End only): ` dragon 180.0 p0 crystals 7 fountain 34.2 over 12 endermen16 1`.

**Progress**: `Gain::End`; `[progress] items` gains `["end_stone", 160]`.

**Race summary**: an `end <bot>: …` line and a `dragon health <bot>: …` trail (a point a minute) for every run that had an End.

## Walls
(None yet.)
