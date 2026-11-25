# Issue 18e and 18f: the burrow and the hunt

Agent P3's wall file (the index is [`pearls.md`](pearls.md)). The forest base (18d) moved to agent P5.

## Needs
A bot in a warped forest (health about 12.5 that it cannot heal, an iron sword with 161 uses, filler by the hundred) gets 12 ender pearls, about 24 to 31 kills, without being hit once.

## Done (world terms)
- Ledgers show "burrow built at …", "provoke enderman N", "provoked enderman N", "fight enderman N from B", "enderman N killed" (with hits and resources per pearl) and "pearl picked (k held)" up to 12, and "pearls met".
- "breach: hurt by an enderman" 0 for a bot in B; hits per kill about 7 (iron) or 8 (stone); angers per provoke and the step-out count read from the notes.

## Risks
- The geometry is the jar's, not yet a real server's (P4's parity rows e04 to e08 first).
- Parked endermen (clojurecraft's main stall): the parked look and the fight cap are the answers.
- The bot cannot heal: a breach is permanent damage.
- One world: every race from the frontier is one forest.

## What is built (branch `issue-18-p3-hunt`)
- `plan::burrow` (pure): the site by cell, nearest the feet within 24 (B, M, O level; O and its outward neighbours three clear; four walkable beyond O; no lava within 4, no drop over 2 within 3, no bricks within 8; walls only non-holdable blocks or room for one), the shell (38 blocks at most: the tunnel in a solid 4 by 5), the missing blocks.
- `plan::pearls`: the nine-row table (`pearls_step` after the always-rows, `pearls_task` for the walks and digs) with its invariants; `[pearls]` in `config/goals.toml`.
- `look::keep_off_except` / `stares_except` (the provoke target only), `count_near_misses` skips it; `ender_threat` yields to a bot in its built burrow; `keep_look`'s eyes-down not in the hunt; `defend::box_distance_of` (2.9-high box); `cover::fill_block`.
- Tests: `crates/plan/tests/pearls.rs`.

## Walls

### 1. The first dev-forest race (race 45534591345, aaa10f9, dev-forest, 4 x 1, cap 12,000; never evidence)
- **What happened**: 3 of 4 bots built a burrow (11 to 12 blocks, the first shape) within 300 ticks of the hunt; one kill (lane 2: provoked from M at 30.4, 7 iron hits from B, health 12.5 throughout, no pearl dropped). Lane 1 provoked one at 6.1 that came close and stayed out of reach 500 ticks (the fight given up, sealed). Lane 0 stood on a fungus cap at y 56, found no site within 4 up or down, walked away blind into the basalt deltas (71 bridge blocks) and died to a magma cube.
- **Read**: each bot waited 3,300 to 4,000 ticks with endermen 15 to 30 off out of the mouth's one-way sight. Lane 1's enderman stood beside B outside the side wall: P4's real parity run 45535279839 showed the cause (O, the cells beside B and behind its back wall all lie 2 from B, a tie for a 2.9-high mob's partial path).
- **Fixes**: the shell a solid 4 by 5 with the tunnel in it (O the one cell at 2); the mouth turned toward the endermen seen lately; a provoke from O at one 12 or more off when M sees none; burrow_idle_ticks 2,400; the walk on is to warped nylium seen, the site search 8 up and down; the sword held from the fight's start (the first swing after the switch was weak); the bot pressed back in B (P4's e04 drift).

### 2. The second dev-forest race (race 45537373222, 2de67ab, P5's base in; never evidence)
- **What happened**: lane 3: base over at tick 3,060 (8 logs, 7 ingots, a second iron sword, a shield), the solid burrow at 3,420 (33 blocks, the mouth toward 61 of 120 sightings), two provokes from O (24.8 and 19.9 off), back in B 8 and 9 ticks after the creepy flag, two kills (243 and 172 ticks, 7 iron hits for the second), a pearl collected from M, 0 damage; 2 pearls in the run. Lanes 0 and 2 stood on fungus caps in the base's stem search for the cap (P5's wall, fixed in 57f25de).
- **Read**: after the two kills no calm enderman came into sight from M or O for 2,400 ticks: the burrow moved (36 blocks again).
- **Then**: P4's real runs (B never hit in 9 rows over 4 runs, M hit in 2 of 4): the bot holds in B while any enderman is within 8 of O (281ab40).
