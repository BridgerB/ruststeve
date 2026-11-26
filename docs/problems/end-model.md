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

## Walls
(none yet)
