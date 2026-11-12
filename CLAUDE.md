# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An autonomous Ender-Dragon speedrun bot (random seed, any%, no human input) — a Rust port of the TypeScript `steve` bot — with its own Minecraft SDK flattened into the same crate (formerly the `rustcraft` crate; `git grep 'rustcraft::'` returns nothing). One crate, three binaries: `ruststeve` (the bot), `datagen` (registry extractor), `rcon` (tiny RCON CLI). Targets **MC 26.1.2, protocol 775**, offline mode only (no auth; bots must be op'd for RCON/`MC_TP`).

Reliability and generality over speed. **No X-ray:** the bot may act only on blocks it has seen with an exposed face. This is enforced in code (see "Honest perception" below), not just policy.

Work proceeds in **cycles** set by a strategy prompt (`docs/ruststeve-loop-cycle4.md` holds the standing rules; each cycle has a `docs/playbook-<date>.md`, `docs/status-cycle<N>-h<H>.md` every 8 h, and a `docs/report-<date>.md` wrap-up). `CHANGES.md` is the running changelog per cycle. Read the latest report's "state for the next cycle" block before starting work. `LOOP.md` is the cycle-1 procedure and is historical.

## Commands

```bash
cargo build --release              # harnesses run target/release/ruststeve (or a copied bins/<name>/ruststeve)
cargo test --release --lib         # ~190 unit tests (SDK + bot); one crate
cargo test --release --lib <name>  # one test by substring, e.g. `cargo test --release --lib recorded_craft`
cargo run --bin datagen            # regenerate data/ (registry) from the Mojang jars — required before any run
cargo run --release --bin rcon -- "list"   # RCON_HOST/RCON_PORT/RCON_PASS (defaults localhost:25576, minecraft-test-rcon)
```

There is no CI (`.github/` does not exist) and no lint config; `cargo build` warnings are kept at zero by convention.

**The registry must exist.** `STEVE_DATA` (default `data`) points at the generated registry (`blocks.json`, `items.json`, `recipes-raw/`, `tags/`, `blockCollisionShapes.json`, …). `data/` is gitignored and also holds run outputs (gym.db, race.db, attempts/, params.json, landings). `src/protocol/data/protocol-schema.json` is the exception: it is `include_str!`'d and committed. `src/registry/mod.rs` patches generated data at load time (portal blocks get no collision — the generated full cube was wrong).

### Run modes (all selected by env vars read in `src/app.rs`)

```bash
MC_HOST=<host> MC_PORT=<port> MC_USERNAME=rust-gym-001 target/release/ruststeve   # plain run: the step machine until death/goal
GYM=<slug> GYM_TRIALS=<n> …        # in-Rust gym: one bot loops a sub-task (src/gym.rs) and records to GYM_DB
GYM=report                         # print the per-slug pass table and exit (no bot)
STEVE_TEST=<step_id> STEVE_TEST_SECS=<n>   # isolation: run only that step until is_complete → "TEST RESULT: PASS|FAIL"
RACE_GOAL=nether|blaze|iron_pickaxe|… RACE_HOLD=<secs>   # race path: telemetry to data/race.db, win recorded once, bot keeps running
```

Other env worth knowing: `RACE_ID`, `BUILD`, `WORLD_SEED` (stamped on every event row); `BOT_ARMS=1` + `ARM_BUCKETS`/`ARM_CLUSTER_MIN` (per-bot bandit arms); `GYM_REGION`, `GYM_DB`, `ATTEMPTS_DIR`, `GYM_LANDINGS` + `GYM_LANDING_OFFSET` (paired landings); `MC_TP`, `NETHER_START`, `FORTRESS_HINT`, `ARENA_TEST`, `RACE_MAX_PRIORITY`, `RACE_STOP_ON_GOAL`, `RUST_VIEW` (SSE world feed for the 3D viewer); debug: `SNIFF`, `SNIFF_JSON=1`, `CRAFT_DEBUG`, `CAST_SNIFF`, `MINE_DEBUG`, `GOTO_DEBUG`, `DIG_DEBUG`, `CHUNK_DEBUG`, `PKT_DEBUG`, `FOLLOW_DEBUG`, `GM_DEBUG`. Full list: `grep -rho 'var("[A-Z_]*")' src | sort -u`.

### Harnesses (the current ones)

- **`local-server.sh [1|2]`** — a local vanilla 26.1.2 gym server (sha1-checked piston-data jar, Java 25 from nixpkgs, difficulty normal, view 6 / sim 4, 6 GB): profile `local-1` = game 25567 / RCON 25577, world `data/local-world`; `local-2` = 25568 / 25578. Runs in the foreground; stop with `rcon stop`. The local world is ours to reset (stop the server, move `data/local-world/world` aside).
- **`gym.sh`** — `PROFILE=local-1|local-2|box-b SLUG=<slug> | SLUGS=a,b,c  N=<bots> TRIALS=<n> BIN=… BUILD=… [LANDINGS=data/landings-<set>.json] ./gym.sh`. Fresh region per batch (`GYM_REGION`, persisted in `<store>/gym-index`), SIGTERMs the previous batch on the same `PIDTAG` (its trials are recorded `aborted`, never left `running`), clears forceloads in all three dimensions unless `KEEP_FORCELOADS=1`, and **refuses more than 6 live bots per server** across all pid files of a profile. Local stores live in `data/local-1/` (gym.db, attempts/). `gym-tonether-b.sh` is a thin `PROFILE=box-b` wrapper.
- **`race-b.sh`** — a 5-bot, 240-min race: fresh region (`x = 900 + RACE_INDEX*3000`), land-checked lane placement (`scripts/race-place.ts`), memory/frame/lit-portal files cleared per race, bandit arms drawn per bot, a heartbeat watchdog that relaunches stalled bots, a respawn check, and the previous `race.db` archived to `data/archive/race-pre-<id>.db`. Host-agnostic via `MC_HOST`/`PORT`/`RCON_PORT` (defaults: Server B, 25566/25576). Example: `BIN=… BUILD=… RACE_INDEX=<n> N=5 setsid ./race-b.sh > race-i<n>.out 2>&1 < /dev/null &`.
- **Legacy, cycle-1 era (hard-coded to the old shared box, sudo `mcrcon`, port 25575):** `race.sh`, `race-loop.sh`, `stop-race.sh`, `isolation-test.sh`, `arena-run.sh`, `nether-run.sh`, `cast-one.sh`, `portal-test.sh`, `lava-find-test.sh`, `gym-portal-run.sh`, `gym-reach-b.sh`, `loop-watchdog.sh`, `tunnel-keep.sh`, `gen_dashboard.sh`. Read before reusing; most need porting to `gym.sh`-style profiles.

**Where the servers are (checked 2026-11-11):** the OCI box `bridger@144.24.32.76` no longer runs any Minecraft server (Server A/B and their ports are gone; it hosts Factorio now). `race-b.sh` therefore has no default target until a server is provisioned; point it at one with `MC_HOST`/`PORT`/`RCON_PORT`. Never touch another project's server, and never wipe a world you don't own.

### Analysis scripts (TypeScript, run directly with `node`, no build)

`scripts/ml/compare.ts` (Bayesian P(B>A) with a sequential stop rule; `--paired <gym.db> <slug> <A> <B> [--exclude ids]` for paired landings: per-landing table, bootstrap of the paired mean, sign test), `race-funnel.ts <race.db…>` (per-bot furthest milestone, first Nether tick, deaths, tick gaps; funnel through the post-portal steps), `race-budget.ts label=<race.db>…` (bot-hours past budget by step), `funnel.ts`, `bandit.ts`, `rows.ts` (reads `data/attempts/*.jsonl`). Plus `scripts/landings.ts` (generate a pre-checked paired landing set by RCON), `scripts/mspt-sample.ts` (server tick sampling), `scripts/sniff-window.ts` / `sniff-fixture.ts` (read a `SNIFF_JSON` capture; cut a unit-test fixture), `scripts/rate-report.sh`, `scripts/gym-report.sh`.

## Architecture

### The engine: sync → pick → execute

`app::run` connects, waits for spawn and chunks, then loops: drain packets, **`state::sync_from_bot(&Bot)`** folds the bot into a flat `GameState` (inventory counts, best tool tiers, vitals, dimension, `portal_built`, `fortress_found`), **`survival::handle_survival`** runs reflexes first (breath, lava, health — the first one that fires wins the cycle), then **`steps::get_next_step`** picks a step and `steps::execute_step` dispatches by string id into `tasks::`. Each step attempt is wrapped by `learn::Attempt` (event row, budget, stall detector). A respawn-loop guard and a "20 failures with no progress" guard exit the process so the harness relaunches it fresh.

`steps.rs` is a static ordered `&[Step]` (28 steps: wood → tools → iron → buckets → flint → build blocks → **build_nether_portal → enter_nether → find_fortress → kill_blaze**, then end-game skeleton steps whose bodies are mostly unimplemented — `get_pearls` barters, the End steps return "not implemented"). Gym-only dispatch ids (`crystals`, `dragon`) also live in `execute_step`.

> **Critical invariant: `get_next_step` returns the FURTHEST runnable incomplete step (`.next_back()`), not the first.** Later steps consume earlier steps' outputs, so earlier steps look incomplete again; picking the last runnable one prevents re-gather loops. Keep the slice ordered by progression. Before that pick, overworld-only overrides run in order: pickaxe recovery, wood recovery, and the bounded early-iron-pickaxe rule. In the Nether only steps with priority ≥ 20 are eligible.

### Tasks (`src/tasks/`)

One module per capability family, ported from steve's tasks: `gather_wood`, `mining`, `craft`, `smelt`, `bucket`, `food`, `lava_move`, `portal` + `portal_mold`, `nether`, `end`. Shared helpers in `bot_utils.rs` (`craft_item`, `get_crafting_table`, `count_items`, water escape). Patterns that recur:

- **Pathfinder first, raw walk second.** `bot.goto*` (A*, **one synchronous search** — the sliced variant was deleted after losing a paired comparison), then control-state walking/tunnelling when the path stalls. `walk_to_xz` jumps; `settle_xz` is the sneaking, non-jumping precise version used beside lava.
- **Explore or remember, never give up.** Missing resources trigger roaming/descending with rotating headings, or navigation to a remembered POI (`memory::WorldMemory`, per-bot SQLite `.memory-<bot>.db`, cleared per race).
- **The portal is cast, not mined:** `portal.rs::prepare_cast_site` finds lava (descend to −50, tunnel; see Honest perception), the sealed scoop **station** (`portal_mold::station_refill`) fills buckets, and `portal_mold.rs` builds a template mold and pours lava cups + water to make obsidian, then lights it. The frame anchor persists per bot in `.frame-<bot>.txt`; the lit frame in `.portal-<bot>.txt` so `enter_nether` can walk back to its **own** portal without line of sight. `enter_nether` order: own lit portal → `find_block` (line of sight) → any exposed portal (the same test that sets `portal_built`).
- **Nether:** `find_fortress` digs down to brick seen below, tunnels toward it, or sweeps ±X (stepping out along ±Z if X is blocked both ways); bridging uses cobble, then netherrack. `kill_blaze` defends against nearby hostiles, then melees blazes on a wall-clock cadence.
- **Feet height:** standing on a block top reads y = 89.9999. Code that derives the feet row uses `floor(y + 0.01)` (Nether tunnel) or `floor(y - 0.5)` for the block below (`dig_down`); a bare `floor(y)` was the cause of a race-stopping bug.

### Honest perception (no X-ray, enforced)

- `Bot::is_exposed` counts air, water and see-through non-fluid blocks as a face — **not lava and not unloaded cells** (the registry marks lava `transparent`, which once made every source inside a buried lake "exposed").
- `Bot::lava_seen` remembers every lava source seen with an air face, refreshed from the loaded chunks every 5 s by the tick driver in every step; `lava_seen_cluster` picks the largest remembered cluster within 64 blocks. `find_lava_cluster` (a chunk-data read through rock) was deleted.
- `Bot::assert_lava_seen` **panics in gym mode** on any lava-site decision whose target was never seen exposed. Keep the assertion on any new lava-targeting path.
- Remaining danger-avoidance reads of unexposed lava (`raw_lava_near`, the shaft seal, `dig_down`'s refusal) are an open strategist question; don't add more.

### The SDK (in-crate)

`bot`, `world`, `protocol`, `chunk`, `path`, `physics`, `registry`, `anvil`, `recipe`, `item`, `block`, `entity`, `nbt`, `window`, `chat`, `auth`, `rcon`, `vec3` — a Rust port of the typecraft TS SDK, re-exported from `src/lib.rs`. `bot::Bot` is the handle (`Bot::connect`, `next_event`/`drive_tick`/`wait_ticks`, `goto*`, `dig`, `place_block`, `click_window`, `look_at`, `open_block`, `attack`).

Load-bearing mechanisms — read the code before touching them:

- **Window/inventory sync** (`bot/inventory.rs::click_window`, `bot/mod.rs` slot handlers, `window.rs::apply_server_slot/apply_server_content`). Clicks are sent with **empty `changedSlots`** so the server always answers with authoritative state. Server set-slot/set-content packets are applied **verbatim**, the player's 36 slots are mirrored between the player window and an open container, the server's cursor (`carriedItem`, and set-slot −1/−1) is applied, and each click's ack wait reads until the window has been quiet for 100 ms. Each of these fixed a measured craft failure; `tests/fixtures/stale-craft-window.json` (recorded with `SNIFF_JSON=1`) replays one. `crafting.rs::find_ingredient_slot` searches **only the inventory section**, never the grid.
- **Melee** (`bot/mod.rs::attack`): protocol 775 has a dedicated `attack` packet (0x01, `{entityId}`); `interact` is right-click only. Send position before attacking; pace swings with `wait_real_ms(650)`, never `wait_ticks` (which collapses under a packet flood).
- **Dimension change** clears loaded columns, entities and `lava_seen`, and resets world height (overworld −64/384, Nether/End 0/256).
- **Packet sniffer** (`src/sniff.rs`): `SNIFF=1` or `SNIFF=container,click,carried` captures both directions with ms timestamps into `data/sniff-<user>.db`; `SNIFF_JSON=1` stores full params as JSON for replay. Reach for it before guessing at any interaction failure.

### Measurement plumbing

- **Gym** (`src/gym.rs`): `GYM_STEPS` holds ~50 slugs, each a kit (`prereq` RCON `give`s), a `step_id` (or `pipeline` = the step machine), a timeout, a pass check and a `GymSetup` (random surface, paired landing, fixed surface, water pools/arenas, real lakes, tunnels, End dragon/crystals, Nether fortress/piglins/pocket/far, hidden or front-facing own portal). Late-game and iron slugs pass on **server truth** (RCON `clear <bot> <item> 0`, entity checks), never the client. `gym_runs` rows carry `build`, `landing` and `lava_s`; a trial killed by SIGTERM is written `aborted`. A death in a Nether setup ends the trial.
- **Event log** (`src/learn.rs`, schema `docs/events-schema.md`): one JSON row per step attempt and per guarded primitive, appended to `ATTEMPTS_DIR` (default `data/attempts/<run>-<bot>.jsonl`, one file per process), mirrored into race.db `attempts`. Rows carry build, seed, run id, bandit arms and an exposure-only `context`. Per-step budgets and stall windows (`budget_s`, `stall_s`, overridable in `data/params.json`) end attempts as `timeout` and bump an alternative counter.
- **Bandits:** `data/params.json` holds Beta arms (buckets, cluster_min, iron_band, retreat_hp, station_tiebreak), all `enabled: false`; races draw buckets × cluster_min per bot (`BOT_ARMS=1`).
- **Race telemetry** (`src/telemetry.rs`, race path only): `data/race.db` tables `ticks`, `events` (death, milestone, win, `race_meta`), `attempts`, `counters`; views `v_step_spans`, `v_top_fails`, `v_deaths`, `v_milestones`. A milestone names the step *reached*, so "Find Nether Fortress" means the bot is in the Nether; the ground truth for Nether entry is the first `ticks` row with a `the_nether` dimension.

### Other crates/dirs

`dashboard/` is a separate Leptos crate (own `target/`); `viewer/` + `src/viewer.rs` (`RUST_VIEW`) stream a bot's world to steve's Babylon viewer over SSE; `examples/` holds small SDK examples (`cargo run --example spawn|ping|craft|digtest|worldview`).

## Rules specific to this project

- **Measure every change against the old build** on the same slug before claiming it: build both binaries (a `git worktree` for the old arm is the usual way), run equal trial counts, report k/n with Wilson intervals; use paired landing sets for terrain-sensitive steps. A reproducing slug must fail on the old build before its pass rate means anything.
- **≤ 6 concurrent bots per server** (10 crashed a 2-CPU server's watchdog four times). Build niced (`nice -n 19`, `-j 1`) on a small shared box, and `cmp` a copied binary before launching it.
- **Never edit a racing binary's tree mid-race**; a race reads one fixed build (`BUILD` label on every row).
- **Never wipe or reset a world you don't own**; a local server's world is yours. MC 26.x gamerules are snake_case (`keep_inventory`).
- Every trial you cut must end as an `aborted` row with its reason; nothing is left `running` in gym.db.
- Commits: conventional prefixes (`feat`/`fix`/`refactor`/`docs`/`chore`), bodies explain the measured cause and result. No AI attribution anywhere. Never push to `main`.

## Code style

Descriptive names, no cryptic abbreviations (prefer `cast_debug` over `cdbg`, `filled_bucket_name` over `bkt`, `dist_from_stand` over `off_now`); `dx`/`dy`/`dz`, `bx`/`by`/`bz` (frame anchor) and `i` are fine. Comments explain **why** and usually cite the race/gym evidence that motivated the code (`race i11 rust-race-004 …`); keep that habit — it is how later readers know a guard is load-bearing. One-off scripts are TypeScript run with `node` (Node strips types natively), never Python.
