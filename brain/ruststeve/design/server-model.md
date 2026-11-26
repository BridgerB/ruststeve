---
title: The server model
description: How ruststeve plays without a Java server - the sim crate (the vanilla server as a pure reducer, grown per issue), S2c encoders that make the model's bytes a server's, the model as the bot's body behind the same channels a socket gives, and what it models so far.
type: reference
tags: [sim, model, server, body, closed-loop]
aliases: [air supply, drowning, fire, lava, hunger, eating, melee, Life, summon, sim crate, Sim, server model, survival, digging model, item entities, window 0 model, delayed destroy, model body, In, Wire, ModelWorld, flat_world, S2c encode, NotModelled, movement check, moved wrongly, moved too quickly, fall damage, lockstep, generated worlds, sim terrain, 1000 worlds, nether_world, Stage, Wall, Nether stage, magma, hot_floor, fast lava, setblock, enderman, endermen, stare, simulation distance, frozen mobs, ticking, biome palette, warped forest, natural spawning, eye of ender, stronghold, with_stronghold, eye flight, the End, EndStage, end crystal, crystal blast, seen_percent, cage, ender dragon, summon_dragon, exit portal, win_game, credits]
status: verified
lastUpdated: 2026-11-26
verifiedAgainst: ruststeve@83dba95 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/sim/src/eye.rs#pub fn eye_step(e: &mut Eye) {"
  - "ruststeve:crates/sim/src/lib.rs#pub fn step(&mut self, input: &In) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#fn chunks_around(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/protocol/src/packets.rs#S2c::LoginFinished { uuid, username } => {"
  - "ruststeve:crates/bot/src/model.rs#pub fn connect(mut sim: Sim) -> Connection {"
  - "ruststeve:crates/bot/tests/model.rs#fn the_bot_joins_the_model_and_walks() {"
  - "ruststeve:crates/sim/src/lib.rs#fn on_move(&mut self, to: [f64; 3], on_ground: bool) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#fn fall(&mut self, dy: f64, on_ground: bool) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/terrain.rs#pub fn world(seed: u64, radius: i32) -> Result<ModelWorld, EncodeError> {"
  - "ruststeve:crates/sim/src/survival.rs#fn stop_destroy(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/survival.rs#pub fn craft_result(width: usize, grid: &[Option<ItemKind>]) -> Option<Stack> {"
  - "ruststeve:crates/sim/src/survival.rs#pub(crate) fn broadcast_changes(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/tests/survival.rs#fn an_early_finish_is_a_delayed_destroy_not_an_abort() {"
  - "ruststeve:crates/bot/src/lockstep.rs#pub fn run("
  - "ruststeve:crates/bot/tests/worlds.rs#fn the_bot_walks_a_thousand_generated_worlds_clean() {"
  - "ruststeve:crates/sim/src/enderman.rs#pub(crate) fn tick_endermen(&mut self) -> Result<Vec<Wire>, EncodeError> {"
  - "ruststeve:crates/sim/src/lib.rs#pub fn ticking(&self, pos: [f64; 3]) -> bool {"
  - "ruststeve:crates/sim/src/nether.rs#pub fn biome(&mut self, x: Span, z: Span, biome: Biome) -> &mut Self {"
  - "clojurecraft:src/clojurecraft/sim.clj#Two ways to drive it: run folds bot and model together, deterministic and fast (generated worlds); connect puts the model on a thread behind a channel pair shaped like conn/open!'s, so main's own loop (its clock, its effects, its tap) runs against it with no Java process."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[cc-sim]]"
  - "[[body-boundary]]"
  - "[[measurement-rules]]"
---

# The server model

Issue 08: the vanilla server as a pure reducer, so the bot can be tested closed-loop without a Java process ([[cc-sim]]). It **grows per issue** (Bridger, 2026-11-12): each issue adds the part of the server its gameplay meets, with parity rows, before any sim number counts ([[measurement-rules]]).

## Key files
- `crates/sim/src/lib.rs`: `Sim`, `In`, `Wire`, `ModelWorld`, `ModelColumn`, `flat_world`.
- `crates/protocol/src/packets.rs` (`S2c::encode`), `crates/protocol/src/chunk.rs` (`Paletted::pack`, `Column::encode`, `packet_body`).
- `crates/bot/src/model.rs`: the model as a connection; `crates/bot/tests/model.rs`.

## How it works
- **`Sim::step(&In) -> Vec<Wire>`** (core: no I/O, no clock): a client packet (`C2s`) or a server tick in; the server's packets out as the wire carries them (state, id, bytes). Who ticks it is the caller's choice.
- **The bytes are a server's**: every packet the model sends is written by `S2c::encode`, the inverse of `decode` (a property test round-trips each, and a chunk column cell for cell). A packet the model does not send yet is `EncodeError::NotModelled`, never invented. A captured world's columns go out as the server's own bytes; a generated one's through `Column::packet_body`.
- **The model as the bot's body**: `bot::model::connect(sim)` runs it on a thread behind the same `inbound`/`outbound` channels `net::connect` gives, decoding its bytes as the reader thread does, and ticking every 50 ms. `Opts::model` picks it; the bot's loop, recordings and replay are unchanged ([[body-boundary]]).
- **What 08a models**: login (offline uuid), configuration (known packs, finish; the bot reads no registry), the play login with the race's view and simulation distances, chunks before the first teleport, health, chunk batches within the view distance and `forget_level_chunk` as the player moves, `set_time` every 20 ticks (since issue 16 with the game time, and in the first one both world clocks as a real join sends them; `Sim::day_time` starts at 0 and a test may set it to start at dusk), a keep-alive every 300 ticks with vanilla's 15 s kick, and moves ignored until the teleport is accepted. What a real server would refuse is kept in `violations`.

## The movement check and falls (08b, read from the 26.1.2 jar)
The 26.1.2 server jar is not obfuscated, so the rules are read with `javap` from `ServerGamePacketListenerImpl.handleMovePlayer`, `Entity.checkFallDamage`, `LivingEntity.calculateFallDamage` and `VoxelShape.collideX`, never from memory:
- **Too quickly**: the squared distance from the tick's first position over 100 × the move packets this tick (more than 5 count as 1) → the server sends the player back (`player_position`, a correction).
- **Wrongly**: the server moves the old position by the client's delta with its own collision; the error to the client's position, its y forgiven inside (-0.5, 0.5), over 0.0625 squared → a correction, but only when the old box was collision-free. **Colliding with anything new**: the new box (deflated 1e-5) in a block the old box was not in → a correction.
- **Falls**: falling (not in water) adds to the fall distance; landing hurts `floor(fall + 1e-6 - 3)` (safe fall 3, multiplier 1) and resets it; moving up resets it. A hurt sends `damage_event` (type `fall`) and `set_health`; at 0 health `player_combat_kill` with `death.attack.fall`.
- Not modelled: the server's own velocity for the player (knockback adds to the allowance), elytra, sleeping, vehicles.

## Digging, drops, pickup and window 0 (10b, read from the 26.1.2 jar)
`crates/sim/src/survival.rs`, each rule read with `javap` ([[mc-dig-timing]]):
- **Digging** (`ServerPlayerGameMode`): START times the dig from the server's own tick; the progress per tick is `getDestroyProgress` (speed / hardness / 30, or 100 when the hand may not harvest it; the hand's speed 1, × 0.2 with the eyes in water, ÷ 5 off the ground). FINISH is accepted once progress × (ticks + 1) ≥ 0.7. **An earlier FINISH is a delayed destroy**: the server's tick keeps adding progress and breaks the block itself at 1.0 (one pending at a time). Every action is acknowledged with `block_changed_ack`, after the block update. Out of reach (eye to box over 4.5 + 1) the action does nothing but the ack.
- **Drops**: the block's first loot pool from the generated tables, only when the hand may harvest it; `Block.popResource`'s item entity (random offset ±0.25, velocity ±0.1 sideways and 0.2 up, pickup delay 10) from the model's seeded roll; sent as `add_entity` and `set_entity_data` (the stack at index 8). Items fall (gravity 0.04, drag 0.98, ground friction 0.588), a sideways move into a block stopped on that axis (issue 10c: a drop beside a trunk sank into the higher ground next to it); the client is told where every 20 ticks (`entity_position_sync`); they despawn at 6,000 ticks.
- **Support**: a block that rests on the broken one (plants, leaf litter, flower beds, snow, torches, carpets: an allow-list of the blocks report's `definition`) pops off first with its own drops, as many as its `segment_amount`; a two-high plant loses both halves. Found by the parity table (leaf litter on grass).
- **Pickup**: `Player.aiStep`'s touch box (the player's grown 1 sideways and 0.5 up) over an item with no delay left; `Inventory.add` (same-item stacks: the held slot, the offhand, then inventory order; then the first free slot, the hotbar first); `take_item_entity`, `remove_entities`, then the menu's changes.
- **Window 0**: the 46 menu slots, the cursor, the state id and the server's view of the client's (remote) slots and cursor. A click: the client's claim becomes the remote view (the bot claims nothing: an empty cursor), `doClick` runs (mode 0 pick/place/swap/merge with left and right, the result slot taken whole; mode 1 the result crafted again into 9-44 from the hotbar's end while it fits, other slots as `InventoryMenu.quickMoveStack`), then **either** `broadcastChanges` (one `container_set_slot` per changed slot, each a new state id, and `set_cursor_item` when the cursor differs from the claim) **or**, for a stale state id, the whole window. A grid change sends the result at once with its own state id (`slotChangedCraftingGrid`), so it arrives first. The result comes from the generated recipes (shaped as laid or mirrored, shapeless). A click that changes nothing sends nothing. Closing window 0 puts the grid back and throws the cursor. The join sends `set_held_slot` and window 0 at state id 1.

## Placing, the crafting table and tools (11a, read from the 26.1.2 jar)
- `use_item_on` ([[placing]]): reach, a crafting table opening its menu, a held block placed in the clicked cell when replaceable else beside it and never in the player, then the two block updates and the ack.
- The crafting table's menu ([[table-window]]): container ids, `open_screen`, the 3x3 grid over window 0's click rules with `CraftingMenu.quickMoveStack`'s ranges, `stillValid`. Closing any menu puts the grid **and the cursor** back into the inventory (10b threw the cursor).
- The dig progress and drops come from `data::dig` with the held item: a tool's speed and drops from its `tool` component ([[tools-and-held-slot]]); stacks by each item's `max_stack_size`.
- **The eyes in water** are judged as `isEyeInFluid` judges them: under the water's surface in its cell (`amount / 9` of a block, a full block under more water; waterlogged blocks, kelp, seagrass and bubble columns count). Found by the parity table at a water landing.

## Air, fire, hunger, eating and melee (12a, read from the 26.1.2 jar)
`life.rs` holds `Life`: food, saturation, exhaustion, air, fire ticks, the invulnerable window, the use in hand, a hunger effect, water breathing, the attack ticker and the summoned mobs. `tick_life` runs once a server tick:
- **Air** (`LivingEntity.baseTick`): 300; −1 a tick with the eyes in water (the fluid's height, as for the parity rows), 2 drowning damage at −20 (then back to 0); +4 a tick out of water. Water breathing (a moment's `breathe`) stops the loss.
- **Lava**: 4 damage and at least 300 fire ticks while the box touches it; a **fire block** 1 damage and 160 ticks; **burning** 1 each 20 ticks; water puts it out.
- **Invulnerability**: 20 ticks after a hit; in its second half only damage above the last hit lands, as the excess.
- **Hunger** (`FoodData.tick`): over 4 exhaustion takes a saturation point, else a food point (not on peaceful); saturation heals fast at full food, 18 food heals 1 each 80 ticks, starvation each 80 down to 1 on Normal. Exhaustion: jump 0.05, attack 0.1, hurt 0.1, a block broken 0.005, a hunger effect 0.005 × (amplifier + 1) a tick.
- **Eating**: `use_item` with a food held (when food is under 20, or the food can always be eaten) runs the item's ticks (`consume_seconds` × 20), then adds nutrition (max 20) and saturation (max the food), sends entity event 9, and leaves the `use_remainder` (a honey bottle's glass bottle) in the emptied hand, else in the inventory. Releasing the use (`player_action` 5) stops it.
- **Melee** (`attack`): damage = 1 + the weapon's modifier, × (0.2 + charge² × 0.8), the charge the ticks since the last swing over 20 / (4 + speed); reach 4 from the eyes to the mob's box (zombie-sized for every kind: `data` has no entity dimensions yet); the mob's own invulnerable window as the player's; death sends entity event 3 and removes it 20 ticks later.
- `set_health` goes out when health or food change or saturation turns to or from zero; air (metadata 1) and the burning bit (metadata 0) when they change.
- **The server's motion for the player** (`markHurt`, `ServerEntity.sendChanges`): the server integrates gravity on its own copy of the player's motion (one tick of it on the ground, `(y - 0.08) × 0.98` off it) and sends it as `set_entity_motion` at the end of a tick the player was hurt in, unless the damage is `no_impact` (drowning). A client takes it as its velocity.
- **Furnaces** (issue 13a): a block entity per placed furnace, its menu and its tick ([[smelting]]).
- **Mob loot** (issue 12c): a kill drops the mob's unconditioned loot (`data::generated::loot::mob_drops_of`), each count rolled in its range, at its feet with `spawnAtLocation`'s push and a 10-tick pickup delay; a parity row's mob is summoned without loot.
- **Fire resistance** (a moment's effect): fire, lava and burning do no damage; lava still slows and lights the body.
- `Sim::summon` places a mob (health from `data::generated::mobs`) for a test or a moment; mobs do not move, fight or drop yet (12c/12d).

## Generated worlds and lockstep (08b)
- `sim::terrain::world(seed, radius)`: seeded rolling ground (value noise), one-block steps, still water to y 62 over sand, carpets, cobblestone pillars, encoded as real chunk columns; a pure function of its seed.
- `bot::lockstep::run`: the bot's world and the model folded together in one thread with no clock: each tick the model ticks, the world ticks, and every packet is delivered both ways before the next. A run is a pure function of world, seed and code. `run_acting` lets the controller act too (digs, clicks); `Sim::seeded` gives the model's rolls the run's seed.

## Evidence
- **1,000 generated worlds** (`the_bot_walks_a_thousand_generated_worlds_clean`, CI's `worlds` job, release): every run placed, no correction, no violation, no decode error, no kick, no death; walked median 31 blocks in 400 ticks. A run repeats exactly from its seed. The first run found 1 world of 1,000 where the server corrected the bot 31 times at a water bank: a bot physics bug, fixed in [[physics]] (vanilla's 1e-7 collision tolerance).
- Model tests: a step accepted; 20 blocks in one move too quick; walking into a wall and sinking into the ground corrected; a 10-block fall hurts 7; into water it does not; a long fall kills with `death.attack.fall`.
- `the_bot_joins_the_model_and_walks`: the bot joins a flat model world, is placed on the grass, walks its staircase past 5 blocks, runs to its 100-tick cap with no correction, no decode error, and its recording replays with 0 mismatches (done-condition 2's first half).

## Limits
- Capture, simrace and the parity table: [[parity-table]]. Mob movement, mob attacks, drops and knockback on the player come with 12c and 12d.
- Drop rolls are the model's own seeded randomness, never vanilla's `RandomSource`: a chance drop (a sapling, a seed) is not an outcome the table can compare.
- Item collision is per cell (no partial shapes sideways). Seagrass and kelp, which turn to water, are not popped. Item merging is not modelled.
- The server-side move uses the bot's own collision (`land_tick`), not vanilla's `Entity.collide` order (y, then the larger of x and z first): the parity table is what says whether it matters.
- `chunk_batch_start` goes out as its id with an empty body; the bot counts it unknown.
- `ModelWorld::ground` and `placeable` place a player as vanilla does (under leaves, two free cells): found by issue 09's measure ([[planner]]).
- Peaceful regeneration (`ServerPlayer.tickRegeneration`, 1 every 20 ticks below full) is modelled when `Config::peaceful`; found by the parity table.

## The Nether (issue 16, read from the 26.1.2 jar with javap)
- `sim::nether` (`crates/sim/src/nether.rs`): `Stage` lays a world in `minecraft:the_nether` (a netherrack floor and ceiling, bedrock at y 0 and 127, 16 sections from y 0) with lava lakes, soul sand, soul soil, magma, gravel and nether bricks, and `build`s it; `nether_world` is the empty cavern. The login carries the dimension type's index in the synced registry (sorted by name: 3). `walls` holds the Nether crossing's walls as stages; `Sim::setblock` sets a block with its updates (lava put so runs, a lake laid in a stage rests until its bank changes).
- Lava reads the dimension (`ModelWorld::fast_lava`: the type's `gameplay/fast_lava` attribute, the Nether alone): `LavaFluid.getTickDelay` 10 against 30, `getDropOff` 1 against 2 (7 cells from a source against 3), `getSlopeFindDistance` 4 against 2. Fluid ticks run only in chunks within the simulation distance of the player and wait elsewhere; `Sim::wake_fluids` schedules every fluid cell that flows or has room to (a captured world's lava falls, still in the columns as sent).
- Magma (`Entity.applyEffectsFromBlocks`, `MagmaBlock.stepOn`): on the ground with a magma block 0.2 under the feet and not sneaking, 1 (`hot_floor`) a tick, so 1 each 10 ticks through the invulnerability; fire resistance refuses it (`is_fire`). The model takes `player_input` and keeps the shift key as `Life::sneaking` (`handlePlayerInput`: `setShiftKeyDown`), which the bot's world sends when its keys change ([[physics]]).
- The fight (`life.rs`): a placed mob may hold a `Brain`: `Melee` (a wither skeleton walking straight in, 8 a hit with the wither effect), `Blaze` (`BlazeAttackGoal`: a charge of 60 ticks, three small fireballs 6 apart, a rest of 100; fireballs as entities, 5 and alight on a hit, fire left on a block hit); `Life::armor` cuts a mob's hit as `CombatRules` does; a shield up 5 ticks stops fireballs from the side faced. Arenas: `walls::fortress_bridge`, `fire_beside`, `shelter_wall`; tests `crates/bot/tests/nether_fight.rs`.
- The blaze stage (`life.rs`): a spawner runs by `BaseSpawner`'s rules (`Sim::add_spawner`); a blaze rises and sinks to the player's eye plus a drawn offset, moves sideways as `Blaze.registerGoals` says (at a seen target within 2, at one lost under 5 ticks, a stroll with no target), touches for 6 within 2, and drops a rod to a player's kill. Knockback runs both ways (`LivingEntity.knockback`, 0.4): a mob's hit sends the player's motion; the player's fresh hit knocks a mob that acts 0.4 away, aloft 11 ticks at the air's drag (about 3 blocks for a walker, as clojurecraft measured; about 4.4 for a blaze in the air). `Life::hurt_by` sums damage by type. A fire set with updates ages by `FireBlock.tick` and goes out off netherrack and magma in about 25 s. The whole stage on the real fortress fixture: `crates/bot/tests/nether_rods.rs` (single runs and eight-seed reports, all `--ignored`).
- The fight's fidelity (probe 45359635431, javap of the 26.1.2 jar): the shield is `Items.SHIELD`'s `BlocksAttacks` (0.25 s delay, 90 degrees, item damage `floor(1 + damage)` from 3 up, 336 uses, bypassed by `#bypasses_shield`): it stops a mob's blow as well as a fireball (judged from the fireball's own position), wears and breaks, its wear sent as the slot's `damage`; a blocked blow knocks the mob 0.5 (`blockedByItem`); an axe's blow disables it for 100 ticks (`Melee::disables_shield`). A blaze's touch sets nothing alight (`Mob.doHurtTarget`). A hit that lands nothing leaves no fire and no wither. Armour cuts `lava`, `in_fire`, `hot_floor`. A knock lifts `min(0.4, vy / 2 + 0.4)`; a knocked walker goes over an edge and takes `ceil(fallen - 3)` (`tick_falls`). Infiniburn is the jar's `#infiniburn_nether` (netherrack, magma_block); fires the world began with age too (`Sim::wake_fires`).
- Not modelled: falling gravel, fire from lava, natural mob spawning other than endermen in warped forests (issue 17), mob pathfinding.
- Tests: `crates/sim/tests/nether.rs`; the walls closed loop in `crates/bot/tests/nether_walls.rs`, the pathfinder alone on them in `crates/plan/tests/nether_routes.rs` (docs/problems/nether-testbed.md).

## Endermen, ticking and Nether biomes (issue 17, read from the 26.1.2 jar with javap)
`crates/sim/src/enderman.rs` (probe 45472254333; docs/problems/endermen-model.md):
- **Ticking** (`Sim::ticking`): a mob ticks only with its chunk within the simulation distance of the player's, Chebyshev (`DistanceManager.getPlayerTicketLevel` = 31 - distance, entity ticking at level 31 or less; `ServerLevel.tick` returns early for a non-player entity out of `inEntityTickingRange`, and `ChunkMap` sends its changes only there). Every mob brain (walkers, blazes, knocks, falls, endermen) is frozen beyond it: no move, no attack, no teleport, no position update.
- **Endermen** (`Brain::Enderman`, `Sim::summon_enderman`): 0.6 by 2.9, eye 2.55, 40 health, attack 7, speed 0.3 (+0.15 with a target), follow range 64; metadata 16 carried block, 17 creepy (has a target), 18 stared at. The stare (`Sim::stares_at`, `isBeingStaredBy`): no carved pumpkin, `dot(view, to its eye) > 1 - 0.025 / distance` and a collider line of sight eye to eye. The look goal starts on an even tick of its age, sets 18, counts 3 goal ticks while the stare holds, then targets the player (17). Targeted: stared within 4 teleports at random (64 box); farther than 16, after 15 goal ticks (30 server ticks) teleports toward; frozen while stared within 16; walks straight at the player and hits for 7 within its box grown 0.83. A player's hit angers it and never teleports it. Untargeted it strolls (1 in 120). Teleport landings go down to a block that blocks motion, not water, with the body free of blocks and liquid.
- **Natural spawning** (`NaturalSpawner`): each tick, each entity-ticking chunk whose middle is within 128: a place up to the top block + 1, three packs, a pack's kind drawn by weight from the biome's monster list at its first try the right distance away (warped forest: enderman alone; soul sand valley: skeleton 20, ghast 50, enderman 1; nether wastes: ghast 50, zombified piglin 100, magma cube 2, enderman 1, piglin 15), its group size then its tries, each moved `nextInt(6) - nextInt(6)`; a try spawns an enderman where the player is 24 to 128 away, the body fits over a floor and the biome's spawn cost allows; the call stops at 4; the monster cap 70. Only endermen are spawned, and only in a model built `with_natural_spawns` (off by default, so earlier issues' worlds keep their mobs). A natural one despawns past 128. No light rule (the model holds none), no random despawn.
- **Biomes** (`nether::Biome`, `Stage::biome`): a stage's sections carry `nether_wastes` by default and any of the five Nether biomes per 4x4 column, indexed in the synced (name-sorted) biome registry; `ModelWorld::biome_at` reads them back.
- Tests: `crates/sim/tests/endermen.rs`, closed loop `crates/bot/tests/endermen_model.rs`.

## The pearl hunt (issue 18)
- `Sim::place_item(stack, pos)` lays an item on the ground before the join, at rest with no pickup delay (a parity row's `items`: a pearl at the burrow's mouth); the join announces every item on the ground (`items_added`).
- **The chase paths** (`MeleeAttackGoal`, `PathFinder`): an angry enderman walks a breadth-first search over walk nodes for a mob 2.9 high (three cells clear over a floor; a step of one up, a drop of up to three; corners only past two open sides; at most 1,024 nodes) to the node nearest the player's cell, ties to the first found: a partial path when the player is in a burrow too low for it, so it stops at the mouth. The path is found again when the player's cell changes, when the next cell is reached, or every 10 ticks. Its blow needs a line of sight eye to eye (`canPerformAttack`), sampled as `sight` samples it: the real server hit the bot in M under the roof in 3 of 5 runs, and an exact line never did, so the model is never kinder than the game there.
- **A knock fits the whole 0.6 by 2.9 box** (`tick_pushes`): a hit never pushes an enderman into a two-high space.
- **Its blow's damage event names it** as cause and direct (other mobs' still send none, so issue 16's fights are unchanged); `Life::hurt_by_mob` counts the blows that landed by kind.
- Loot: a summoned enderman keeps the generated table (an ender pearl, 0 or 1).
- **Random despawn** (`Mob.checkDespawn`): a natural enderman idle over 600 ticks and past 32 despawns 1 in 800 a tick (`Life::ender_despawns`); within 32 its idle count starts again.
- **Tool wear**: a sword wears 1 a landed hit (another tool 2), a tool 1 a block that takes time (a sword 2), as the slot's damage component (`Life::tool_wear`, by slot; it moves with a number-key swap, click mode 2, now modelled); at its max damage the tool breaks.
- Tests: `crates/sim/tests/pearls.rs`, closed loop `crates/bot/tests/pearls.rs`; the rows and walls in docs/problems/pearls-model.md.

## Eyes of ender (issue 19, read from the 26.1.2 jar with javap)
`crates/sim/src/eye.rs` (probe 45685968956; docs/problems/eyes-model.md):
- **The throw** (`EnderEyeItem.use`): a look whose block hit is an end portal frame throws nothing. The target is the overworld's stronghold point the model was told (`Sim::with_stronghold`, from RCON's `locate`: the model has no structure generator); none, or another dimension, throws nothing and keeps the eye. Else one eye is spent and an `EyeOfEnder` (entity data 8 its item) is added at the player's middle (feet + 0.9), its delta zero.
- **The flight** (`signalTo`, `tick`, `updateDeltaMovement`; the pure `eye_step`): beyond 12 on the plane its target is 12 along the line and 8 up, within 12 the point itself; each tick it moves by the old delta with no collision, then its horizontal speed lerps 0.0025 of the way to the distance left along the line to the target (both speeds times 0.8 within 1), its vertical speed 0.015 of the way to +1 below the target's y or -1 above. Every place lies on the one line from the spawn to the target. Its place is told every 4 ticks (the entity type's update interval, `EntityPositionSync`).
- **The end**: at life 81 it is removed; 4 in 5 (`nextInt(5) > 0`, the model's seeded draw) an ender eye item where it was, kicked up to 0.1 sideways and 0.2 up with no pickup delay; else level event 2003 (the shatter). Counted in `eyes_thrown`, `eyes_dropped`, `eyes_shattered`.
- **A synthetic stronghold** (`crates/sim/src/stronghold.rs`, `portal_room`, `stronghold_world`): the portal room laid as `StrongholdPieces$PortalRoom` has it (frames at y 3 round the centre, the pool under the ring, side lava, stairs, the silverfish spawner, iron bars, the door) with a corridor north of the door, buried in flat rock; brick kinds and infested bricks (5 in 100) on a fixed hash, not the selector's draw. For Q4's closed-loop tests. Silverfish are not modelled yet. Test `crates/sim/tests/stronghold.rs`.
- Tests: `crates/sim/tests/eyes.rs` (the exact line, 12 out; near; 60 throws spend one eye each and end at 81, about 4 in 5 dropped; no stronghold throws nothing), the closed loop `crates/bot/tests/eyes.rs` (the bot's read line within 0.1 degree, a dropped eye fetched).

## The End (issue 20h, docs/problems/end-model.md)
- `crates/sim/src/end.rs`: `EndStage` lays a world in `minecraft:the_end` (void, its own dimension type and biome): the arrival platform (`EndPlatformFeature`, obsidian 5 by 5 at (100, 48, 0), the player at (100.5, 49, 0.5)), an end stone island disc, spikes as `SpikeFeature` lays them (obsidian under the height within r² + 1, bedrock on top, a guarded one's iron bar cage joined as worldgen joins it) and a simplified fountain. `Layout::small()` is a test's End: a 20-block void gap, one open spike and one caged.
- End crystals: `Sim::summon_crystal`; a strike within reach of the 2 by 2 box removes one and it explodes with power 6 (`EndCrystal.hurtServer`). The blast's damage to the player is `ServerExplosion`'s: exposure from rays over the player's box to the centre (`seen_percent`, marched against the cells' shapes, iron bars by their post and arms), impact (1 - d) × seen, damage (impact² + impact) / 2 × 7 × 12 + 1 cut by armour; an `explode` packet with the knockback. Blocks the blast breaks are not modelled.
- The dragon (`end::Dragon`, `Sim::summon_dragon`): scripted, not the AI. It sits on the fountain's pillar (feet at floor + 5) facing a yaw, sitting scanning (phase 6, metadata 16; health at 9; the yaw by a `TeleportEntity` the model writes itself), its parts' ids its id + 1 to + 8. A hit on the head (part 1) takes the swing, on the neck, body or its own box the swing / 4 + min(swing, 1), each in reach of that part's box. The head's and neck's boxes grown by 1 blow a player for 10 while the dragon was not hurt in 10 ticks. A quarter of 200 taken sitting ends the perch: it flies 400 ticks (40 up, 40 out) and sits again. At 0 it dies (phase 9, entity event 3); 200 ticks later it is removed and the exit portal fills the basin within 2.5 at floor + 1. Feet in a portal cell get `win_game` (game event 4, 1.0) once, and `client_command` 0 after it is counted. Not modelled: its flight paths, breath, wings, charges, the crystals' heal, and the overworld the respawn goes to.
- Tests: `crates/sim/tests/end.rs` (the layout cell by cell), `crates/bot/tests/end_strike.rs` (the strike through a wall at 2 off, and from an open stand).

## Issue 14a
- Fluids, buckets along the look, obsidian and portal lighting (`crates/sim/src/fluid.rs`), and the player's look kept from each move with rotation ([[buckets-and-fluids]]).

## See also
- [[cc-sim]] - clojurecraft's model, the original.
- [[body-boundary]] - the seam the model plugs into.
