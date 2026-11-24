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
| (a) soul sand | `soul_sand_flat`: soul sand wall to wall, bricks 30 east | `a_soul_sand_flat_is_crossed_to_the_bricks` (ignored) | `a_soul_sand_flat_has_a_route` (ignored) | A2 |
| (b) lava moat | `lava_moat`: 4 wide, 2 deep, wall to wall, bricks 6 past it | `a_lava_moat_is_crossed_to_the_bricks` (ignored) | `a_lava_moat_has_a_route` (ignored; the bot holds 64 cobblestone in slot 36) | A2 |
| (c) the tunnel | `lake_under_a_hill`: the shore at y 32, the lava sea 49 wide, bricks 60 east, 4 of air under rock to the roof | `a_lava_lake_under_a_hill_is_tunnelled_over_to_the_bricks` (ignored; four pickaxes) | `the_lake_and_the_stair_cut_have_no_walk_and_say_so` | A7 |
| (d) the seal | `stair_cut`: wall 22 to scale, a lava source woken beside the doorway of a one-wide stair, the bot at its foot | `lava_at_the_door_of_a_stair_cut_is_sealed_out` (ignored) | the same | A2, A7 |

Also in CI: `every_wall_is_joined_in_the_nether_with_its_bricks_in_view`, `no_route_on_any_wall_puts_the_body_in_lava`, and the model's own rules in `crates/sim/tests/nether.rs`.

To take a wall: remove its `#[ignore]` in the commit that crosses it. A wall that a change makes worse (a death where there was a stand) shows in the same output.

## Walls

### 1. What each wall does today
Not yet read: the first run of the ignored tests is the next entry.
