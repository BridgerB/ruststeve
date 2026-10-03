# ruststeve — cycle 2 retrospective (2026-10-01 17:57 → 2026-10-02 17:57 MDT)

## Goal

Cycle 2's plan (`docs/ruststeve-loop-cycle2.md`, `docs/playbook-2026-10-01.md`): fix measurement first, then water, then lava siting by chunk scan, then mold resume. Gates:
- **Phase B:** water gym 10/10 and zero drownings in a surface race.
- **Phase C:** portal gym ≥ 8/10, median < 15 min, zero refill starvation, zero deaths.
- **Phase D:** a 5-bot 4-hour natural race, then this retrospective and the briefing.

## Outcome against the gates

| Gate | Result | Met? |
|---|---|---|
| A: measurement integrity | `outcome` column, race_meta, SDK death counter, heartbeats, breath events | yes |
| B: water gym 10/10 | water_roofed (the drowning that failed the race) 14/14 and 10/10 on its gate batch; 0 deaths in 50+ water trials after the rework; water_aquifer and water_cave still time out from task re-entry, not drowning | partly |
| B: zero drownings, surface race | 1 drowning (race i4, roofed aquifer); fixed in gyms afterwards, not re-raced | **no** |
| C: portal gym ≥ 8/10, < 15 min, 0 deaths | best batch 6/9 clean (batch 2, median 19.7 min); 10 batches run; deaths per batch fell from 9 to 2 by batch 9 | **no** |
| D: natural race (5 bots, 4 h) | **0 deaths** (gate 1, surface survival, met on a clean fresh-region measurement); **nether 0/5**: the furthest bot reached flint and steel at 54 min, then a crafting-table loop for 3 h | gate 1 yes; portal gate no |

## Wall-clock allocation

| Window (MDT) | Work |
|---|---|
| 17:57–20:57 | Phase A: measurement integrity, race and gym launchers on the box over localhost |
| 20:57–23:00 | Phase B water gym: batches 1–10, breath watchdog, `leave_water`, dig-time penalties |
| 23:00–01:45 | Surface race i4 (the gate measurement), crafting-recipe fix, the roofed-aquifer drowning and water fixes 1–12 in gyms |
| 01:46–11:57 | Phase C: 10 portal gym batches (regions 31–43) with a fix between each |
| 10:19–14:19 | Phase D natural race i5 (started early: the Phase C gate was out of reach with 75 min left); a table-bootstrap gym during the wait |
| 14:20–17:57 | Rate tables, this retrospective, the briefing |

## Rate tables

### Natural race i5

`race-20261002-161914-i5`: 5 bots, 14400 s, fresh region x 15900, cleared inventories, goal nether. Integrity: 1 clear per bot, relaunches 8/6/6/1/1 (the 8-failure abort), 0 watchdog kills.

| Bot | Furthest step | Final state | Deaths | Breath pre-emptions / alarms |
|---|---|---|---|---|
| rust-race-005 | 10/28, flint and steel at 53.6 min | 182 min in `craft_iron_pickaxe` ("need a crafting table" ×111) | 0 | 245 / 20 |
| rust-race-004 | 8/28, furnace at 117.5 min | `craft_furnace`, the same table loop | 0 | 2158 / 19 |
| rust-race-001 | 3/28 | `mine_stone` 0/16 ×167, bobbing at the sea surface | 0 | 3358 / 140 |
| rust-race-002 | 0/28 | `gather_wood` 0/6 ×135, treeless region | 0 | 9333 / 71 |
| rust-race-003 | 0/28 | `gather_wood` 0/6 ×136 | 0 | 11809 / 43 |

Nether-entry rate 0/5; median and p90 time-to-nether undefined. Deaths 0.

### Surface race i4

3 bots, 2 h, goal iron_pickaxe, priority cap 13. Iron pickaxe 2/3 (45 and 52 min). 1 drowning (roofed aquifer). 0 relaunches.

### Portal gym

Every launch is recorded in gym.db. Clean = a pass with 0 deaths.

| Batch | Region | Clean passes / valid trials | Clean median | Deaths | Main failure |
|---|---|---|---|---|---|
| 1 | 31 | 2/5 | 26 min | 6 | refill improvisation |
| 2 | 32 | 6/9 | 19.7 min | 31+ (one death loop) | respawn-into-lava loop |
| 3 | 33 | 2/5 | 17.7 min | 239 | void falls (unloaded-chunk physics) |
| 4 | 37 | 1/4 | 36.3 min | 2 | water on the descent, tunnel stalls (earlier launches void: server crash, ocean) |
| 5 | 38 | 1/5 | 17.9 min | 0 | stalls |
| 6 | 39 | 0/5 | — | 4 | roofed-lake scoop |
| 7 | 40 | 1/4 | 22.3 min | 6 | lava-ringed drop, death loop |
| 8 | 41 | 0/3 | — | 9 | platform-edge jumps after the walk fix |
| 9 | 42 | 0/5 | — | 2 | time (two near-misses) |
| 10 | 43 | **3/10** (6 lit) | 16.7 min (fastest 13.5) | 4 | one death per trial; timeouts |

### Water gym

0 deaths across all post-rework trials.

| Slug | Result |
|---|---|
| water_roofed | 10/10 gate batch, 14/14 overall |
| shore | 2/2 |
| lake | 2/2 |
| cave | 2/2 (post-timeout re-checks) |
| cave_iron | 1/2 |
| aquifer | 0/2 (re-entry) |
| water_descent | 2/2 to below the pool |

### Time-to-site and cluster size

Every bot that reached depth sited a cluster as it arrived. Cluster sizes ranged from 44 to 358 sources, at 2–39 blocks. The logged "into the at-depth search" includes the ~170 s descent.

## The wrong shape, and why

1. **Too many restarts of the portal gym.** Ten batches in ten hours, most stopped after one trial per bot to retest the next fix. Every stop was justified by a new failure in the logs, but the gate was never measured on a single stable build, so batch-to-batch numbers mix builds and terrain. Next cycle: two full batches per build before changing it, except for a safety bug.
2. **A timing bug hid behind tuned behaviour.** `walk_to_xz`, `leave_water` and several mold loops counted `drive_tick()` returns (network packets) as game ticks. Code had been tuned around walks that barely moved. Fixing it to real ticks made walks real and briefly raised lava deaths (0 → 4 → 6 → 9 per batch) until the mold's centring used its non-jumping walker. Lesson: a timing primitive fix needs its own gym before it ships into the portal path.
3. **Fixes to the shared water escape regressed other slugs twice** (the watchdog yield, then the unnormalised exit direction, then the hop-only ledge). Each was caught by the regression batch; each cost a batch. The regression batch is mandatory for any change to `leave_water`.

## What went right

- **Measurement is trustworthy now.** Every gym launch has a row, deaths come from the SDK counter and the server log, and the death-loop guard turns a stuck trial into a clean FAIL in seconds instead of 40 minutes.
- **Ground truth before code paid off every time.** An RCON block map found the roofed aquifer; the bot's own `name_at` view found that air read as "nothing"; per-tick SWIM/BANK traces found the hop-out impulse and the packet-count bug; crash reports found `spreadplayers` blocking the server thread.
- **Safety came down hard.** Fixed: void falls (physics in unloaded chunks), respawn-into-shaft and lava-ringed-drop death loops, the lava look-ahead, the shaft seal, the refill stand repair, and the drop-beside-lava guard. Lava deaths per batch went 9 → 2 (batch 8 → 9).
- **Chunk-scan siting works.** Every bot sites a lava cluster as it reaches depth; the gaps that remain are scoop access (roofed lakes) and distance.

## Open problems, ranked by measured cost

| # | Problem | Measured cost |
|---|---|---|
| 1 | **Crafting-table bootstrap loop.** 3 planks against the table's 4; the 2×2 inventory craft of a log "phantoms" its result; each cycle spends 2 planks on sticks. Not a tag problem: the planks tag resolves all 12 types. | 1 race bot per race (i4: 44 min; i5: 1 h+). Blocks the bot furthest along. |
| 2 | **Surface floater counted as submerged.** `head_in_water` tests feet+1, not the eye, so a bot bobbing at the sea surface (eye in air) loops `leave_water` forever. | 1 race bot in i5 (mine_stone 0/16 for 1 h+) |
| 3 | **Treeless start.** `gather_wood` roams ~90 blocks; the nearest forest was 340 blocks away. | 2–4 of 5 race bots in i5 never got wood |
| 4 | **One death per portal trial**, from three mechanisms: the first lava pour escaping the cup after `platform done`, a long refill approach walk, and a pool-cavity fall. | batch 10: 3 of 6 lit portals not gate-valid |
| 5 | Portal time: roofed-lake scoops, 100+ block tunnels, slow refills | batch 9: 0/5, all timeouts; batch 10: 4/10 timeouts |
| 6 | Water on the descent near the surface (beach landings, shallow aquifers) | 1–2 bots per batch lose 10–40 min |
| 7 | Task re-entry into water after an escape (mine_stone, gather_wood) | water_aquifer 0/2 |
| 8 | Post-goal step loop: gather_wood "ok 9/6" never complete | 1 race bot in i4 (capped run) |
| 9 | An obstructed spawnpoint strands a bot at world spawn | 1 trial in batch 9 |
| 10 | race-b's region forceload generates synchronously (22.9 s stall at the i5 start) | 0 crashes yet; the same mechanism crashed Server B via spreadplayers |

## Next-cycle plan (draft)

Gate 1 (surface survival) is met; gate 2 (portal from scratch) is next. Race i5 shows that the natural pipeline never reaches the portal: the three ranked blockers stop every bot before then. The next cycle fixes those first, then measures the portal.

1. **Phase 1 (hours 0–8): the crafting-table loop.** Reproduce with the race inventory (fragmented plank stacks, a full hotbar) in `table_bootstrap`. Make `Bot::craft` return an error when the result does not appear. Fix the plank and table bootstrap. Gate: `table_bootstrap` with the race inventory 5/5.
2. **Phase 2 (hours 8–12): the eye-height `head_in_water`.** Then the full water regression (all slugs × 2, zero deaths, no slug worse than this cycle).
3. **Phase 3 (hours 12–16): treeless starts.** A long-range wood search (chunk scan for logs, then a long walk) beyond the ~90-block roam. Gym: a treeless start.
4. **Phase 4 (hours 16–24): measure.** A 5-bot 4-hour natural race. Gate: surface survival held, and ≥ 1 bot reaches the portal stage. Then the portal gym on a stable build, two full batches before any change, to attack the one-death-per-trial mechanisms (cup pour, long refill walk, pool cavity).

## Line counts

`src/` total 28,142 lines.

| File | Lines |
|---|---|
| `tasks/portal.rs` | 2,665 |
| `bot/mod.rs` | 2,278 |
| `gym.rs` | 922 |
| `bot_utils.rs` | 884 |
| `tasks/portal_mold.rs` | 789 |
| `path/movements.rs` | 587 |

Uncommitted diff: 22 files, +2,564 / −2,294.
