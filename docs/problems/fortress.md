# Issue 15: Nether entry and the fortress

## Needs
From the checkpoint start `nether` (the Nether at (0, 0), where a portal near spawn leads; the post-cast kit, 64 cobblestone, food; each bot spread 48 under y 100), reach a fortress, keeping the building blocks to get anywhere.

## Done (world terms)
30% of real bots end within `FORTRESS_NEAR` (6 blocks) of a nether-brick cell they saw with an exposed face, and the median bot holds 32 or more building blocks. Pooled pairs of 8-lane races.

## Risks
- The Nether's lava seas and open drops; tunnels must never open a cell beside lava.
- Ghasts, piglins and zombified piglins; a struck zombified piglin angers its group.
- Building blocks spent on bridges and never restocked (clojurecraft's stage 18 ran dry in 40 minutes).
- The nearest fortress on the race seed is 182 blocks from (0, 0) (probe 44386934210, setup data only).

## Walls

### 1. "Reached" meant "seen" (baseline race 44395241839, 37afb56: 97 of 112 met, 112 of 112 at the stage)
- **What happened**: the `fortress` stage held for all 112 bots, the aim met by 97 (every bot alive at the cap), most with no exploring at all.
- **Cause**: the stage and the aim held once nether bricks were seen with an exposed face; a fortress's open bridges are in view from 180 blocks.
- **Fix**: the stage and the aim hold when the feet are within `FORTRESS_NEAR` 6 of a nether-brick cell on record (`world::stage::at_fortress`); the aim walks toward the nearest brick seen (`bricks_near`, within 16 chunk columns), refusing each once tried, else explores.

### 2. Defending against zombified piglins (the same race: 15 of 112 died, 11 to piglins)
- **What happened**: deaths to `mob zombified_piglin` 7, `spear zombified_piglin` 2, `mob piglin` 2; notes "defend ZombifiedPiglin" 14 times in 10 runs.
- **Cause**: zombified piglins are in the game's `monster` category, so `defend::hostile` answers them on sight; a struck one angers its whole group.
- **Fix**: `neutral` kinds in the survival rules (`zombified_piglin`, `enderman`) are fought only once one of their kind has hit the bot within `PROVOKED_TICKS` 400 (`defend::provoked`). Piglins stay foes: with no gold worn they attack.
- **Raced** (walls 1 and 2, f810efa, pair 44405101661 and 44405107364): **0 of 224** within 6 of bricks; deaths 57 and 56 (against 15 standing still): piglins 32 (arrow, melee, spear), fire 22 (`inFire`), ghasts 21 (fireball, explosion, fire), hoglins 12, skeletons 6, endermen 4, lava 3, falls 5. Zombified piglins 1 (against 9).

### 3. Walking to a fortress across the open (the f810efa pair: 0 of 224, half dead)
- **Read** (race 44405101661's notes): "walk toward the fortress at …" 1,115 times in 104 runs, "explore to … failed" 1,529, "relocate" 540; no bot came within 6. The nearest fortress is about 180 blocks from the start across lava seas and open drops; a walk took a partial route, ended in the open short of it, and the next one did the same.
- **Cause**: the aim walks on partial routes; the Nether's open ground between is where ghasts, piglins and fire killed half the bots.
- **Fix**: walk to bricks only on a found route; else tunnel toward the nearest brick (stairs down when it is below, a level tunnel else; `dig_toward`, shared with wall 40's stairs to a water bank), three without coming nearer refusing it. The stairs' refusals hold (no fluid in a cell opened, no lava beside one).
- **Not yet**: piglins attack a bot wearing no gold (32 deaths); gold armour is not in any earlier stage's kit.
- **Raced** (ca34f7c, pair 44413659356 and 44413666090, cap 24,000): **65 and 46 of 112, 111 of 224 reached a fortress (49.6%)**; median building blocks held 213 over all 224 bots (268 over those that reached it, none under 32: tunnels restock netherrack); deaths 20 and 37 against 57 and 56. Met bots ended at (25, 55, 130), (55, 57, 92) and the like, in the fortress round (48, ~, 176). The done-condition holds.

## Not done in 15a (carried on)
- Entering the bot's own portal from the overworld (15c): the `nether` stage is met here by the start's teleport.
- Resources per unit of progress (digs per block, blocks placed against kept) are not reported per run; the building-block median is computed from the case files' items gained and lost.
- Piglins (no gold worn), ghasts and hoglins still kill about a quarter of the bots.

## 15c: into the Nether through the bot's own portal (reopened 2026-11-18)
- **Needs**: the issue's first scope line, never built: from a lit portal, walk to it without line of sight and step in; PR #60 met the fortress done-condition from the `nether` start, where bots are teleported in, so a run from spawn stopped at a lit portal.
- **Done (world terms)**: from the `portal` checkpoint (a lit portal built beside each bot), bots arrive in the Nether; then a race from `spawn` with the `nether` aim says how far the whole chain gets.
- **Built**: memory watches `nether_portal` blocks; `Aim::Nether` walks into the nearest portal seen (or the one this bot lit) on a found route and stands in it until the dimension changes (`Task::Enter`, 400 ticks at most), else casts one with the `portal` row; the `portal` start (`Start.setup`, commands run after placing the bot).
- **Raced** (2820a63, `portal` start, aim `nether`, race 44656360133, 8 lanes x 14, cap 6,000): **94 of 112 arrived in the Nether (83.9%)**, each "enter the portal at …" then "through the portal"; no deaths. The 18 others logged no entry: no found route to the portal built round them (setup in rough ground), so the aim fell back to casting one.
- **The marathon** (d701b19): one runner on the latest main plays the `race` aim (cast, enter, fortress) from spawn through deaths (`--revive`, `keep_inventory`), leg after leg; its first legs give the spawn-to-Nether number.

## One bot first (2026-11-19)
- Spawn race 44662871756 (d0e92fc, 7 lanes x 14, aim nether, cap 144,000): **2 of 98 bots went from spawn into the Nether** in one run (table 85, stone pickaxe 45, iron kit 2, nether 2).
- Snapshots: race 44678197603 (`portal` start, aim `race`) captured `nether` and `fortress` snapshots in all three lanes. Resumed from its lane-0 `nether` snapshot (race 44687236976), both bots that ran started in the Nether with the snapshot bot's items and reached the fortress. The frontier is its lane-0 `fortress` snapshot.
