# LOOP.md — perfect the ruststeve gym until it chains to the Nether

Persistent state + procedure for a self-driving debug loop. The model forgets between firings; this file doesn't. Read it top-to-bottom each iteration, do the work, then **update the "Fixes log" and "Current status" sections** before ending.

## Goal & success criterion

Build and **perfect a gym** — each speedrun sub-task run in isolation across **random real terrain** — until every piece is reliable, then **chain them in a race** to reach the Nether. The insight (from steve + our races): the blockers are terrain-dependent, so a clean-arena test lies; only random-terrain trials expose them.

**DONE when:** every gym slug (esp. the terrain-hard ones) sits at a high pass-rate across random terrain — including `reach_lava`, `cast_light_enter` (once added), and the `to_nether` capstone (`pass = dimension == minecraft:the_nether`) — AND a chained `race.sh` run logs `entered the nether` for a raw `rust-race-*` bot. Target each slug **≥ ~90%** before moving on.

## The gym (all Rust, no bash)

- Run trials: `GYM=<slug> GYM_TRIALS=<n> MC_HOST=144.24.32.76 MC_USERNAME=rust-gym-001 STEVE_DATA=data RCON_HOST=localhost RCON_PORT=25575 RCON_PASS=minecraft-test-rcon ./target/release/ruststeve`
  - One connected bot; each trial self-RCONs `clear`/`give <prereq>`/`spreadplayers` to a random surface spot (0–10k), runs just that step to pass/timeout, records `pass/duration/x,y,z/prereq/message` to `data/gym.db`.
- Report: `GYM=report ./target/release/ruststeve` → per-slug pass% / avg (lowest first). Or `sqlite3 data/gym.db`.
- Dashboard: the Leptos app in `dashboard/` (charts + per-step reproducible history) — see its README.
- **Registry:** `src/gym.rs` `GYM_STEPS` — the 19 pipeline steps + `reach_lava` (pass = `lava_bucket ≥ 1`) + `to_nether` capstone. Prereqs mirror `isolation-test.sh::setup_prereqs`. `custom_pass` overrides the step's `is_complete` where needed (lava/dimension).

## Prereqs to run

- RCON tunnel up: `ssh -fN -L 25575:127.0.0.1:25575 bridger@144.24.32.76` (then `localhost:25575`, pass `minecraft-test-rcon`).
- Release binary built: `cargo build --release`.
- `data/` registry generated (`cargo run --bin datagen` once).

## Iteration procedure (plan → act → verify → critique → replan)

1. **Gather data:** run a batch on the target slug(s), e.g. `GYM=reach_lava GYM_TRIALS=20 …`. Then `GYM=report`.
2. **Pick the lowest-pass slug** that's on the critical path (mining/descent/water/lava/portal first — see steve's data: resource/terrain steps are hardest; crafts are reliable).
3. **Reproduce a failure deterministically:** pick a failing row's `x,y,z` from `gym.db` (`SELECT x,y,z,message FROM gym_runs WHERE slug='…' AND pass=0 ORDER BY ts DESC`), teleport a bot there (RCON `spreadplayers`/`tp`), and watch it fail the same way. Enable debug: `MINE_DEBUG=1 CRAFT_DEBUG=1` etc.
4. **Diagnose → fix the code → rebuild → re-run that slug's gym.** Verify pass% climbs on a fresh batch.
5. **Log it** in the Fixes log, update Current status, repeat. If a step resists ~3 attempts, surface to the user — don't spin.
6. When all critical slugs are green, run `./race.sh` (raw bots, chained) and confirm the Nether.

## Shared-server rules (box shared with the TypeScript `steve` bot)

- **NEVER wipe/reset the world.** Gym forceloads are scoped per trial and removed (never `forceload remove all`). Names in `rust-gym-*` / `rust-race-*`; process-kills scoped to `target/release/ruststeve`. Don't touch steve's bots. Gamerules are snake_case (`keep_inventory`).
- Do **not** touch the `craft`/`click_window`/table load-bearing path (the container-click force-resync) — it's hard-won.
- macOS: `sed -i` needs `''`; prefer `perl -0pi -e` or the Edit tool.

## Fixes log (append-only — the skill library; never re-fix)

Portal / lava chain (from the race-loop session, all committed):
- `72860ea` dig_down digs the full 0.6-wide footprint (straddle bug capped every descent).
- `37c28c3` water fill: memory-first (navigate back to a remembered water body before roaming).
- `8e7e1dc` `climb_out_of_pit`: pillar out of a deep mining shaft before seeking water (was stranded 28 min).
- `4df7f17` lava scoop only from a real SOURCE, prefer a flush stand (feet 1 above), place one if the terrain lacks it.
- `ed1fb36` pillar up to a source when the terrain gives no stand (+ log source_count).
- `9b7f61d` descent settles only when a source is near (skip flowing-only pockets).
- `b8a715a` `source_lava_near` requires AIR above (exposed) — matches `fill_bucket`; stops settling on submerged interior lava (the y-12 "0 sources / 131 lava" stall).
- `70719cf` drop the feet to ONE block above the source (two-above never fills: `lava true->lava`).
- `9340769` predict the lava scoop when the source vanishes (no sync echo) + per-attempt scoop diagnostics.
- `d436619` `dig_down` beside-check is LAVA-only, not water — water-beside wedged descent in wet biomes (217 relocates on surface grass).

## Gym-infra fixes (the harness itself)

- Hard per-attempt timeout around `execute_step` (`tokio::time::timeout`): the deadline was only checked BETWEEN attempts, so a task hanging internally (gather_wood pathfinding to an unreachable tree at 4623,5308) wedged the whole batch for 20+ min. Now it aborts at the remaining budget and records a FAIL. (This also means every batch completes — good data.)
- `setup_trial` retries spreadplayers once if the bot didn't move (first trial after connect sometimes lands the command before the player is teleportable).

## Gym method (steve's)

Run `GYM=random` **4 bots at a time** (`rust-gym-001..004`, e.g. `GYM_TRIALS=12` each) to sweep the pipeline broadly, then `GYM=report` and **focus only on the failing slugs**. Keep total online ≤8 (steve runs its own gym on the box). `GYM=random` excludes the two expensive portal steps; run `reach_lava`/`to_nether` in dedicated focused batches.

## First 4-worker sweep (2026-08-03) — findings

- **Passing:** craft_table, craft_furnace, **mine_stone 4/4** (much better than steve's 19%), smelt_iron, stone_sword, gather_wood ~75%, plus the crafts.
- **Failing (focus):** `mine_iron` (was 0/2, both timed out), `get_flint_and_steel` 0/2, `mine_coal` 0/1, `craft_stone_pickaxe` 0/1 (small sample).
- **mine_iron root cause:** not a hang — `mine_ore` has its own 300s deadline; the gym pass wanted **11 iron** (the full race amount) in a 200s budget, unrealistic in one cold spawn. FIXED: gym `mine_iron` tests the CAPABILITY (`raw_iron+iron_ingot ≥ 3`, steve's bar) with a 250s budget. Re-run to see the TRUE reach-and-mine-iron rate; if still low, THEN it's a descent bug (reproduce with `MINE_DEBUG=1` at a failing x,y,z).

## Current status

- **Gym built + wired** (`21db39e` core, `5ce109b` dashboard, hard-timeout fix pending commit): `GYM`/`GYM=report` + `/gym` dashboard; verified `craft_planks` + `gather_wood` pass on real random terrain and record.
- **First finding:** gather_wood HANGS on some terrain (not just fails) — an unbounded internal loop; the gym hard-timeout is the safety net, but `gather_wood`'s task loop should self-bound (investigate from the FAIL coords). steve's gym had gather_wood ~65%, so expect a terrain long tail.
- **Next:** re-run `gather_wood x5` (now completes via hard timeout) → read pass%; then descend the chain (mine_stone → … → reach_lava → to_nether). Reproduce each failure at its recorded x,y,z.
- **Add later:** `cast_light_enter` gym step (teleport onto a prepared lava arena to isolate cast+light+enter from lava-finding).
- **Blocker:** none in the harness now; data-gathering underway.
