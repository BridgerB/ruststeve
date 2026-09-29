# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An Ender-Dragon speedrun bot — a **Rust port of `steve`** (the TypeScript speedrun bot), with a built-in Minecraft SDK (the `bot`/`world`/`protocol`/… modules, formerly the separate `rustcraft` crate, now flattened into this one crate). It ports the **single-bot core of steve's `main.ts`**; steve's multi-bot/MCP orchestration is out of scope here.

A single binary: connect → wait for spawn + chunks → run a tick loop until the dragon is dead.

## Commands

```bash
cargo run                    # connect and run the speedrun loop (app::run)
cargo build --release        # the harness scripts run the release binary
cargo test                   # 172 SDK tests + the bot, all one crate now
cargo test <name>            # run one test by (sub)string, e.g. `cargo test nbt`
cargo run --bin datagen      # regenerate data/ (registry) from the Mojang jars

# Config via env vars (main.rs / app.rs read these):
MC_HOST=<host> MC_PORT=25565 MC_USERNAME=ruststeve-001 cargo run
STEVE_DATA=data              # registry dir (default shown)
MC_TP="x y z"                # teleport to a real forest on spawn (needs op); steve's spawnBot equivalent
RACE_HOLD=<secs>             # spawn, then idle alive so the orchestrator can position the bot
RACE_GOAL=nether|blaze|iron_pickaxe|…   # stop as soon as that goal is reached (race finish line)
STEVE_TEST=<step_id> STEVE_TEST_SECS=<n>   # isolation mode: run ONLY that step until is_complete
GYM=<slug>|report GYM_TRIALS=<n>  # in-Rust gym: loop a sub-task on random terrain → data/gym.db
ARENA_TEST=1                 # combat rig: treat fortress as found + run find_fortress/kill_blaze in the overworld
FORTRESS_HINT="x y z"        # pathfind toward a known fortress origin before wandering (real-nether combat)
NETHER_START="x y z"         # command-tp the bot straight into the nether on spawn (skip the portal build)
SNIFF=1 | SNIFF=a,b,c        # capture packets → data/sniff-<user>.db (all, or names containing a/b/c)
CRAFT_DEBUG=1                # dump inventory/window state around each craft (also enables CAST debug)
MINE_DEBUG=1                 # log descent/dig-down decisions
# other targeted flags: GOTO_DEBUG, DIG_DEBUG, CHUNK_DEBUG, PKT_DEBUG, FOLLOW_DEBUG, GM_DEBUG, PORTAL_BUDGET_SECS
```

**Development is driven by two live-server harnesses, not just `cargo run`** (see "Race & test harness" below) — most changes are validated by running a step in isolation or a full race against the shared MC box at `144.24.32.76`.

**The registry must exist.** `STEVE_DATA` points at the generated `data/` (run `cargo run --bin datagen` first — it extracts blocks/items/recipes straight from the Mojang jars). Without it the bot starts with an empty registry and resolves no block/item names — it will spawn but can't do anything useful. `data/` is gitignored (regenerable); `src/protocol/data/protocol-schema.json` is NOT — it's `include_str!`'d at compile time, so it must stay committed.

This is offline-mode only (no auth wired up): `ClientOptions { access_token: None, uuid: None }`. The server must allow offline players, and `MC_TP` / RCON ops need the bot to be op'd.

## Architecture

The loop in `main.rs` is **sync state → pick the next incomplete step → execute it**, repeated. Three files form the engine; `tasks/` does the real work.

**`state.rs` — perception.** `sync_from_bot(&Bot)` scans `bot.inventory` and folds it into a flat `GameState` (counts of logs/planks/sticks/cobble/coal/iron/diamonds/food/buckets/flint, best pickaxe & sword `Tier`, has-table/has-furnace, plus health/food/position/dimension). The step machine only ever reads `GameState`, never the bot directly.

**`steps.rs` — the plan.** A static ordered `&[Step]` walking the tech tree (gather wood → planks → table → sticks → wooden pick → mine stone → stone pick/sword → furnace → coal → iron → smelt → iron pick → buckets → water → food → flint & steel). Each `Step` is gated by `can_execute(&GameState) -> bool` and satisfied by `is_complete(&GameState) -> bool`; `execute_step` dispatches by string `id` into `tasks::`.

> **Critical invariant — `get_next_step` returns the FURTHEST-along runnable step (`.next_back()`), not the first.** Steps consume the (consumable) outputs of earlier steps, so an earlier step looks "incomplete" again the moment a later one runs. Picking the last runnable+incomplete step is what stops the bot from looping forever re-gathering. Preserve this when adding steps; keep the slice ordered by progression.

**`tasks/` — execution**, one module per family, each a faithful port of the matching steve task. They translate goals into the SDK's bot primitives (`bot.goto*`, `dig`/`dig_toward`, `place_block`, `click_window`, `look_at` + control states — all in the in-crate `bot`/`world`/… modules). Recurring patterns worth knowing before editing:

- **Pathfinder-first, raw-walk fallback.** Tasks try `bot.goto*` (the SDK's A*); when it stalls short, they fall back to manual control-state walking that carves through terrain. `gather_wood` is the fullest example: `approach_raw` carves stairs up a hillside, `chop` fells a trunk via block-prediction, `explore`/`downhill_angle` escape stone-locked basins, and unreachable trees get blacklisted. `mining` mirrors this with `dig_down`/`descend_step`/`strip_tunnel`.
- **Local state prediction.** The SDK's container-craft and item-use inventory sync is inherently racy (see "Minecraft SDK" below), so tasks predict known results with `bot.ensure_item(...)` / manual count edits after a server action succeeds but doesn't echo (e.g. `craft_item` in `bot_utils.rs`, `fill_water_buckets`). Without this the step machine re-runs the step forever.
- **Resource-finding is explore-or-remember, never give up.** A task that needs a resource not in immediate scan range must roam/descend to look (rotating headings) or reuse a remembered POI — never fail outright. `bucket` roams for water; `mine_ore` records exposed **lava** it passes (`find_exposed_blocks`) so `portal::prepare_cast_site` can navigate back to it for the obsidian cast instead of blind-searching (digging *down* toward lava is refused by `dig_down`'s death-avoidance, so the bot can't reach the lava layer that way).
- **Crafting-table acquisition.** `bot_utils::get_crafting_table` finds a nearby table, else crafts and places one — digging a side niche if the bot is boxed in (e.g. mid-tunnel). 2×2 recipes pass `table: None`; 3×3 recipes need a placed table.
- **Physics-jitter guards.** Vertical block math uses `floor(y - 0.5)` (not `floor(y) - 1`) so a position dipping just under an integer doesn't target the wrong block — see `dig_down`.

### Minecraft SDK (in-crate, ~11k lines)

The `bot`, `world`, `protocol`, `chunk`, `path`, `physics`, `registry`, `anvil`, `recipe`, `item`, `block`, `entity`, `nbt`, `window`, `chat`, `auth`, `rcon`, `vec3` modules ARE the SDK (a Rust port of the `typecraft` TS SDK). `src/lib.rs` is the crate root and re-exports them; the bot core (`app`/`state`/`steps`/`tasks`/…) sits alongside them and calls them via `crate::`. There is no separate crate anymore — `git grep 'rustcraft::'` should return nothing.

- **`bot::Bot`** is the handle: `Bot::connect(opts, &registry)` then an async event loop (`next_event`, `drive_tick`, `wait_ticks`). It owns `inventory`, `entity`, `world`, `movement`, `registry`, `current_window`, and exposes `goto*`/`dig`/`place_block`/`click_window`/`look_at`/`open_block`/`run_command`, etc.
- **Container-click sync is the single most load-bearing, hard-won mechanism** — read `bot/inventory.rs::click_window` before touching anything crafting-related. It sends the click with **empty `changedSlots`**, which forces the server to treat every click as a prediction mismatch and reply with a full authoritative `container_set_content`. This is deliberate: the optimistic local-prediction path desynced under sustained multi-bot load and cascaded into `missing crafting ingredient` / `result never appeared` failures that capped every race at iron ore. Forcing a server resync after each click keeps the window authoritative. `craft_item` (bot_utils) layers on top: retry-on-ingredient-not-found and poll-for-the-exact-result-item. Do **not** "optimize" click_window back to sending predicted changed slots.
- **Melee combat is a second load-bearing, protocol-version-specific mechanism** (`bot/mod.rs::attack`) — read it before touching anything combat-related. **Protocol 775 (MC 26.1.2) split attacking into a dedicated `attack` packet (id 0x01, just `{entityId: varint}`); the old `interact` packet (0x1a) is now RIGHT-CLICK ONLY** and its 775 form even requires a trailing `hand` field. The whole "melee deals zero damage" wall was sending an interact-with-mouse=1 (a no-op right-click that decoded fine and dealt nothing). Do NOT revert `attack` to send `interact`. Two more hard-won details live in that function and `tasks/nether.rs::kill_blaze`: (1) it force-sends `send_position()` *before* the attack so the server evaluates reach/angle against the current pose, not a stale one; (2) swings are paced with **`wait_real_ms(650)`, never `wait_ticks`** — under the combat packet flood `wait_ticks` collapses to ~0ms (buffered packets satisfy the tick counter), bursting every swing into the mob's 10-tick hurt-invulnerability so only the first lands. 650ms of *wall-clock* clears both the i-frames and the iron-sword cooldown. `packet_attack`'s field lives in `src/protocol/data/protocol-schema.json` (authoritative source: minecraft-data `pc/26.1` — there is no `26.1.2` dir).
- **`WorldMemory`** (`memory.rs`) is a per-bot SQLite POI store (`.memory-<username>.db`): `record`/`observe`/`nearest`/`mark` remember ore/log/water/lava/table/descent locations across ticks and deaths, plus an event log. Passive block sightings during mining feed it (`observe_blocks`), so the bot never X-rays — it only remembers exposed blocks it actually saw.

### State of the port

The full **overworld→nether pipeline** is ported (21 steps): gather wood → planks → table → sticks → wooden pick → mine stone → stone pick/sword → furnace → coal → iron → smelt → iron pick → buckets → water → flint & steel → gather build blocks → build nether portal (obsidian cast) → enter nether → **find fortress → kill a blaze**. The portal is built by casting obsidian (lava cup + water bowl, no diamond pickaxe) and lit with flint & steel.

The first **post-nether step pair** (`tasks/nether.rs`) is ported and working: `find_fortress` sweeps ±X (fortresses X-align) — bridging cobble over lava, bailing off lava underfoot — until nether brick is in a direct block scan (`see_fortress`, NOT `find_block`, which needs LOS to an exposed block and returns None for the floor you stand on), then pathfinds onto it; `kill_blaze` equips the best sword, camps the spawner, and melees the nearest reachable blaze (≤12 blocks) on the 650ms cadence until it holds a blaze rod (`RACE_GOAL=blaze`). **Verified end-to-end in the overworld `ARENA_TEST` rig** ("RACE GOAL REACHED: blaze"), not yet exercised in a real fortress (that needs the cross-dim chunk-load delay solved, or a real portal entry, plus fortress-finding traversal). **This work is currently uncommitted** (last commit is the portal); `src/tasks/nether.rs`, `arena-run.sh`, `nether-run.sh` are untracked.

Beyond the blaze rod, the run stops: `is_dragon_dead` always returns false (`WorldState::dragon_dead` is never set), so the rest of the Nether (more rods, ender pearls) and the whole End *combat* phase — the victory path — are **not yet implemented**. Extending the run means adding ordered steps + task modules following the patterns above.

## Race & test harness

Development happens against a **shared, always-on MC server at `144.24.32.76`** (RCON on 25575), driven by helper scripts that build the release binary and orchestrate bots over SSH. Two ways to validate a change:

- **`./isolation-test.sh <step_id> [N] [SECS]`** — the fast loop. Spawns N bots in clean stone arenas, gives each the step's prerequisites (see `setup_prereqs()`), and runs the binary in **`STEVE_TEST` mode** (main.rs: run only that one step until its `is_complete` passes → `TEST RESULT: PASS`, else `FAIL` on timeout). Add a `setup_prereqs()` case to support a new step. This is how portal/mining/craft fixes are verified without a full run.
- **`./race.sh`** — the full loop. `N` bots (default lanes at `x≈680`, `z=350+90·i`) race spawn→…→nether; `RACE_SECONDS` bounds it, `RACE_GOAL=nether` is the finish line. `gen_dashboard.sh` renders live standings to `/tmp/race.html`; `stop-race.sh` ends a run cleanly (kills only `target/release/ruststeve` + kicks the `rust-race-*` names — never touches other processes). `race-loop.sh` respawns dead bots mid-race. Per-bot logs are `race-<i>.log`.
- **`GYM=<slug> GYM_TRIALS=<n> cargo run`** — the *in-Rust* gym (`src/gym.rs`, no shell): one connected bot self-RCONs `clear`/`give`/`spreadplayers` and reruns a single sub-task across random real terrain, recording pass/duration/xyz to `data/gym.db` (`GYM=report` prints the per-slug pass table). Use it to harden a terrain-sensitive step in isolation before chaining it into a race. RCON reaches the shared box over the SSH tunnel (`localhost:25575`).
- **Combat rigs:** `./arena-run.sh` (overworld sealed netherrack arena + `ARENA_TEST=1`, fast) and `./nether-run.sh` (kit + tp into the real nether, blaze spawner via RCON) drive the `find_fortress`/`kill_blaze` steps. `cast-one.sh` / `gym-portal-run.sh` are the portal rigs. Bots always run on this MacBook; the *world* is the OCI box — bots reach it via the SSH tunnel (`tunnel-keep.sh` forwards 25565/25575), so scripts connect to `MC_HOST=localhost`.

**Packet sniffer (the protocol-debug tool):** `SNIFF=1` (or `SNIFF=name,substrings`) captures every packet, both directions, with ms timestamps into `data/sniff-<user>.db` (`src/sniff.rs`), so a desync (late/reordered/absent server reply, or a mis-encoded outbound packet) can be queried after the fact with SQL. Combined with `PKT_DEBUG=1` (prints packets to stderr, including the server's `disconnect` reason before EOF) this is how the 775 attack-packet bug was found — reach for it before guessing at any interaction-reliability failure.

**Shared-server rules (this box is shared with the TypeScript `steve` bot):**
- **NEVER wipe/reset the world or restart the server** — automatically or manually. That is the operator's call only. The harness must not `forceload remove all` (it nukes the other project's chunks) or reset the world; scope forceloads to this project's region.
- Gamerules on 26.x are **snake_case** (`keep_inventory`, not `keepInventory`).
- Keep bot names in the `rust-race-*` / `ruststeve-*` namespace and process-kills scoped to `target/release/ruststeve`, so races can run concurrently with the other agent's Node bots.

## Code style

**Descriptive names — no cryptic abbreviations.** Name things for what they are.
Recent renames that show the bar (prefer the right-hand side):

- `cdbg` → `cast_debug`
- `bkt` → `filled_bucket_name`
- `is_obs` → `is_obsidian_at`
- `off_now` → `dist_from_stand`

Short conventional loop/coordinate names (`dx`/`dy`/`dz`, `bx`/`by`/`bz` for a frame
anchor, `i`) are fine; helper functions, locals that carry meaning, and closures
should read like prose.
