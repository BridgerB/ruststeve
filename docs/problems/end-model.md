# Problem: the End in the model and on a real server (issue 20a.4, 20h)

**Status: open** (agent E6). The index of issue 20's walls is [`end.md`](end.md).

## What the End needs
The crystal plan (a pillar 3 off, a wall at 2 off, a strike by reach), the dragon fight (the head's point and its blow) and the gaze discipline are built on what the server does with each. Each is proven on a real server first (rows `e01-frame` to `e06-drop` in `config/moments.toml`, development setups by RCON, never evidence), then modelled in `crates/sim` (a synthetic End) so the other agents' paths get closed-loop tests in `crates/bot/tests/end*.rs`. Dragon flight is not modelled: the fight is judged real-only.

## Done, in world terms
- `e01-frame`: a use of an eye on an empty frame's top fills it, and the 12th lights the 3x3 inside.
- `e02-enter`: walking into a lit end portal lands in `minecraft:the_end` at the platform (100.5, 49, 0.5).
- `e03-strike`: a crystal on bedrock struck from 3 off through end stone at 2 off (feet and eye): it dies, the bot takes no blast damage, the wall stays (end stone is dragon-immune and resists 9).
- `e03-bars`, `e03-cage-wall`: the caged crystal struck with the bars in place (E1: blast rays pass the bars' gaps, so damage expected), and with the two bars at 2 off swapped for end stone (the plan).
- `e04-gaze-*`: the stare cone at 12 (3 aside angers, 6 aside does not, straight down does not).
- `e05-head`, `e05-neck`, `e05-blow`: the sitting dragon's head 6.5 along (sin yaw, -cos yaw) and 1 under its feet; a strike lands on it from 2.0 out of its blow's box; one blow through full iron with boots (E1's jar: 6.0).
- `e06-clutch-*`: a 21 fall onto poured water costs nothing; `e06-drop` the control (18).

## The rows (how they are built)
- Each on its own landing, 40 east and 40 north of its centre (`e04-gaze-down` and `e06-*` 40 west and 40 south), at Normal, in full iron with boots.
- `lift` stands the start over the surface: the real side tps to the ground first (the chunk loads for the `fill`), lays the row, then lifts the bot onto its pillar.
- `report` writes `<id>.<real|sim>.end.txt` (health at each change, hits, blasts, crystals, dragons and endermen with health, dimension, the watched cells' full states).
- Scripts `crystal:<every>`, `dragon:<every>:<part>` (the dragon's parts are entity ids after its own: 1 head, 2 neck, 3 body; the client is never told where they are, so the server's verdict on a strike is the probe of where a part is), `clutch:<yaw>:<walk>:<above>`.
- parity.yml `only` and `tolerance` inputs run the e rows alone.

## The real rows (parity 45864998071, 6dac60c), read from each row's `real.end.trace.txt`
- **e03, the strike works by reach through cover, and the cover is spent.** In `e03-strike` (end stone at 2 off at the feet and the eye) the crystal was struck through the wall and removed, and the bot lost **0.42** (20 to 19.58). That is `ServerExplosion`'s floor: a fully hidden player within 12 still takes 1.0 before armour, 0.42 through 15 points. **Both wall blocks were air after the blast**: the damage is dealt before the blast breaks blocks, so the wall shields exactly one blast.
- **e03-bars**: the cage's own bars at 2 off, left in place. The strike went through, and the loss was the same 0.42 (exposure 0: a bar on the cage's x edge is joined north and south, a full-width pane facing the stand). The two bars at the feet and the eye were air after; the one at the bedrock's level stayed. **e03-cage-wall** (those two swapped for end stone): the same 0.42, both blocks air after. So for a cage no bar needs breaking.
- **e03-open** (no wall): dead through full iron with boots.
- **e04**: an enderman 12 east, the look 3 degrees aside of its eye: creepy (angered) at t 49. 6 degrees aside: calm. Straight down: calm. The cone at 12 lies between 3 and 6 degrees (the jar's 0.025 / d: 3.7).
- **e06**: a 21 fall off an end stone pillar, the water bucket poured falling (tried each tick from 4.0, or 2.0, over the ground): the water went in and there was no damage in either row. The model matched the 2.0 row; in the 4.0 row the model never poured (its bucket's reach from a falling body differs: open). `e06-drop` landed in the ocean (landing 3 is sea) and is moved to dry ground.
- **e02**: walking into a lit end portal arrived at **(100.500, 49.000, 0.500)** in `minecraft:the_end` (t 70). The script kept walking east and fell into the void, as a blind bot would.
- **e01**: the use of an eye on the empty frame's top set `eye=true`. The ring with every frame facing out did not light; `e01-frame-in` tries them facing in.
- **e05**: a dragon with no AI never moves its parts. No strike on part 1 or 2 landed (health 200 throughout), and walking into where the head should be drew no blow. The rows now summon it with its AI in phase 6 (sitting scanning).

## The rerun (parity 45866776019, 3c7b5eb)
- **e01-frame-in**: with every frame facing **in** (toward the ring's centre), the eye in the 12th lit the whole 3x3 (`end_portal`). Facing out does not light.
- **e05-head**: a sitting dragon (phase 6, with its AI, yaw 0), the bot 2.0 off the head box predicted at 6.5 along (sin yaw, -cos yaw) and from y - 1 to y. Strikes on part id + 1 every 14 ticks with an iron sword: 200, 194, 188, 182, 176, 170. **Every strike landed the full 6.0, and the bot was never hurt** (it stood outside the blow's box, the head's grown by 1).
- **e05-neck**: part id + 2 takes 2.5 a strike (6 / 4 + 1); the dragon's own id is its body, the same rule.
- **e05-blow**: walking into the head's box: 20 to 14.0, then 14.83 to 8.83 ten ticks later. **One head blow is 6.0 through full iron with boots**, and they come every 10 ticks.
- **e06-drop** (dry ground): the 21 fall with no water cost 18 (20 to 2). Set against e06-clutch's 0, the pour is the whole difference.

## The model (20h)
- `crates/sim/src/end.rs`: the synthetic End (`EndStage`, `Layout::small()`) and the crystal's blast, with exposure as `getSeenPercent` counts it (iron bars by their post and arms). The model's floor matched the real 0.42 to the hundredth.
- **The dragon** (`end::Dragon`, ae32015): scripted, not the AI. It is perched (phase 6, its yaw by a `TeleportEntity`) and is hit by part: the head takes the swing, any other part swing / 4 + min(swing, 1). The head and neck blow for 10, grown by 1. A perch ends after 50 taken sitting, a 400-tick flight follows, and it sits again. At its death come entity event 3, and 200 ticks later the exit portal within 2.5. Feet in the portal get `win_game`, and the `client_command` 0 after it is counted.
- **Not modelled:** the blocks a blast breaks (the real wall at 2 off is spent, above); the dragon's flight paths, breath, wings and charges; the crystals' heal; the portal's travel; the frame fill; the overworld after the credits.

## Walls
1. **The cover at 2 off is consumed by each blast** (e03, parity 45864998071). A second crystal struck from the same stand without rebuilding the wall meets an open stand (`e03-open`: dead). For E4, rebuild or re-check the two cells before every strike, never assume them.
2. **The last crystal leaves the bot on top of its pillar** (closed loop, `crates/bot/tests/end_loop.rs`, CI 45871565862 and probe 45873677573 on b818d90). The bot pillars, rings, walls, strikes (open spike reach 2.09, caged 1.94 through the bars, health 19.58 after the blast) and the crystal is gone. Then `end_task` hands over to the spikes only while `crystals_alive > 0`, so `spikes_task`'s dig down never runs. The bot stays 15 to 18 up, and the ledger shows "no walk to the bunker" 24 times. E3 owns the fix in `end.rs`. **Fixed** by E3's 7b4ccc1 (the spikes also run while the bot is up a pillar): both spike tests pass with the dig down (probe 45877283204 on ae32015).

## Closed-loop tests (`crates/bot/tests/end_loop.rs`)
These run the planner itself (`Aim::Race`, config/goals.toml) on the synthetic End, through `bot::lockstep`. They are development evidence only.
- **The bridge**: platform to lip, the lip at y 52 out to x 72 as E1 measured. The bot pillars up 4, lays 27 floor blocks and 58 guards (rails at the feet's height on both sides of every cell over the void), and steps on at (72, 53, 0) in 600 ticks; the feet never go under 49. It passes (probe 45873677573).
- **The gaze**: three endermen at eye height on the island. The bot walks to the bunker's site, digs it and tunnels, and never stares at or angers any of them. It passes (probe 45873677573).
- **Open and caged spikes**: walk, pillar, guard ring, wall (or the bars), strike, dig down. They pass (probe 45877283204). Each run is 2500 ticks: the bot is down by about tick 700, and 8000 ticks spent 520 s walking to the test's unreachable bunker.
- The island in the tests is 20 deep: with 3 deep, the bunker's third dig opened onto the void and the bot fell. The real island is tens deep near the fountain, so this is a test layout fix, not a bot wall.
- **The dragon** (E5's `dragon.rs`, the sim's scripted `end::Dragon`): perched on the pillar facing north. The bot digs its bunker, opens it, goes behind the dragon and to the head stand, and strikes until the dragon is dead; each quarter of its health sends it off for 400 ticks. Then the bot walks into the exit portal, gets `win_game` and sends the respawn. It passes (probe 45877283204), with the health never under the danger line and no model violation.
- **Wall 3, the fountain one too high** (sim, fixed in ae32015): the model laid the bowl's floor one over the island's top, so the perched dragon's head was out of every head stand's reach. The bot held at the stand behind for 16,000 ticks with 0 strikes (probe 45876437928). The jar lays the floor flush (end.toml `[fountain]`), and so does the model now.

## The water clutch window (rows e09-pour, parity 45929976619 on c0ea051)
Setup: 35 rows on a real server, walking off an end stone pillar onto end stone, looking straight down, in full iron at Normal. Each row makes one use of the water bucket only. K is how many ticks before the bot's own predicted landing the use is sent (1: the tick whose move lands); "reach" is the first tick the eye is within 4.5 of the ground; "drop" has no bucket. "eye" is the eye's height over the ground at the position the use was sent from, which is the body's position before that tick's move (a tick's acts go out before its move packet).

| drop | reach | K 1 | K 2 | K 3 | K 4 | K 5 | no bucket |
|---|---|---|---|---|---|---|---|
| 8 | water, 0 lost (eye 3.93) | water, 0 (2.06) | water, 0 (3.03) | water, 0 (3.93) | dry, hurt to 18.3 (4.78) | dry, 17.0 (5.56) | 18.3 |
| 15 | water, 0 (3.37) | water, 0 (2.07) | water, 0 (3.37) | dry, 10.0 (4.62) | dry, 11.2 (5.81) | dry, 10.0 (6.95) | 10.0 |
| 25 | water, 0 (3.19) | water, 0 (3.19) | dry, dead (4.79) | dry, dead (6.34) | dry, dead (7.85) | dry, dead (9.30) | dead |
| 40 | water, 0 (3.78) | water, 0 (1.79) | water, 0 (3.78) | dry, dead (5.72) | dry, dead (7.63) | dry, dead (9.50) | dead |
| 60 | water, 0 (2.26) | water, 0 (2.26) | dry, dead (4.56) | dry, dead (6.84) | dry, dead (9.08) | dry, dead (11.29) | dead |

- **Water was placed exactly when the eye was within the bucket's 4.5 of the ground at the position the use was sent from.** Every use with the eye at 3.93 or less placed water; every use at 4.56 or more placed none. That holds whatever the drop and whatever K.
- **The server judged the reach from the last move it had heard, not from the predicted one.** e09-pour-15-3 was sent with the eye at 4.62, and its tick's move would have brought it to 3.37: no water. The same holds for 25-2 (4.79 then 3.19) and 60-2 (4.56 then 2.25). An early pour misses because the server measures the reach from where the body was.
- **A late pour is not too late.** K 1, sent on the tick whose move lands, placed water and took 0 damage at every drop, 60 included (falling 2.34 a tick). The use is processed before that move, so the feet land in the water.
- **So the right rule is height, not ticks:** pour while the eye over the ground at the current body position is 4.5 or less, which with the eye at 1.62 means the feet 2.88 or less over the ground. Keep a margin, and pour every tick until the water shows. "Reach" (the first tick in reach) took 0 at every drop. A count of ticks before the landing goes out of reach as the fall speeds up: K 2 failed at 25 and 60 and K 3 at every drop from 15 up.
- **The race's death fits.** Race 14 / 45927433568 lane 0 poured 3.31 over the ground, with the eye at 4.93, out of reach; nothing was placed.
- Not explained: e09-pour-8-4 shows water in a watched cell at t 123, long after its landing at t 45, with the bucket still full at the end of the row.
