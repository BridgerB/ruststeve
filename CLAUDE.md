# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An Ender-Dragon speedrun bot — a **Rust port of `steve`** (the TypeScript speedrun bot), built on the `rustcraft` SDK (`../rustcraft`, a path dependency). It ports the **single-bot core of steve's `main.ts`**; steve's multi-bot/MCP orchestration is out of scope here.

A single binary: connect → wait for spawn + chunks → run a tick loop until the dragon is dead.

## Commands

```bash
cargo run                    # connect and run the speedrun loop
cargo build
cargo test                   # (rustcraft holds the test suite; this crate is thin)

# Config via env vars (see main.rs):
MC_HOST=<host> MC_PORT=25565 MC_USERNAME=ruststeve-001 cargo run
STEVE_DATA=../rustcraft/data            # registry dir (default shown)
MC_TP="x y z"                            # teleport to a real forest on spawn (needs op); steve's spawnBot equivalent
CRAFT_DEBUG=1                            # dump inventory/window state around each craft
MINE_DEBUG=1                            # log descent/dig-down decisions
```

**The registry must exist.** `STEVE_DATA` points at rustcraft's generated `data/` (run `cargo run --bin datagen` in `../rustcraft` first). Without it the bot starts with an empty registry and resolves no block/item names — it will spawn but can't do anything useful.

This is offline-mode only (no auth wired up): `ClientOptions { access_token: None, uuid: None }`. The server must allow offline players, and `MC_TP` / RCON ops need the bot to be op'd.

## Architecture

The loop in `main.rs` is **sync state → pick the next incomplete step → execute it**, repeated. Three files form the engine; `tasks/` does the real work.

**`state.rs` — perception.** `sync_from_bot(&Bot)` scans `bot.inventory` and folds it into a flat `GameState` (counts of logs/planks/sticks/cobble/coal/iron/diamonds/food/buckets/flint, best pickaxe & sword `Tier`, has-table/has-furnace, plus health/food/position/dimension). The step machine only ever reads `GameState`, never the bot directly.

**`steps.rs` — the plan.** A static ordered `&[Step]` walking the tech tree (gather wood → planks → table → sticks → wooden pick → mine stone → stone pick/sword → furnace → coal → iron → smelt → iron pick → buckets → water → food → flint & steel). Each `Step` is gated by `can_execute(&GameState) -> bool` and satisfied by `is_complete(&GameState) -> bool`; `execute_step` dispatches by string `id` into `tasks::`.

> **Critical invariant — `get_next_step` returns the FURTHEST-along runnable step (`.next_back()`), not the first.** Steps consume the (consumable) outputs of earlier steps, so an earlier step looks "incomplete" again the moment a later one runs. Picking the last runnable+incomplete step is what stops the bot from looping forever re-gathering. Preserve this when adding steps; keep the slice ordered by progression.

**`tasks/` — execution**, one module per family, each a faithful port of the matching steve task. They translate goals into rustcraft primitives (`bot.goto*`, `dig`/`dig_toward`, `place_block`, `click_window`, `look_at` + control states). Recurring patterns worth knowing before editing:

- **Pathfinder-first, raw-walk fallback.** Tasks try `bot.goto*` (rustcraft's A*); when it stalls short, they fall back to manual control-state walking that carves through terrain. `gather_wood` is the fullest example: `approach_raw` carves stairs up a hillside, `chop` fells a trunk via block-prediction, `explore`/`downhill_angle` escape stone-locked basins, and unreachable trees get blacklisted. `mining` mirrors this with `dig_down`/`descend_step`/`strip_tunnel`.
- **Local state prediction.** Because rustcraft's container-craft and item-use inventory sync is racy (see rustcraft's CLAUDE.md), tasks predict known results with `bot.ensure_item(...)` / manual count edits after a server action succeeds but doesn't echo (e.g. `craft_item` in `bot_utils.rs`, `fill_water_buckets`). Without this the step machine re-runs the step forever.
- **Crafting-table acquisition.** `bot_utils::get_crafting_table` finds a nearby table, else crafts and places one — digging a side niche if the bot is boxed in (e.g. mid-tunnel). 2×2 recipes pass `table: None`; 3×3 recipes need a placed table.
- **Physics-jitter guards.** Vertical block math uses `floor(y - 0.5)` (not `floor(y) - 1`) so a position dipping just under an integer doesn't target the wrong block — see `dig_down`.

### State of the port

Only the **overworld/early phases** are ported (through flint-and-steel). `#![allow(dead_code)]` in `main.rs` covers `GameState`/`Step` fields (sword, furnace, vitals, priority) reserved for later phases. `is_dragon_dead` always returns false (`WorldState::dragon_dead` is never set), so the Nether and End phases — and thus the victory path — are **not yet implemented**. Extending the run means adding ordered steps + task modules following the patterns above.

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
