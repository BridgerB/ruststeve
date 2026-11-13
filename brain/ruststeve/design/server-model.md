---
title: The server model
description: How ruststeve plays without a Java server - the sim crate (the vanilla server as a pure reducer, grown per issue), S2c encoders that make the model's bytes a server's, the model as the bot's body behind the same channels a socket gives, and what it models so far.
type: reference
tags: [sim, model, server, body, closed-loop]
aliases: [air supply, drowning, fire, lava, hunger, eating, melee, Life, summon, sim crate, Sim, server model, survival, digging model, item entities, window 0 model, delayed destroy, model body, In, Wire, ModelWorld, flat_world, S2c encode, NotModelled, movement check, moved wrongly, moved too quickly, fall damage, lockstep, generated worlds, sim terrain, 1000 worlds]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@b86d924 clojurecraft@be85a92
sourceRefs:
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
- **What 08a models**: login (offline uuid), configuration (known packs, finish; the bot reads no registry), the play login with the race's view and simulation distances, chunks before the first teleport, health, chunk batches within the view distance and `forget_level_chunk` as the player moves, `set_time` every 20 ticks, a keep-alive every 300 ticks with vanilla's 15 s kick, and moves ignored until the teleport is accepted. What a real server would refuse is kept in `violations`.

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

## See also
- [[cc-sim]] - clojurecraft's model, the original.
- [[body-boundary]] - the seam the model plugs into.
