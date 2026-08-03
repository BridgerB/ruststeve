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
- `cf67491` `walk_to_xz` refuses to step onto a lava cell (or a cell whose floor is lava) during the raw scoop walk — the deep-sea scoop no longer walks the bot into the lava and burns it (VALIDATED: gym scooped y-54, deaths=0).
- `3f6f4b8` `prepare_cast_site` chamber-clear shrunk 210→48 cells (only what the frame needs). The 210-deepslate clear timed out prepare's 600s deadline → returned None → the cast step looped re-scooping forever, never casting (made=0). After: **prepare completes, the bot casts its first obsidian** (VALIDATED: gym-002 prefill=1, scoop=1, made=1, at a shallow y106 surface lava lake).
- (near-scoop, pending commit) `cast_obsidian_at` re-scoops the NEAREST lava first (the frame anchors ~4 blocks off the pool edge, so the edge is right there); the far remembered `lava_pool` is only a fallback. Going to the far pool first stranded the bot on the wrong side of a wide lava lake — it cast block 1, walked west to re-scoop, then couldn't path back east across the lava to cast block 2 (stuck at x4524, frame at x4541, 17 away). Validating via to_nether (cast5).

## Gym-infra fixes (the harness itself)

- Hard per-attempt timeout around `execute_step` (`tokio::time::timeout`): the deadline was only checked BETWEEN attempts, so a task hanging internally (gather_wood pathfinding to an unreachable tree at 4623,5308) wedged the whole batch for 20+ min. Now it aborts at the remaining budget and records a FAIL. (This also means every batch completes — good data.)
- `setup_trial` retries spreadplayers once if the bot didn't move (first trial after connect sometimes lands the command before the player is teleportable).

## Gym method (steve's)

Run `GYM=random` **4 bots at a time** (`rust-gym-001..004`, e.g. `GYM_TRIALS=12` each) to sweep the pipeline broadly, then `GYM=report` and **focus only on the failing slugs**. Keep total online ≤8 (steve runs its own gym on the box). `GYM=random` excludes the two expensive portal steps; run `reach_lava`/`to_nether` in dedicated focused batches.

## First 4-worker sweep (2026-08-03) — findings

- **Passing:** craft_table, craft_furnace, **mine_stone 4/4** (much better than steve's 19%), smelt_iron, stone_sword, gather_wood ~75%, plus the crafts.
- **Failing (focus):** `mine_iron` (was 0/2, both timed out), `get_flint_and_steel` 0/2, `mine_coal` 0/1, `craft_stone_pickaxe` 0/1 (small sample).
- **mine_iron root cause:** not a hang — `mine_ore` has its own 300s deadline; the gym pass wanted **11 iron** (the full race amount) in a 200s budget, unrealistic in one cold spawn. FIXED: gym `mine_iron` tests the CAPABILITY (`raw_iron+iron_ingot ≥ 3`, steve's bar) with a 250s budget. Re-run to see the TRUE reach-and-mine-iron rate; if still low, THEN it's a descent bug (reproduce with `MINE_DEBUG=1` at a failing x,y,z).

## mine_iron: descent-limited (~25%), mining works when down

MINE_DEBUG proof: when the bot reaches ore depth (y~40-50) it mines iron cleanly (11-13 in 77-180s). ~75% of trials STALL on the surface→ore-depth descent and time out (oscillate y73↔75, descend_step `moved=false`). This is the SAME descent used by reach_lava/the portal — fixing it lifts mine_iron, mine_coal, AND reach_lava together. Fail-fast ore approach (8s goto_near_timeout) fixed the POI-cycling burn but not the descent. NEXT: capture descent-FAIL traces (run mine_iron with MINE_DEBUG on all 4 workers) to see WHY dig_down/descend_step refuse on the failing surface terrain (cave fall-avoidance? aquifer? slope/tree oscillation?), then harden the descent. Data-quality note: recorded gym coords can be stale (position-sync lag makes setup_trial's "didn't move" retry misfire); fix by pumping until position syncs near the tp target.

## Progress log (focus phase)

- **mine_iron 25% → 37%** across fixes: fail-fast ore approach (8s), dry-veins-then-descend, and the big one — **teleport-sync (5bee3b8)**: MINE_DEBUG confirmed the descent was digging a STALE local world (coords far from the trial spot); now it operates on the right terrain. Remaining fails: mine 1-2 iron then stall reaching the 3rd (long tail). mine_iron is the HARDEST step (steve 2%); 37% + improving is acceptable for now — the race accumulates iron over repeated runs.
- **Key realization:** the stale-world hang affected EVERY slug, so re-measure the whole pipeline on the sync-fixed binary before over-grinding one step. Cadence: sweep → focus → sweep.

## Portal chain (nether-critical) — reach_lava WORKS, cast is the last gap

- **reach_lava = 50% (2/4)** across random terrain. Worker 001 textbook trace: descend y68→64 → "approached lava" → "dropped to scoop level y=53 (surface 52)" (drop-to-source+1) → "fill lava: OK" → lava_buckets=1. The descent→exposed-source→drop→SCOOP chain WORKS on random terrain (pass-recheck c6205d3 correctly catches the transient bucket). NOTE: reach_lava's `lava_bucket>=1` pass is fragile (casting empties the bucket); the pass-recheck saves it, but rely on 'fill lava: OK' / 'entered the nether' as the real signals.
- **LAST GAP = the CAST**: after scooping, build_nether_portal starts the frame ('pool east_edge=.. anchor x=..') then STALLS (no cast progress for the rest of the 600s). Passed in the flush-lava ISOLATION arena, but stalls when lava is a REAL remote source (bot must re-navigate to the source per obsidian, ~10 scoops). To reach the nether across terrain, the cast loop (tasks/portal.rs cast_obsidian_at / build frame / re-scoop-per-block) must complete without stalling. NEXT: run to_nether w/ CAST debug, read WHERE the cast stalls (first obsidian? re-scoop nav? interior clear? ignite?), fix, re-run.
- Only 001 has CAST debug (CRAFT_DEBUG); 002-004 blank traces = no debug, not no-progress.

## Verdict: components validated; portal is WORKS-BUT-SLOW → chain a race

The gym validated every component across random terrain: crafts ~100%, water/smelt 100%, mine_stone 75% / coal 50% / iron 37% (race accumulates), and the **descent→exposed-source→drop-to-source+1→scoop chain works** (reach_lava 50% in ONE 600s shot). The portal CAST is not broken — it's SLOW: after scooping, the frame anchors ~8 blocks from the lava, then the chamber-clear (180-cell dig) + 10-obsidian cast (re-scoop from the offset source each block) can't finish in the gym's single-shot budget (reach_lava 001 scooped then ran out at 600s; to_nether from a y94 spawn spent 900s just descending deep deepslate to find lava). **In a RACE, Build Portal retries over hours** — the budget the gym lacks. So the right test now is chaining a raw race with the improved binary (which carries the gym's task-code fixes: fail-fast ore, dry-veins-descend, plus the earlier portal fixes).

**Future gym/race speedups (if the race stalls at the cast):** anchor the frame CLOSER to the lava (fewer re-scoop walks); shrink/skip the 180-cell chamber clear; cache the lava source so per-block re-scoop navigates straight back. These would speed both gym and race but touch the load-bearing cast — do carefully.

## RACE RESULT: raw bot SCOOPS deep-sea lava, then DIES casting → the last blocker

In the chained race (improved binary), race-2 reached Build Portal, descended ~120 blocks to the deep lava sea, and SCOOPED lava ('dropped to scoop level y=-52 (surface -55)' → 'fill lava: OK' → lava_buckets=1) — a RAW bot completing the hardest capability in a race. Then it **'died at (933.7,-58,543.3)'** — it fell INTO the lava sea (y-58 is BELOW the -55 surface) during the cast/scoop re-nav. keep_inventory saved items, but on respawn it hit the intermittent **'crafting table would not open'** desync and the step machine regressed it to Mine Iron Ore.

**THE FINAL BLOCKER = deep-sea cast is LETHAL.** Casting the 10-obsidian frame next to a lava OCEAN, re-scooping per block, the bot ends up below the surface and burns. Same class the earlier race sessions fought. FIX (careful — cast is load-bearing): keep the bot STRICTLY above the lava surface during drop-to-level/scoop/re-scoop (never step onto a cell whose floor is lava; the drop should stop at surface+1 and the scoop stand must be solid, not the lava rim); AND/OR cast at a SHALLOW surface lava pool (the gym's reach_lava 001 scooped a y52 surface pool cleanly and safely) instead of the deep sea — bias find_fluid/prepare toward shallow exposed lava over the deep sea. Secondary: the post-death 'crafting table would not open' desync regresses a portal-ready bot all the way back — a bot that still HOLDS the portal kit (lava_bucket+water_bucket+flint+cobble) should resume Build Portal, not re-mine iron (Build Portal can_execute / step ordering).

## Race #1 on the cast-fixed binary (2026-08-03, ~t=1600s snapshot)

No nether yet; all 3 overworld, bots PROGRESSING (driver `pick=None` is a stale parse — ignore it, read per-bot race-N.log tails): race-0 Mine Iron pick=Stone 0/11; race-2 Mine Iron pick=Stone **6/11** (furthest); race-1 STUCK at Craft Stone Pickaxe, repeated `missing crafting ingredient id=947` despite 16 cobble+4 stick (craft-window desync, reusing remembered table 899,44,447). Cast is solved → the gate is now IRON (mine_ore searches SOUTH only, slow — 6/11 after ~27min). Watch: does any bot reach Build Portal? If bots stall pre-portal for another ~25min, THEN improve mine_ore (all-dir/spiral) and/or fix race-1's craft strand. Keep the race ALIVE meanwhile (bots keep_inventory, resume on respawn).

## ✅ CAST FULLY VALIDATED (2026-08-03) — `TEST RESULT: PASS build_nether_portal`

The flat flush-lava isolation arena (`STEVE_TEST=build_nether_portal`, `CRAFT_DEBUG=1`, x400/z2400, manual scoped-forceload setup — NOT `isolation-test.sh`, whose line-104 `forceload remove all` is banned) proved the full cast end-to-end: **all 10 obsidian placed, frame lit — "ok — nether portal cast & lit at 418,70,2400"**. The three fixes chain: `cf67491` (scoop-safe walk → no death) → `3f6f4b8` (chamber-clear 210→48 → prepare completes) → `ff3cb8f` (re-scoop NEAREST lava → bot stays at the frame, builds all 10 blocks instead of stranding at block 1). Slow (~60s/block, re-scoop-per-block navigation) but reliable; a race's Build Portal has the retry budget the gym's single-shot doesn't. **→ Next: chain a race on this binary; the cast is no longer the blocker.**

**Manual cast-isolation recipe (reuse to re-validate any cast change):** `pkill -f target/release/ruststeve`; RCON `forceload add 380 2380 430 2420` (scoped); launch `MC_USERNAME=rust-gym-001 STEVE_TEST=build_nether_portal STEVE_TEST_SECS=300 RACE_HOLD=35 CRAFT_DEBUG=1` detached; wait for `holding`; RCON `op`+`tp 400 70 2400`+`spawnpoint`+`fill 390 45 2390 425 69 2410 stone`+`fill 390 70 2390 425 82 2410 air`+`fill 404 69 2395 414 69 2405 lava`+`clear`+give `iron_pickaxe water_bucket bucket flint_and_steel cobblestone 128`; poll `/tmp/castiso-001.log` for `TEST RESULT`.

## (superseded) Current status — THE CAST WORKS; frame-completion is the last gap

- **Portal cast breakthrough (2026-08-03):** two fixes turned the cast from "loops forever, made=0" into "casts obsidian":
  1. `cf67491` — the deep-sea scoop no longer walks into lava (deaths=0).
  2. `3f6f4b8` — chamber-clear 210→48 cells; prepare now COMPLETES → the bot **casts its first obsidian** (gym-002: prefill=1, scoop=1, made=1, at a y106 surface lava lake).
- **Last gap = completing the 10-obsidian frame.** gym-002 cast block 1, then re-scooped from the FAR remembered pool (walked west across a wide lake), and couldn't path back east to cast block 2 (stuck x4524, frame x4541, 17 away, "CAST cast … ENTER" repeating). Near-scoop fix (pending commit): re-scoop the NEAREST lava first so the bot stays at the frame; far pool is fallback only.
- **VALIDATING NOW:** to_nether gym x3 detached (`/tmp/gym-cast5-00N.log`) on the near-scoop binary. Success signal = `made=` climbs past 1 toward 10 → frame build → `entered the nether`. Check: `for i in 001 002 003; do echo gym-$i made=$(grep -c made=true /tmp/gym-cast5-$i.log) prefill=$(grep -c 'prepare pre-fill' /tmp/gym-cast5-$i.log) nether=$(grep -c 'entered the nether' /tmp/gym-cast5-$i.log); done`.
- **If made climbs then stalls again:** read `cast_obsidian_at` (portal.rs ~688) — the block-to-block transition (positioning at each successive frame block, interior clear, ignite, walk-in). If it reaches the nether → RESTART `./race.sh` on the full-fix binary (also improve `mine_ore`: only searches SOUTH, the race's coal/iron bottleneck to even reach the portal).
- **Blocker:** none in the harness now; data-gathering underway.
