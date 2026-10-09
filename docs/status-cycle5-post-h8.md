# ruststeve cycle 5, post-handoff status (2026-10-05 02:40 MDT, handoff + 7 h)

Every number below is from gym.db and the box gym logs. Intervals are Wilson 95%.

## The race water wall: first real movement

water_wall_i8 starts a bot in the lake that held i8's rust-race-002 all race; it passes at 16 cobblestone within 600 s.

| build | change | result |
|---|---|---|
| baseline | (tree) | 0/10 |
| 56f7e6b | escape budget 40 s | 0/2 |
| b395ecb | ignore "land" floating on water (lily pads) | 0/10 |
| 13ef075 | break a lily pad in the way | 2/10 (the clear never fired) |
| swim-first | mine_stone swims out before searching | 0/10 |
| 8f82c54 | commit to one shore column | 0/10 (now reaches the bank, cannot climb out) |
| 636e8ba | SDK physics: out-of-water ledge lift tested free space at the world origin | 1/10 |
| **07b588d** | mine_stone only refuses when actually swimming | **4/10 (0.17–0.69)**, P(> baseline) = 0.98 |

- Arena water set on 07b588d: **14/14** (0.78–1.00). 6b-head scored 13/14 and 6b-safe 12/14.
- The physics bug is in the SDK port (`does_not_collide` called with a velocity offset instead of a position). It probably affects every water exit in this tree.

## The tree still trails 6b-head on the portal

| comparison | tree | 6b-head |
|---|---|---|
| regions 79/80 (separate), tree with search-time A* | 7/18 | 10/17 |
| region 69 | 4/7 | 6/10 |
| pooled | 11/25 | 16/27 |

P(tree > head) = 0.125 on 79/80.

- With SAFE_FIXES off, portal.rs is the same as 6b-head's (gated code and instrumentation only).
- The tree's timeouts end on long siting trips: tunnelling toward lava 46–99 blocks away, or heading to remembered pools.
- Test now running: `ASTAR_SYNC=1` restores the one-shot A* (6b-head's). 6b-head (region 84) vs tree + sync (region 85), 6 trials per bot.

## Race-build trade-off for cycle 6 (strategist)

| | tree (target-wade + A*) | 6b-head |
|---|---|---|
| water wall | 4/10 | 0/10 (6b-head lacks the slug; its race lost 1–3 bots to lakes per race) |
| water set | 14/14 | 13/14 |
| iron_from_surface | 5/10 (early iron) | 1/10 (tree without early iron) |
| portal | ~11/25 | ~16/27 |
| event rows / bandits | yes | no |

## Box

Server B only. Running now:
- 6b-head portal arm (bots 001–003, region 84);
- tree-sync arm (bots 004–006, region 85).

race-b.sh has both race fixes (respawn check, frame anchors cleared per race).
