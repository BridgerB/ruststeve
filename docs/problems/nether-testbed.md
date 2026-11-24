# The Nether test bed (issue 16)

## Needs
The Nether walk is the wall of issue 16 (blaze.md walls 22 and 23 and the stop after them) and every experiment on it was a real race of 15 to 60 minutes. Two attacks on it (the route: soul sand as ground, a narrower lava rule, a bridging move; the tunnel: over the lava sea, level to the bricks) need to try a crossing in CI in minutes: a Nether in the model, the Nether's own rules where they differ, and closed-loop tests that state each wall as an outcome.

## Risks
- A model that is kinder than the game: every rule here is read from the 26.1.2 jar (javap) and tested against its numbers. What is left out is listed below, so a green test is never taken for a race.
- Sim numbers do not count as issue-16 evidence (measurement-rules): these tests are where a fix is developed. A race is still the proof.

## Done, in world terms
A change to the route or the tunnel is tried on the four walls below in one CI run, and the wall it claims to break goes from ignored to passing in the same commit.

## What exists
- `sim::nether` (`crates/sim/src/nether.rs`): `Stage::new(radius, floor, ceiling)` or `Stage::over(x, z, floor, ceiling)` lays a cavern in `minecraft:the_nether` (dimension type 3 of the synced registry, 16 sections from y 0, bedrock at y 0 and 127); `fill`, `air`, `lava_lake`, `soul_sand`, `soul_soil`, `magma`, `gravel`, `bricks`, `bridge`, `set`, `spawn`; `build()` gives the `ModelWorld`. `sim::nether_world(radius, floor, ceiling)` is the empty cavern, `flat_world`'s like. `Sim::setblock(pos, state)` sets a block with its updates.
- `sim::nether::walls`: the walls as stages, each a `Wall { stage, start, bricks, wake }` with `wall.sim(config)` and `wall.world()`.
- Lava (`crates/sim/src/fluid.rs`): where the dimension has fast lava (`gameplay/fast_lava` in the Nether's dimension type; `LavaFluid.isFastLava`) the tick delay is 10 not 30, the drop-off 1 not 2 (7 cells from a source, not 3) and the slope search 4 not 2. A lake laid in a stage is at rest and runs only once a cell beside it changes, as the game's does when a bank is dug.
- Magma (`crates/sim/src/life.rs`): on the ground with a magma block 0.2 under the feet and not sneaking, 1 `hot_floor` a tick, which the 20 ticks of invulnerability make 1 each 10 ticks; fire resistance refuses it.

## The real exit
`crates/bot/tests/fixtures/nether-exit.rsw`: the 77 chunk columns (x -1 to 5, z -4 to 6: blocks x -16 to 95, z -64 to 111) round this world's Nether portal exit and on to the bricks, byte for byte as the server sent them to lane 5 of race 45194253932 on its arrival, cut from its recording at server tick 25,500 by the ignored tool in `crates/bot/tests/nether_capture.rs` (probe 45336288184; 158 KB). The portal is at (22 to 23, 34 to 35, -20); the nearest bricks at (61, 30 to 37, 95), 121 blocks from the snapshot bot's place; 1,927 brick cells were on the bot's record. `crates/bot/tests/nether_exit.rs` plays it: the fixture holds blaze wall 22's block view, and the kit bot's own situation (the snapshot's `snap.toml` items and position, the fortress aim) is an ignored closed-loop test. Outside those columns the model is solid and the bot's world unloaded. It is the server's side of the test: the bot still sees by exposed faces only.

In the box x 16 to 47, z -32 to -1, y 28 to 36 round the portal (read from the snapshot's saved region): netherrack 4,037, lava 1,522, air 1,330, soul soil 952, magma 749, soul sand 306, gravel 122. The "soul_s" cells of the probe's block view beside the stair are soul soil, a full block the walk stands on, not soul sand.

## Found on the way
- **The bot cannot sneak, to a server.** `MagmaBlock.stepOn` spares an entity that `isSteppingCarefully` (`isShiftKeyDown`), which a server sets from the `player_input` packet. The bot's protocol has no `player_input` (`C2s` has none), so `Controls::sneak` never leaves the client: on a real server a "sneaking" bot burns on magma all the same. `Life::sneaking` in the model is false unless a test sets it.
- **Soul sand does not slow the bot.** Vanilla multiplies the horizontal velocity by the block's speed factor after each move (`Entity.getBlockSpeedFactor`: soul sand 0.4, honey 0.4). `crates/physics` has no block speed factors at all (nor ice's friction), so nothing was added there: the bot's client walks soul sand at full speed, and a server accepts it (it checks distance, not speed). A route across soul sand costs the same per block as netherrack in the real game for this bot; if physics ever gains speed factors the walk slows to 40%.

## Left out of the model, and what that hides
- Soul sand's speed factor (above); gravel does not fall; lava lights no fire; no mobs (ghasts, piglins, the blazes); lava's random 1-in-4 slower spread when a flow shrinks (`LavaFluid.getSpreadDelay`).
- A partial shape never occludes a flow (the fluid module's note), which matters for soul sand banks only at the eighth of a block they lack.

## The walls as tests
`crates/bot/tests/nether_walls.rs` (closed loop: the planner with `Aim::Fortress`, a stone pickaxe and 64 cobblestone, against the model) and `crates/plan/tests/nether_routes.rs` (`plan::path::plan` alone on the same stages). Run the ignored ones with `cargo test -p bot --test nether_walls -- --ignored --nocapture` on a runner (the probe). Each failure prints where the bot ended, its health and the planner's ledger.

| wall | stage (`sim::nether::walls`) | closed loop (bot) | pathfinder (plan) | owner |
|---|---|---|---|---|
| control | `netherrack_flat` | `a_netherrack_flat_is_walked_to_the_bricks` | `a_netherrack_flat_has_a_route` | runs in CI |
| (a) soul sand | `soul_sand_flat`: soul sand wall to wall, bricks 30 east | `a_soul_sand_flat_is_crossed_to_the_bricks` (passes today, runs in CI) | `a_soul_sand_flat_has_a_route` (ignored) | A2 |
| (b) lava moat | `lava_moat`: 4 wide, 2 deep, wall to wall, bricks 6 past it | `a_lava_moat_is_crossed_to_the_bricks` (ignored) | `a_lava_moat_has_a_route` (ignored; the bot holds 64 cobblestone in slot 36) | A2 |
| (c) the tunnel | `lake_under_a_hill`: the shore at y 32, the lava sea 49 wide, bricks 60 east, 4 of air under rock to the roof | `a_lava_lake_under_a_hill_is_tunnelled_over_to_the_bricks` (ignored; four pickaxes) | `the_lake_and_the_stair_cut_have_no_walk_and_say_so` | A7 |
| (d) the seal | `stair_cut`: wall 22 to scale, a lava source woken beside the doorway of a one-wide stair, the bot at its foot | `lava_at_the_door_of_a_stair_cut_is_sealed_out` (ignored) | the same | A2, A7 |
| the real exit | `fixtures/nether-exit.rsw` | `the_kit_bot_reaches_the_bricks_from_the_real_exit` (ignored; `nether_exit.rs`) | | A2, A7 |

Also in CI: `every_wall_is_joined_in_the_nether_with_its_bricks_in_view`, `no_route_on_any_wall_puts_the_body_in_lava`, and the model's own rules in `crates/sim/tests/nether.rs`.

To take a wall: remove its `#[ignore]` in the commit that crosses it. A wall that a change makes worse (a death where there was a stand) shows in the same output.

## Walls

### 1. What each wall does today (probe 45335940127, facb439)
- **Read**: the probe's `probe-out/bot.txt` and `plan.txt`, every test run with `--include-ignored`.
- **The model's rules**: 6 of 6 pass (`crates/sim/tests/nether.rs`).
- **Control**: the netherrack flat is walked to the bricks at 20 health; the pathfinder finds its route.
- **(a) soul sand**: the pathfinder has nothing: `Status::None` after 0 expansions, the start and all four cells round it not standable. The bot crosses all the same, at 20 health: with no walk the fortress task falls to its level tunnel, which steps over the soul sand. So (a) is a wall for the route only; the closed-loop test passes and now runs in CI as a guard.
- **(b) the moat**: the pathfinder answers `Partial` after 6,000 expansions, its route ending at the bank (9, 34, 0). The bot lives at 20 health and never crosses: 2,400 ticks of "tunnel toward the fortress" tasks that each end "cut 6 stairs" in 40 to 60 ticks with nothing cut and nothing refused, and explores to the side between them. It ends 27 blocks from the bricks. No task says the lava stopped it.
- **(c) the lake under the hill**: the same for 8,000 ticks: 233 ledger lines of level tunnels along the shore that end "cut 6 stairs" and explores; it never climbs into the hill. Alive at 20 health, 82 blocks from the bricks at the end.
- **(d) the seal**: dead. The stair task runs from tick 20 to 60 and ends with an empty outcome; the lava escape starts at 60 ("trapped in lava"), seals the head's cell (0, 32, 0) at ticks 61 and 71, then "no bank or notch from (0, 31, 1)" at 81, 101 and 121; health 2.3 at its lowest, then 0. The lava was beside the head from about tick 50 and the escape began only once the body was in it; one step into its own tunnel, the bot had nowhere to go.
- **Cause**: open; these are the owners' walls. What the test bed shows is that none of (b), (c), (d) needs a race to see.

### 2. The kit bot on the real exit (probe 45337130636, fde50e0)
- **What happened**: `the_kit_bot_reaches_the_bricks_from_the_real_exit`, 12,000 ticks: alive at 20 health the whole way, never at the fortress, no server violation. It goes down from (22, 34, -20) to y 29 by tick 600, y 21 by 2,400, and from tick 4,800 to 12,000 stays within x 4 to 22, y 18 to 24, z 29 to 61: under the lava sea, 40 blocks short of the bricks in z.
- **Read** (its ledger, 203 lines): ticks 20 to 540, four "stairs down toward the fortress" that end `Boxed` on "lava beside (26, 28, -19)", "(22, 28, -18)", "(20, 28, -17)", "(23, 28, -19)"; then level tunnels that each end "cut 6 stairs"; from tick 2,360 on, "stairs up toward the fortress at (61, 22, 95)" and "stairs down toward the fortress at (61, 23, 95)" take turns, 220 to 340 ticks each, both ending "cut 6 stairs"; refusals "magma under (9, 24, 28)" and "no floor at (22, 20, 62)"; near tick 9,800 the race's own line, "stairs failed: StepFailed after 0 stairs (last refusal: ; turn the heading)", and "relocate (1 of 6) (12 tasks failed in a row with none done)".
- **What it shows**: the model gives the race's symptoms on the race's ground with no race: the stair that goes down because the nearest brick it names is lower each time (the target's y follows the bot: 33, 28, 26, 25, 23, 22), the up and down that cancel, the empty refusal. It does not give the lava deaths of wall 22: no bot died here in 12,000 ticks (one run, one seed; and no ghast or piglin exists in the model).
- **Cause**: the owners'. The stage is here to try it on.
