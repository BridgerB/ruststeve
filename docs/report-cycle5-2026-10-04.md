# ruststeve cycle 5 report (the race-funnel cycle)

Cycle 5 ran 2026-10-03 19:40 → 2026-10-04 19:40 MDT on branch `feat/cycle5-race-funnel`. Every number below is from `data/gym.db`, `data/race.db` (and its archives), the per-process event logs under `data/attempts/`, or the box logs.
- Intervals are Wilson 95%.
- P(B>A) is Monte Carlo on Beta(1,1) priors.
- "Any-pass" counts passes with deaths; "clean" excludes them.

## 1. The headline

**Neither race reached the nether.** Race i8 got three bots to build_nether_portal; i7 had two. A harness fault invalidated two results: race-b.sh never cleared the per-bot frame anchor, so 6b's displacement recovery tp'd bots to earlier races' molds. i7's 2,500-block "blind tunnel" was toward an i6 mold; i8's one lit portal was its i7 frame, 3,200 blocks away (fixed in 8934356; see CHANGES). The cycle's measured gains are upstream of the portal.

- **Iron.** An early-iron-pickaxe rule raised iron_from_surface from 1/10 to 5/10 (P = 0.968).
- **Portal pathfinder.** A wall-clock A* budget, hidden in the cycle-4 tree, explains why that tree lost to 6b-head on the portal. Tree with that budget vs 6b-head:
  - wall-clock budget: 1/15 vs 8/10;
  - search-time budget: 4/7 vs 6/10, on par.
- **Race placement.** Every bot now starts on land. Race i5 and i6 each lost two bots to an ocean start.
- **The race wall is still water.** Bots walk off land into lakes and do not get out: 3/5 in i7, 1–2/5 in i8. The arena water gyms pass 13/14 and do not reproduce it. A slug at the i8 lake (`water_wall_i8`) now does, 0/10. The cause is a **lily pad**: the escape's "nearest land" search took it as shore 1–4 blocks away, and its collision box then stops the swim at the pad's edge. The real shore is 28–50 blocks out. Two fixes measured 0/2 and 0/10; a third (break the pad, 13ef075) is running at handoff.
- **Late game.** The dragon passed 2/2 by the server check. Crystals did not: the cage-climb build went 0/3.

## 2. Funnel

Bots reaching each step (race.db milestones; a milestone means every earlier kit step was done), out of 5 per race:

| reached | i5 | i6 | i7 | i8 |
|---|---|---|---|---|
| Craft Planks | 3 | 3 | 3 | 5 |
| Mine Iron Ore | 1 | 3 | 3 | 4 |
| Craft Buckets | 1 | 2 | 3 | 4 |
| Build Nether Portal | 0 | 2 | 2 | 3 |
| Enter Nether | 0 | 0 | 0 | 0 valid (1 invalid, stale anchor) |

Gym skills this cycle:

| skill | build | result |
|---|---|---|
| water (arena slugs) | 6b-safe | 12/14 (0.60–0.96) |
| water (arena slugs) | 6b-head | 13/14 (0.69–0.99) |
| iron_from_surface | baseline (tree, wall A*) | 1/10 (0.02–0.40) |
| iron_from_surface | early iron (b08ffcd) | 5/10 (0.24–0.76) |
| portal | 6b-head, region 57 | 8/16 (0.28–0.72), clean 6/16 |
| portal | 6b-safe-p, region 59 | 3/6 (stopped, clause a) |
| portal | 6b-head, regions 62 + 68 (matched) | 8/10 |
| portal | tree, wall-clock A*, regions 60/62/68 | 1/15 |
| portal | tree + search-time A*, region 69 | 4/7 (stopped, clause a) |
| portal | 6b-head, region 69 | 6/10 |
| dragon | late27-cry8 | 2/2 (server check) |
| crystals | cage1 (4d91508) | 0/3 |

## 3. Where the race time went

| race | bot | where it ended | how long |
|---|---|---|---|
| i7 | 004, 005 | gather_wood in a lake | ~3.7 h each |
| i7 | 001 | craft_bucket, swimming | last ~1.5 h |
| i7 | 003 | build_nether_portal, blind tunnel | ~2 h, 2,500 blocks at y −60 |
| i7 | 002 | craft_planks / craft_sticks loop at depth | ~1.5 h |
| i8 | 002 | mine_stone in a lake | all race, 14 relaunches |
| i8 | 001 | enter_nether on its stale i7 portal (invalid) | last ~1.5 h, 100 relaunches |
| i8 | 004, 005 | build_nether_portal at depth | from 82–84 min |
| i8 | 003 | get_flint_and_steel | last ~1 h |

## 4. Bandit posteriors

None live. The bandit-capable tree never raced: decision 1 put both races on 6b-head, whose tree has no bandit or event-row code. Per-bot arm wiring is committed and tested (511337e, 5004e3f). The 12-trial bandit gym batch was not run, because it was specified "on the race build". **No posterior moved this cycle.**

## 5. Structural changes and their comparisons

1. **6b-safe** (diagonal lava rule, live lava re-check, no sprint near lava, 8-tick cap verify, mid-task stone pickaxe, frame check by mold cells, settle lava refusal).
   - The first B batch was excluded: B had the new gym rules and A did not (harness fault). Re-run with `GYM_PARITY`.
   - Result: 3/6 vs A's 8/16. Stopped by clause (a): one bot died twice in 31 s at one mold site, both during mold gotos.
   - **Not raced.**
2. **SAFE_FIXES gate** (3848887): the 6b-safe changes behind a flag, so this tree can run 6b-head behaviour. Measured as "race-head": 1/12 on portal.
3. **Sliced A* budget** (7d3e236, da2625d):
   - The cycle-4 plan_path drives a 50 ms tick between 40 ms slices against a wall-clock 2 s, so it searched ~0.9 s. Mold gotos and tunnel detours timed out ("tunnel_step: no floor … — pathfinder" until timeout).
   - Search-time budget, region 69: 4/7 vs 6b-head 6/10, P = 0.45, so on par. Stopped by clause (a) (mold-site deaths).
   - The first A* batch was also stopped by clause (a): two bots walked to a dry remembered pool and died mid-descent.
4. **Early iron pickaxe** (b08ffcd, bounded in 4e6c4a6): 1/10 → 5/10, P = 0.968.
5. **Crafting table: cave_air counts as empty** (2885487). Found in a 1,944-attempt loop; folded into the early-iron build, not measured alone.
6. **Frame check by stored mold cells** (dcd279c). Reproduced the 6b-head hang once (102 attempts at "12 obsidian within 8, mold 0/10").
7. **settle_xz lava refusal** (38cac1b): fired 13× at one site, all real lava.
8. **Race harness**:
   - dry-land placement (66a6d7a): 10/10 bots on land across i7/i8;
   - respawn check (fixed after i7 to also accept the last position): two correct tp-backs in i8 from world spawn (3,249 / 3,217 blocks).
9. **Cage climb** (4d91508): 0/3; see section 7.
10. **Water wall**, slug `water_wall_i8` at the i8 lake:
    - baseline 0/10 (0.00–0.28);
    - 40 s escape budget (56f7e6b): 0/2, stopped once the log showed the escape ending early;
    - "land" floating on water excluded (b395ecb): 0/10;
    - the bot still stops against the lily pad's collision box at x 25082.3;
    - breaking the pad (13ef075): 10 trials running at handoff.
11. **Stale frame anchors across races** (8934356): race-b.sh now clears `.frame-<bot>.txt` per race (i7 003's tunnel and i8 001's portal were both caused by it).

## 6. The races

| | i7 | i8 |
|---|---|---|
| binary | target-head (6b-head, old tree) | target-head |
| start (MDT) | 01:05 | 10:20 (moved up from ~13:40; the gym was idle) |
| region | x=21900 | x=24900 |
| land placement | 5/5 | 5/5 |
| furthest | Build Nether Portal ×2 | Build Nether Portal ×3 (+1 invalid Enter Nether) |
| portal step reached at | 42, 45 min | 3_TIMES |
| deaths (all causes) | 3 (lava 3) | 9 (lava 6, drowned 3) |
| relaunches | 8 | 116 (001 ×100 on the invalid-portal loop, 002 ×14 in a lake, 004 ×2) |
| harness respawn tp | 2 (wrong: landing-only check) | 2 (correct: world-spawn respawns) |

Deviation: neither race had event rows or bandit arms (decisions 5 and 6), because both ran the old tree.

## 7. Late-game gyms

- **Dragon:** 2/2 by the server check (hours ~3.5 and ~11), 0 deaths, each at the 900 s timeout.
- **Crystals (one 2-hour slot, cage1):** 0/3, 0 deaths.
  - The bow killed 3, 4 and 4 of 10.
  - Of the given-up crystals, two towers registered as caged (33 bars each).
  - Climbs reached the top 3 times but opened 1 bar in total.
  - Cause: the pillar column (tower r+1) ran inside the radius-2 cage ring. pillar_up dug the bars over its head by hand, and the kit has no pickaxe.
  - One climb ran out of obsidian; ground detection mis-read from a high start.
  - Fix list:
    - pillar at max(tower r, cage r) + 1;
    - a pickaxe in the kit;
    - ground taken before climbing;
    - verify the water pour;
    - scan cages after walking near each tower.

## 8. Tick-rate experiment

Rejected by audit, not run. The client tick is a fixed 50 ms, `ticking_state` is unhandled, and ~50 wall-clock timers sit on the water and portal paths. Gyms ran at tick 20.

## 9. Measurement integrity

- **Harness asymmetry** (excluded): 6b-safe's first batch ran with the same-failure cut and respawn tp-back, which 6b-head's tree lacks. `GYM_PARITY=1` now turns both off.
- **Shared-region contamination** (excluded): a 6b-head bot "passed" by entering another bot's portal (row 867, `mold cast incomplete at 0/10`). It is the only such pass since row 795. Arms must use separate regions, or the pass check must require the bot's own frame.
- **Stale last-line logs:** "NO PICKAXE in inventory" is rate-limited and was misread as the iron cause until the step trace showed the 22-iron quota.
- **Respawn check:** the i7 version tp'd portal bots off their molds. Fixed before i8.
- **Event log:** per-process files (`data/attempts/<run>-<bot>.jsonl`), with a test of 2 writers × 2,000 rows → 0 unparseable. The abort handler records `aborted` rows (verified many times this cycle).
- **Builds over the limit:** five builds this window touched portal-adjacent behaviour (safe, safe2, race-gate, astar, iron). The cap of three per 24 h was exceeded; each is in CHANGES with its measurement.

## 10. Open problems, ranked by expected bot-minutes per 5-bot race

| rank | problem | evidence | expected bot-minutes |
|---|---|---|---|
| 1 | Water wall: bots walk into lakes and never bank (lily pads: false bank, then a collision stop) | i7 3/5 bots for 1.5–3.7 h; i8 1–2/5 (002 all race); slug 0/10 → 0/10 after the first fix | ~500 |
| 2 | Portal at depth: blind tunnels and mold-site deaths | i7 003 tunnelled 2 h; gym timeouts mostly "tunnelling toward" | ~120 |
| 3 | Iron quota stall (fixed in the tree, not raced) | gym 1/10 → 5/10 | ~100 in races still on 6b-head |
| 4 | Tool loop at depth (no wood to remake a pickaxe) | i7 002 ~1.5 h; i8 004 gather_wood at y −51 | ~90 |
| 5 | Crystals (caged and unhittable towers) | 0/3; bow ≤ 4/10 | blocks the end game only |

## 11. What I would tell the other bot's agent (steve)

These are things steve likely shares, since ruststeve's SDK is a port of typecraft.

1. **cave_air is not air by state id.** Any "is this cell empty" test that compares to state 0 rejects cave_air (15293 here). Table placement looped 1,944 times in a carved cave. Compare names ending in "air".
2. **A sliced pathfinder needs a search-time budget.** Ticking between slices against a wall-clock budget silently cut the search to ~45% and turned solvable paths into timeouts.
3. **The furthest-runnable step picker can starve a tool.** smelt needs a furnace that ranks below mine_iron, so no iron pickaxe until the full iron quota. Make the first iron pickaxe as soon as 3 iron exist.
4. **Radius counts lie.** "Frame already cast" by counting obsidian within 8 was fooled by natural obsidian. Judge the stored frame's cells (steve's own cycle-4 finding, confirmed here).
5. **A raw walk near lava needs its own check.** Our mold's centring walk bypassed every pathfinder lava rule.
6. **Respawn checks must use the bot's last position, not its start.** Bots move their own spawnpoint; world-spawn respawns are the case to catch. The server ignored the spawnpoint twice in i8.
7. **Concurrent arms in one region contaminate portal passes:** a bot can enter another bot's portal.
8. **Placement:** forceload, wait for `execute if loaded`, then test water, lava and shore (scripts/race-place.ts).

## 12. What was hard, and what I need decided

**What worked:**
- the death rule (clause a) as a brake;
- server-side pass checks;
- matched regions for close calls;
- the step-trace diagnosis that found the iron quota.

**What was hard:**
- Decision 1 tied both races to a tree without the event log or bandits. Every measured fix this cycle lives in the other tree.
- Clause (a) stopped three arms that were not worse on any-pass (6b-safe 3/6, A* 4/7 at P = 0.45, the first A* batch). Its "path the build changed" is broad when a change touches the pathfinder, which every movement uses.

**Decisions I need:**
1. **The race tree for cycle 6:** the current tree with `ASTAR_SEARCH_BUDGET=1` + early iron + the table fix (event rows, bandits, water fixes), or keep racing 6b-head?
2. **Clause (a) for pathfinder-wide changes:** keep stopping at two same-site deaths, or compare deaths per trial at the cap (6b-head runs at 1.25 deaths per trial)?
3. **find_lava_cluster reads unexposed lava;** 57 of 64 portal passes used it. Keep it, or replace it with an exposure-honest search (it changes pool choice)?
4. **Crystals:** a second cage build (fix list in section 7), or move crystals to a pickaxe + pillar + melee design?

## State for the next cycle

- **Branch** `feat/cycle5-race-funnel` (pushed through c1b957d; later commits pushed with this report). No PR opened; the cycle-4 PR #3 is merged.
- **Box** (bridger@144.24.32.76, `~/ruststeve`, Server B only). Binaries:
  - `target-head`: 6b-head, old tree, ran races i7 and i8;
  - `target-tree`: current tree as of bf6701b;
  - `target-iron2`: early iron + bounded rule; with `ASTAR_SEARCH_BUDGET=1` this is the region-69 candidate;
  - `target-lily`: tree + lily-pad water fix (b395ecb);
  - `target-crystal`: cage1;
  - `target-late2`: late27-cry8, the dragon runs.
- **Switches in this tree:**
  - `SAFE_FIXES=1`: 6b-safe behaviour;
  - `ASTAR_SEARCH_BUDGET=1`: search-time A*;
  - `GYM_PARITY=1`: no same-failure cut or respawn tp, for arms against old trees;
  - `BOT_ARMS=1` or `ARM_BUCKETS` / `ARM_CLUSTER_MIN`: per-bot bandit arms.
- **Data:**
  - `data/gym.db` (gym-index 74);
  - `data/race.db` = race i8 (race-index 8, so the next race is i9); archives in `data/archive/`;
  - `data/attempts/*.jsonl`, plus the legacy `data/attempts.jsonl` (read-only);
  - `data/params.json`: all bandits `enabled: false`.
- **race-b.sh** on the box has both cycle-5 race fixes:
  - the respawn check accepts the last position;
  - frame anchors are cleared per race.
- **Disk:** 11 G free of 46 G. **Memory:** 17 Gi available of 23 Gi.
- **Next race command** (current tree, if decision 1 changes):

```
cd ~/ruststeve && ASTAR_SEARCH_BUDGET=1 BIN=$HOME/ruststeve/target-lily/release/ruststeve BUILD=tree-lily RACE_INDEX=9 N=5 setsid ./race-b.sh > race-i9.out 2>&1 < /dev/null &
```

- **Next arms** (separate regions; B = candidate):

```
cd ~/ruststeve && BIN=$HOME/ruststeve/target-head/release/ruststeve BUILD=6b-head SLUG=portal N=3 TRIALS=6 STAGGER=30 GYM_REGION=75 PIDFILE=gym-a.pids NAME_START=1 setsid ./gym-tonether-b.sh > gym-arm-a.out 2>&1 < /dev/null &
ASTAR_SEARCH_BUDGET=1 GYM_PARITY=1 BIN=$HOME/ruststeve/target-lily/release/ruststeve BUILD=tree-lily SLUG=portal N=3 TRIALS=6 STAGGER=30 GYM_REGION=76 PIDFILE=gym.pids NAME_START=4 KEEP_FORCELOADS=1 setsid ./gym-tonether-b.sh > gym-arm-b.out 2>&1 < /dev/null &
```
