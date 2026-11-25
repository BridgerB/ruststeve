# Problem: the burrow on a real server, and the pearl hunt in the model (issue 18a.4, 18g)

**Status: open** (agent P4). The index of issue 18's walls is [`pearls.md`](pearls.md).

## What the hunt needs
The hunt (P3, `docs/problems/pearls-hunt.md`) fights endermen from a level burrow one wide and two high: B the bot's cell, M the mouth, O outside where the enderman stands. The bot cannot heal, so the burrow's geometry must be proven on a real server before any race: an enderman comes to O and cannot hit the bot in B, the bot's strike reaches it, the roof cuts any stare from B, a pearl at O is collected from M and not from B, and M is too shallow to stand in with an angry enderman at O. Then the model must do the same (the parity table agrees) before a closed-loop test or a sim number counts.

## Risks
- **A kinder model than the game** (clojurecraft's sim hid the stare-freeze: its endermen came in anyway, its real bots died of it). The model walks an angry enderman in a straight line; the game paths it, for a mob 2.9 high, to the nearest reachable node.
- **Daylight and rain** in the overworld parity world: an enderman under open sky in daylight teleports at random once 600 ticks past its last target change, and rain hurts it. The arena rows put a glass roof at 4 over everything.
- **The free-standing burrow ties**: from B, O and the cells beside the side walls and behind the back wall are all 2.0 away; which one the game's pathfinder walks to is the real answer, not ours.

## Done, in world terms
- Rows `e04` to `e08` in `config/moments.toml` pass on the real server: e04 the enderman at O with the bot in B never hit; e05 it dies to strikes from B and never teleports; e06 not stared and not creepy; e07 the pearl picked up from M and not from B; e08 the bot in M hit.
- The model's rows match the real ones; the whole suite unchanged against the base; closed-loop tests in `crates/bot/tests/pearls.rs`.

## The rows (see config/moments.toml's comment)
Arena rows (e04, e05, e08) start in M: a stone floor 19 by 13, walls 4 high, a glass roof at 4, the burrow in its west part (back wall, sides and roof at 2), an enderman 10 east of M with its AI. `burrow:hold` stares from M at its eye until creepy, walks west into B (the block west of the feet solid: the back wall) and looks at its torso; `burrow:strike` also strikes from B every 14th tick (an iron sword's full charge is 12.5) while its box is within 3.0 of the eye, the attack then the swing; `burrow:near` stays in M. Each row with mobs or items writes `<id>.real.trace.txt` and `<id>.sim.trace.txt` (the bot and every enderman and item within 32, every 10 ticks).

## The real rows (parity 45531046147, b4d5a0d)
- **e04-mouth pass**: creepy about 10 ticks into the stare from M; the bot walked into B; the enderman walked from 10 east to O within about 30 ticks of anger and held there (its centre 1.30 to 1.74 past the M/O face; one stray to 3.25) for about 400 ticks; the bot never hit. The bot stopped 0.14 toward M from B's centre (forward released once the back wall was behind it), so the gap to its attack box was about 0.23, not the plan's 0.37.
- **e05-strike pass**: 7 iron hits (40, 34, 28, 22, 16, 10, 4, 0) from tick 88 to 240; never a teleport; each hit knocked it 1 to 1.7 east and it walked back, so about one hit in 25 to 30 ticks and about 150 ticks a kill; the bot never hit.
- **e06-roof pass**: looked in the eye from B at O, neither stared (data 18) nor creepy.
- **e07 pass**: the pearl at O not picked up from B, picked up from M.
- **e08-near**: see wall 1.

## Walls
1. **M under the roof was not hit in the first run** (e08-near, parity 45531046147; it was hit in the second, wall 3). The bot at M's centre, the angry enderman at O with its box at the face (centres 0.80 apart, inside the 1.43 reach) stood 240 ticks and never struck: its blow needs a line of sight eye to eye (`MeleeAttackGoal.canPerformAttack`, `Sensing.hasLineOfSight`), and the line from its eye (2.55) to the bot's (1.62) meets M's roof. So the burrow's depth buys the stare and teleport margin (from B the roof cuts every stare, e06) and the pickup reach (e07), not the hit margin against one enderman at O. . **The model's endermen walked along z** (parity 45532755681, a54212c: 167 of 170; e04, e05, e08 off). Issue 17's `ender_step` took the goal's z from the feet's height (`at[1]`), so every stroll and chase ran along z toward a point about 40 blocks off and stopped at the first wall. The path was right (probe 45534441711: next cell (9, 41, 0) toward the player while the body walked -z). . **A free-standing burrow ties O with three other cells** (parity 45535279839, 6879fa5, the second real run). For a 2.9-high mob the cells beside B (two to each side) and behind the back wall are 2 from B's cell, as O is; the partial path may end at any. In run 2 the enderman held O about 200 ticks, then walked round to B's side and behind the back wall and paced there to the end, out of strike reach. In the same run the bot in M was hit once (20 to 13) at about t335 after the enderman came back to O at an angle (run 1: none in 240 ticks). The bot in B was never hit (4 of 4 rows over both runs). Fix in the rows (and proposed for P3): the burrow is a solid block 4 long, 5 wide, 3 high with the tunnel in it, so O is the only cell at 2 (the next at the square root of 5). The same run built no arena at e05 (the bot sank into water): the start now retries a fill until the server says it filled.

## The model (18g, b4d5a0d and on)
- **The chase paths** (`crates/sim/src/enderman.rs`, `ender_path`): a breadth-first search over walk nodes for a mob 2.9 high (three cells clear over a floor, a step of one, a drop of up to three, corners only past two open sides, at most 1,024 nodes: `PathNavigation`'s follow range 64 × 16) to the node nearest the player's cell, ties to the first found; found again when the player's cell changes, when the next cell is reached, or every 10 ticks. A player in B is out of reach, so the path is partial and ends at O. Before this the model walked a straight line at the player: the gap that hid clojurecraft's stare-freeze.
- **The blow needs a line of sight**, eye to eye (`MeleeAttackGoal.canPerformAttack`: `Sensing.hasLineOfSight`), and its damage event names the enderman as cause and direct; `Life::hurt_by_mob` counts landed blows by kind. Other mobs' blows still send no cause, so issue 16's fights are unchanged.
- **A knock fits the whole box**: `tick_pushes` checks an enderman's 0.6 by 2.9 box (`ender_fits`), not the 1.79 column of the others.
- **Loot**: a summoned enderman keeps the generated table (an ender pearl 0 or 1); a parity row's `loot = true` keeps it in real too.
- **Items** laid by a row: `Sim::place_item`, announced at the join.
- Tests: `crates/sim/tests/pearls.rs` (walks to O and never reaches B; seven iron hits kill it at the mouth and no hit pushes it in; the roof cuts the stare from B; the pearl from M, not from B) and the closed loop `crates/bot/tests/pearls.rs` (the scripted bot provokes from M, steps to B and holds with no hit over three seeds; strikes from B kill it).
- Not modelled: carried blocks (`EndermanTakeBlockGoal`; waits on P1's read of `#enderman_holdable`), crits, the pathfinder's node costs (every node costs the same).
