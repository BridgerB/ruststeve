# Event log schema (cycle 4)

`data/attempts.jsonl` is append-only, one JSON object per line. It's written by `src/learn.rs` (Rust bot) and should be matched field for field by the TypeScript bot (steve). On the race path each row is also mirrored into `race.db` table `attempts` (indexed columns plus the full row as `json`).

## Attempt row (one per step attempt; one per gym trial)

| Field | Type | Meaning |
|---|---|---|
| `run_id` | string | `RACE_ID` (race), `GYM_RUN` = `gym-r<region>-<slug>` (gym), else `adhoc` |
| `bot_impl` | `"rs"` \| `"ts"` | which bot wrote the row |
| `build` | string | `BUILD` env set by the launcher (e.g. `6c`, `6c+lsm`); `dev` if unset |
| `world_seed` | int \| null | RCON `seed` read by the launcher |
| `skill` | string | one of the twelve skills (below) or `other` |
| `step_id` | string | the step machine's id (`mine_iron`, `build_nether_portal`, …) |
| `source` | `race` \| `gym` \| `adhoc` | |
| `bot` | string | username |
| `gym_slug` | string | gym rows only |
| `start_ms` | int | wall clock at attempt start |
| `duration_s` | float | wall seconds |
| `outcome` | `ok` \| `timeout` \| `failed` \| `death` | see the rules below |
| `reason` | string | stall/budget reason, the failure, or the step that took over |
| `deaths` | int | deaths during the attempt (the SDK death counter) |
| `death_cause` | string \| null | `"see server log"` when deaths > 0 (the client sees no cause) |
| `pos` | [x, y, z] | block position at attempt start |
| `progress_best` | float | best value of the step's progress ratchet (Part 8) |
| `params` | object | every bandit arm in force for this attempt: `{ "cluster_min": "100", … }` |
| `context` | object | features at attempt start (below); `null` for gym rows |

**Outcome rules (step loop):**
- `ok`: the loop moved to another step and this step's `is_complete` holds.
- `failed`: the loop moved away without completing it (a regression or preemption), or the process exited mid-attempt. A killed process's attempt is closed as `failed` by the next launch from `data/.attempt-<bot>.json`.
- `timeout`: the stall detector or budget ended it (Part 8).

**Gym rows:** pass → `ok`, timeout → `timeout`, a water slug passing with deaths → `death`, anything else → `failed`.

## Primitive row (one per guarded-primitive call)

Same shape. `skill` is the primitive (`leave_water`, `station_refill`; `lava_safe_move` and `tunnel_step` from Phase 1). `step_id` is the calling step when known. `outcome` may be `vetoed` (the post-condition refused the arrival). `context` carries the primitive's own fields (e.g. lava before/after for `station_refill`).

## Skills (Part 3)

`wood_stone_tools`, `iron`, `kit_craft`, `portal` (lava site + cast + light: one step today), `fortress`, `blaze_rods`, `pearls`, `stronghold`, `end_entry`, `crystals`, `dragon`.

## Context features: exposure-only

Every feature is something the bot legitimately sensed. No feature reads a block the bot has not seen exposed. This is the no-X-ray rule as an ML rule: a model trained on leaked truth looks good in the gym and fails in the race.

| Feature | How the bot senses it |
|---|---|
| `y` | its own position (server position packets) |
| `health`, `food` | the health packet |
| `time_of_day` | the time-update packet |
| `dim` | the login/respawn packet |
| `cobble`, `iron`, `buckets`, `lava_buckets`, `pick_rank` | its own inventory (window packets) |
| `hostiles_seen` | hostile entities within 24 blocks that the server sent as spawned (visible-range entity packets) |
| `lava_cells_8`, `water_cells_8` | `find_exposed_blocks`: cells within 8 with an air neighbour (exposed), not raw chunk data |
| `ore_sightings_memory` | iron-ore POIs in WorldMemory, recorded only from exposed sightings |
| `in_water`, `head_in_water` | the fluid at its own feet and eye cells |
| `light`, `air` | `null`: the bot does not track them yet |

**Audit (2026-10-03):** all features above are exposure-only. The one known violation in the codebase is not a feature: `find_lava_cluster`, the lava-siting scan, reads unexposed chunk data. It is flagged under its own heading in the retrospective, per the cycle-4 rules.

## params.json

`data/params.json`, read at the start of every attempt (hot-reload), written under a lock file:

```json
{ "params": { "cluster_min": { "skills": ["portal"], "enabled": false,
                               "arms": { "60": [1, 1], "100": [1, 1], "150": [1, 1] } } },
  "budgets": { "mine_iron": 1200 },
  "stall_s": { "default": 180, "build_nether_portal": 300 } }
```

`arms[v] = [a, b]` with `a` = successes + 1 and `b` = failures + 1. Only `enabled` parameters whose `skills` include the attempt's skill are drawn (Thompson sampling). Tools: `node scripts/ml/bandit.ts init|show|refit|test`, `node scripts/ml/compare.ts`.
