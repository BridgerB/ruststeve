# Issue 19: the way home's last leg and the step into the portal (agent Q6)

The orchestrator's index is [`eyes.md`](eyes.md); the way home is agent Q2's ([`eyes-home.md`](eyes-home.md)); this file is agent Q6's: the last 30 to 100 blocks to the bot's own entry portal, the approach to its front, the seals, the step in and the enter.

## Needs
A bot on the way home in the Nether, about 100 blocks south of its entry portal (the endermen frontier's old pocket at (8, 85, 81) in the soul sand valley; or the blaze spawner's side, if Q2 routes there), with 14 eyes, reaches the portal's front and goes through it. The portal: cells x 22 to 23, y 34 to 36, z -20, frame x 21 to 24, y 33 to 37, on a ledge over the lava sea; the only dry standing cell is (22, 35, -19) on the bot's own cobblestone, with lava at (21, 35, -19), (23, 34, -19) and (24, 35, -19) and falling at x 20 and 24 to 26; row z -18 behind it is netherrack; the north face has no floor over the lava sea (Q1's probe 45688112814).

## Done (world terms)
2 of 2 development bots from `dev-nether-home` (resumed from the frontier, 14 eyes) through the portal into the overworld, the dimension change noted in the ledger, on a commit green in CI. Development knowledge, never evidence.

## Risks
- The frontier bot's health is 10 with nothing to heal with: every hit is kept, and soul sand valley skeletons shoot from 30.
- The ground drops 50 to 60 blocks from the soul sand valley to the ledge.
- The lava round the standing cell is falling lava: a seal may be washed out again from above.
- One world, one portal, one ledge.

## What is built (branch `issue-19-q6-ledge`, `crates/plan/src/portal_way.rs`)
Within `[portal_way] reach` 64 of the portal on the plane, with its front seen (`home::portal_front`), the heading stands down: a tunnel through rock to the front's tunnel cell (22, 35, -18); within `cut_range` 12 an exact cut of level steps and stairs to that cell itself (the tunnel ends within its reach of 5); at the tunnel cell or the standing cell, Q2's approach (the seals round the standing cell, the walk in, `Task::Enter`). Called from one fenced block in `home_trip`, just before the approach. Unit tests: `crates/plan/tests/portal_way.rs`.

## Walls
1. **The last leg never reached** (race 45727347488 at b074f2f, `dev-nether-home`, 2 lanes x 1 bot, cap 12,000). Neither bot came within the approach's 24. Lane 1 (health 15.7 to the cap) swung east and west for 6,000 ticks between (-5, 97, 16) and (43, 102, 24), 48 off the portal and 60 over it: each leg re-aimed at the point (23, -20) on (0, -1), then turned off it at once ("245 crimson signs seen ahead", "465 crimson signs seen ahead"), so no leg ever went north. Lane 0 (from health 10) went past the portal on the east at x 66 to 77, y 44, z -6 to -45, 54 to 59 off it, and was shot down by three soul sand valley skeletons at tick 5,440 ("death.fell.assist skeleton", a pocket begun at 6.3). Fix: the last leg (above) from 64 out, by rock, no biome sign asked.
