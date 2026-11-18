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
