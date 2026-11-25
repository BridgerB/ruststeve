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

## Walls
