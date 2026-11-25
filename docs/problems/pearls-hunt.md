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
- `plan::burrow` (pure): the site by cell, nearest the feet within 24 (B, M, O level; O and its outward neighbours three clear; four walkable beyond O; no lava within 4, no drop over 2 within 3, no bricks within 8; walls only non-holdable blocks or room for one), the shell (12 blocks at most), the missing blocks.
- `plan::pearls`: the nine-row table (`pearls_step` after the always-rows, `pearls_task` for the walks and digs) with its invariants; `[pearls]` in `config/goals.toml`.
- `look::keep_off_except` / `stares_except` (the provoke target only), `count_near_misses` skips it; `ender_threat` yields to a bot in its built burrow; `keep_look`'s eyes-down not in the hunt; `defend::box_distance_of` (2.9-high box); `cover::fill_block`.
- Tests: `crates/plan/tests/pearls.rs`.

## Walls
(none yet)
