---
title: The race and its results
description: How ruststeve is measured - the race binary and race.yml (one server per runner, a lane of bots, caps in server ticks), the shared race config, checkpoint starts as data applied over RCON, the stage ladder, endings, case and batch files, coverage and the k/n summary.
type: reference
tags: [race, results, measurement, case-files, starts, stages, coverage]
aliases: [hold after aim, --hold-after-aim, minutes, wall limit, race binary, race.yml, lane, batch file, case file, batch.rsb, case.rsc, Stage, stage ladder, stage reached, Ending, coverage, k/n, starts.toml, race.toml, checkpoint start, results summary, RCON]
status: verified
lastUpdated: 2026-11-25
verifiedAgainst: ruststeve@d0cd4db clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/bot/src/bin/race.rs#fn race(a: &Args) -> Result<bool, String> {"
  - "ruststeve:crates/bot/src/lib.rs#pub fn run(opts: Opts) -> Result<Outcome, String> {"
  - "ruststeve:crates/world/src/stage.rs#pub fn highest(w: &World) -> Option<Stage> {"
  - "ruststeve:crates/results/src/lib.rs#pub fn summarize(batches: &[Batch]) -> Summary {"
  - "ruststeve:crates/results/src/lib.rs#pub fn signature(&self) -> [String; 4] {"
  - "ruststeve:crates/results/src/config.rs#pub fn start_commands(start: &Start, bot: &str, index: usize) -> Vec<String> {"
  - "ruststeve:config/race.toml#bots_per_server = 14"
  - "ruststeve:config/starts.toml#kit = [[\"crafting_table\", 1], [\"oak_log\", 4]]"
  - "ruststeve:.github/workflows/race.yml#./target/release/race --lane \"$LANE\""
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Every ending has a case file"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[measurement-rules]]"
  - "[[observability]]"
  - "[[cc-race-and-gym]]"
  - "[[cc-observability]]"
  - "[[data-out]]"
---

# The race and its results

Issue 07a: the measuring machine, built before any gameplay so every later issue is judged the same way ([[measurement-rules]]). A rate is a real-race k/n with its commit; every ended run leaves a case file.

## Key files
- `crates/bot/src/lib.rs`: the bot loop as a library (`run(Opts) -> Outcome`); `crates/bot/src/main.rs` runs one bot, `src/bin/race.rs` a lane.
- `crates/world/src/stage.rs`: the stage ladder.
- `crates/results`: the race config and starts (`config.rs`), case and batch files and the summary (`lib.rs`), the `results` binary.
- `crates/rcon`: a Source RCON client for the race's own server.
- `config/race.toml`, `config/starts.toml`; `.github/workflows/race.yml`.

## How it works
- **One config** (`config/race.toml`): simulation distance 3, view distance, seed, difficulty, bots per server (14, clojurecraft's measured best), join stagger, the default cap. `xtask server --config` writes `server.properties` from it, so the race and (issue 08) the model read one file.
- **Starts as data** (`config/starts.toml`): a name, the stage it begins, a place (`spawn` or a dimension and position) and a kit; checked at load (an unknown stage or item is refused). The race applies it over RCON once each bot's body is placed: `give` per kit item, then `execute in <dim> run tp`. RCON is on only with a per-run password from `RUSTSTEVE_RCON_PASSWORD`, never committed. The inventory once the kit settles (20 server ticks) is the run's start inventory.
- **The stage ladder** (`world::Stage`, core): `in_world`, `log`, `table`, `stone_pickaxe`, `gear`, `ready` (issue 16: the gear, the portal kit, 4 logs and 32 food points held under the open sky: the clean checkpoint), `iron_kit`, `nether`, `fortress`, `spawner`, `rods_7`, `endermen`, `forest`, `pearls_6`, `pearls_9`, `pearls_12`, `overworld` (issue 19: in the overworld holding 12 eyes and pearls together), `stronghold` (an `end_portal_frame` on record within 128 horizontally), `portal_room` (the feet in an air cell within 8 of the frame ring's centre along each axis, at the frame y - 2 to + 2: the jar's portal room piece), `end`, each a predicate over the world (inventory, dimension, sightings: `fortress` is nether bricks sighted, through memory's kind index). `World::stage_reached` is raised every tick and never lowered: items get spent. Stages the world cannot see yet (a lit portal, the portal room, the dragon) are added by their issues; files carry stages by name, so a new one never moves an old one.
- **Endings** (`results::Ending`, a closed set): `Death { killer }` (the server's `player_combat_kill`, [[wire-nbt-text]]; the run ends there, never a respawn), `Cap` (the start's server ticks reached, 20 per `set_time`), `Kicked`, `Lost`, `Error`, `Cut` (the job's end; not counted in n).
- **A case file per ended run** (`<bot>.case.rsc`, named MessagePack + zstd, and `<bot>.case.txt`): run, bot, commit, lane, start, stage reached, ending, the death text, the first hit and every hit in the last minute (damage type by name, attacker kinds), what the bot was doing and why, items gained and lost against the start inventory, swings, health, place, server ticks, wall time, the recording and its replay command. With the live stream on, its sent, dropped and failed counts ([[snapshot-stream]]). With an aim (`--goal log:<min>`), the planner's summary: reached, stuck, walks, failures by kind, the longest stall, expansions, the brain's time per tick ([[planner]]). A bot that never connects still gets one.
- **A batch file per lane** (`batch.rsb`): the header (run, commit, lane, start, source, config hash, bots, server, the server's tick rate), a row per run with its signature `[ending killer doing why]`, and **coverage**: cases/runs, recordings/runs, events dropped, unknown packets. The race exits non-zero when cases/runs is under 1.0.
- **k/n**: `results summary <dirs>` folds batches into k/n per stage for each (commit, start, source); k counts runs whose stage reached is at least the stage, n every run but `Cut`. It also prints coverage and the ranked signatures.
- **race.yml** (dispatch): one runner per lane (up to 20), Java 25, the cached sha1-checked jar, the server with RCON on, `race`, one recording replayed at the commit; each lane uploads its directory; `publish` writes the summary into the job summary and a `race-<run id>` release with the batches and cases, then calls `ingest.yml` to add the race to the insight store ([[insight-store]]).
- **Spread** (issue 13): a start may say `spread = <blocks>`: each bot is then moved to a random surface spot within that of where it joined (`execute at <bot> run spreadplayers ~ ~ 0 <blocks> false <bot>`), so a lane's bots do not dig up each other's tables and take each other's drops. The `stone` start spreads 128.
- **Holding after the aim** (issue 12e): `hold=true` (`race --hold-after-aim true`, `Opts::hold_after_aim`) keeps a bot whose aim is met living to the cap, its planner's always-rows running on (escapes, defence, food: [[escapes]], [[combat-and-pacing]], [[food-and-hunting]]). `minutes` is the lane job's timeout and the wall limit 15 less: a game hour (72,000 ticks, about 61 minutes at the 19.7 ticks a second a 14-bot runner keeps) takes `minutes=100`.

## The lane's time and files (issue 16, `docs/problems/race-tooling.md`)
- **A lane never loses its files.** The server log and `upload-artifact` run straight after the race, before the replay. The race is ended from the job's start: bots end `Cut` 10 minutes before the job's limit (`minutes`), `timeout` kills the race 7 minutes before it, and the race step's own limit is the job's less 9. Before, the replay ran first and unbounded, and a lane at the job limit uploaded nothing.
- **The replay never fails a lane.** It runs last, at most 5 minutes, with `continue-on-error`; its result is a line in the job summary, a warning when it fails and `verify-lane-N.txt` on the release ([[replay-and-why]]).
- **While the race runs**: every 2,400 ticks each bot rewrites `progress-<bot>.txt` and, beside it, `<bot>.ledger.txt`, `<bot>.senses.txt` and `<bot>.case.txt` (`bot::evidence`; the case text starts `RUNNING` and holds the story so far, and the run's end writes the real case over it). The lane uploads `progress-lane-N.txt` and `evidence-lane-N.tar.zst` to the release every 5 minutes (10 with more than 8 lanes: the workflow token has 1,000 requests an hour), and a snapshot within 30 seconds of its `snap.toml`. Read them with `gh release download race-<id> -p 'evidence-lane-*' -p 'progress-lane-*'`.
- **Each lane reports its seconds** (setup, race, after the race, job) in the job summary.
- **`NoProgress`** (`bot::progress`, `config/race.toml` `no_progress_ticks` and `[progress] items`): progress is a new stage, an item kind the run has not held, more of a goal item than the run ever held (up to the most any goal row asks for) or a 16-block cube not stood in before. "Any count went up" was progress until issue 16, and a bot digging in place gained netherrack and never ended.

## Evidence
- Runners, run 43181608927 on main 409c473: 4 lanes × 14 bots on `spawn`, cap 6000: **56/56 `in_world`, 0/56 above, coverage cases 56/56 and recordings 56/56**, every lane in about 7 minutes; the server ticked 20.1 a second with 14 bots (headroom for more).
- Local, 3 bots on `table`, cap 600: the kit arrived over RCON, 3/3 reached `table`, coverage 1.0, a recording verified with 0 mismatches over 10,846 events; the local server ticked 21.3 a second.

- **The first hour** (issue 12e, run 43396217935, spawn, stone-tools, cap 72,000, hold, 4×14 on Normal): **0 deaths of 56**, every run to the cap; stone pickaxe 51/56, aim met 50/56. But the hour tested survival little: the aim is met in 3 to 4 minutes, then the bots stand still, clustered near spawn, for the rest of it. Lane 0's 14 recordings hold no always-row firing at all (no escape, defence, meal or hunt), no hostile came within 4 blocks, and food stayed at 20 (standing still spends none). Monsters spawn at least 24 blocks from any player and within simulation distance 3, so a still cluster sees them only at the edge.

## Gotchas
- The killer groups across bots only without the victim's name: `death.attack.mob zombie`; the case keeps the whole text.
- `workflow_dispatch` needs the workflow on `main`: a new race workflow runs only after its PR merges.
- Unknown packets are counted, not errors: 16,085 over three 30 s runs, the packets the protocol layer does not model yet.

## Limits
- Today's bots stand still (no planner until issue 09): `doing` is `idle` with that why, and the honest ladder is `in_world` and the kit.
- A batch row carries the planner's aim and whether it was met (`Row::aim`), and `results summary` prints "aim <aim>: met k/n"; `race.yml` takes a `goal` input ([[planner]]).
- Sim batches (`simrace`, [[parity-table]]) are the same files with `source = sim`, released as `race-sim-<run>`; scripts (`--walk square|straight:<yaw>|jump:<yaw>|still`) start after the start settles.
- Swings are 0 until combat exists; the fields are there.

## Issue 14
- **Checkpoint places**: a start may list `places`, overworld feet positions; bot i of every lane takes place i mod n (a lane's bots share one server, so each needs its own; every lane takes the same, paired). The `lava` start puts bots at fourteen landings, 32 blocks apart or more, where `kit` race bots chose cast sites on the race seed, the water bucket filled, cap 24,000: a race works the cast from its first tick (Bridger, 2026-11-16: start where the work is, not from scratch).
- The `kit` start (Bridger, 2026-11-15: two buckets): an iron pickaxe, two buckets, flint and steel, a stone pickaxe and sword, a table, 8 sticks, 16 cobblestone, spread 128 ([[portal-cast]]).
- **A case file carries the planner's notes** (`bot::Notes`, `PlanSummary.notes`): every note in order as "tick what (why)", but one whose `what` repeats either of the two kept before it is left out (a hunt's two notes alternate for an hour), and past 80 from the start and 160 from the end the middle is dropped; the notes left out are counted. A race is read from the release's `batches-<run>.tar.zst`: until 2026-11-17 it was read by exporting every recording, ten to twenty minutes a race, and a case said only "aim portal (the planner)".
- **A race runs a commit whose `ci` is green**: `ci` runs the whole suite on every push in about four minutes, and a newer push cancels an older commit's run.

## Issue 15
- The `nether` start: the post-cast kit, 64 cobblestone and food, `place` in `minecraft:the_nether` at (0, 0), `spread` 48 with `spread_under` 100 (`spreadplayers … under`, so never on the bedrock roof).
- The `fortress` stage holds when the feet are within 6 of nether bricks on record, not when bricks are seen: seen-from-afar met it for 112 of 112 bots standing still (fortress wall 1).
- **Evidence** (ca34f7c, pair 44413659356 and 44413666090, cap 24,000): **111 of 224 reached a fortress (49.6%)**, median building blocks held 213 (no bot under 32), deaths 57.

## The marathon (Bridger, 2026-11-19)
- `.github/workflows/marathon.yml`: one runner, always on the latest main (a push cancels it and starts it again), plays legs back to back: each a fresh bot at spawn with the `race` aim (every stage built so far, in order: a portal cast and entered, then a fortress), `--revive` (a death respawns the bot and play goes on; the server's `keep_inventory` is on), cap 216,000. Each leg's summary goes into the job summary as it ends; the job's legs are a release `marathon-<run>`; the job dispatches itself again at its end, with a 6-hourly schedule as a fallback. The other races keep to 15 runners.
- Revive: `world::Act::Respawn` sends `client_command` 0 and clears the death; `Plan::revived` drops the task in hand and notes "died (…) and respawned".

## Snapshots and the frontier ([[one-bot-first]])
- `race` takes a snapshot the first time any bot in a lane reaches a stage above the lane's floor (the start's stage, or a resumed snapshot's): `save-all flush`, the server's `world/` copied to `<out>/snapshots/<stage>/` with `memory.rsw` (the bot's memory, `results::snapshot::write_memory`) and `snap.toml` (stage, bot, offline UUID, position, dimension, commit). Publish steps add each as a release asset `snapshot-<stage>-<run>-<lane>.tar.zst`.
- `race.yml -f snapshot=frontier` (or `<tag>/<asset>`) restores it: the world unpacked into the server's before it starts, the snapshot bot's `players/data/<uuid>.dat` (26.1.2's place for it, not the older `playerdata/`) copied to every bot's own offline UUID (`results offline-uuid`), `--memory` seeding each bot; the `snapshot` start gives nothing more. `config/frontier.toml` names the furthest snapshot.
- **Snapshots that are not a stage's** (`bot::SnapKind`): `race --snapshot-at <ticks>` (race.yml `snapshot_at`) copies the world and the memory of the first bot of the lane to have played that long, and a bot ending `NoProgress` leaves one of where it stood. Their directory and asset are `at-<ticks>` and `stuck-<bot>`, `snap.toml` begins `kind = "at"` or `kind = "stuck"`, and the frontier refuses both; resume one with `-f start=snapshot -f snapshot=race-<id>/<asset>`.
- **A whole run in one job**: a hosted job ends at 360 minutes. The largest safe cap is 396,000 ticks with `minutes=360`; a 300,000-tick race fits with `minutes=300` (`docs/problems/race-tooling.md`, "A whole run in one job").
- **Development starts are marked** (`bot::devstart`, [[development-starts]]): a start other than `snapshot`, `fresh` or `spawn`, or a resume of a marked snapshot, writes `dev = true` first in each `snap.toml`; race.yml names the asset `dev-snapshot-...` and refuses `snapshot=frontier` on a marked snapshot or a `dev-` asset; a test fails when `config/frontier.toml` names one. Snapshots older than the mark (the present frontier came from the `portal` start) carry none.

## Reading the text files (issue 16, [[observability]])
- `.senses.txt`: `tick x,y,z cell cx,cy,cz on <block>[, lava beside] hp … held … [cursor …] | task (why) | hostiles`; the position has one decimal, the cell is floored (the planner's). A line `tick in <dimension>` when it changes.
- `.ledger.txt`: planner lines in the planner's order, then that second's facts; `[xN, to tick T (#id)]` closes a line told once for a run; `item x 164 -> 177 (3 changes)` is one second. The header counts the file's records and facts, and the lines they are told in.
- `.case.txt`: `where:` under the ending; `notes by kind` (every note of the ledger, with first and last tick: a fix's firings); `escape trails (whole)`; `tasks by time` with those that ended within the second they began.
- `summary.md`: the diagnosis's "notes by kind" table is the race's total over every note of every ledger (it was built from each case's kept notes, which leave repeats out); it says for how many runs it had the whole ledger.

## Issue 17b: the endermen stage
- `Stage::Endermen` (`endermen`) sits between `rods_7` and `pearls_12`: 7 rods held and the feet within `ENDERMAN_NEAR` (32) of an enderman that ticks (`stage::enderman_near`, `world::ticks`). The rods are part of it so an enderman passing a bot at the spawner before rod 7 does not jump the ladder (race 45358767468 lane 1 had one at 31). It reads entities only, so it is checked every tick; the snapshot taker needs no change (it names a snapshot by `Stage::name` and orders by the ladder).
- Ledger facts: `Biome`, `Enderman` (first seen within 128, or begun or stopped ticking), `EndermanStill` (ticking by distance but unmoved for `STILL_TICKS`), `ForestSeen`. A new feet biome or a known forest is progress (`Gain::Biome`). The senses row adds the feet biome, the nearest ticking and frozen enderman, pickaxe uses left and filler held.
- `results endermen <dirs>` counts senses seconds with an enderman within 128 and 32 by place; `results player-dat` prints a gunzipped player file (17a, probes 45472883583 and 45472888528).

## Issue 18b: the pearl stages and the hunt's facts
- Two checkpoints between `endermen` and `pearls_12` (Bridger, 2026-11-24): `forest` (7 rods, `World::feet_biome` is `warped_forest`) and `pearls_6` (7 rods, `PEARLS_HALF` 6 pearls); `pearls_12` now needs the 7 rods too. A third, `pearls_9` (7 rods, `PEARLS_THREE_QUARTERS` 9; 2026-11-25), after a bot at 9 pearls ran out of supply with no stage snapshot past 6 (race 45602491038 lane 6). The snapshot taker names them `snapshot-forest-…` and `snapshot-pearls_6-…` by `Stage::name` with no change.
- Ledger facts: `EndermanAngry` (the creepy flag, index 17, turning true, with whether it ticks), `EndermanHit` (its health, index 9, falling), `EndermanKilled` (entity event 3; `by_bot` when it was struck), `PearlDropped` (an ender pearl item within `PEARL_NEAR` 8 of a death of the last `KILL_LOOT_TICKS` 40), `PearlPicked` (`take_item_entity` for a pearl item known), `BlockTaken` (left for the burrow's owner). The bot's kill and a pearl picked are progress (`Gain::Hunt`); `[progress] items` adds `["ender_pearl", 12]`.
- The senses row adds `pearls`, `kills`, angry endermen ticking and frozen within 128, and `swords` (uses left over every sword): `Senses::hunt`.
- A batch row keeps `pearls` (`Case::pearls_gained`); the summary prints `pearls gained: 0 x5, 12 x1 (6 runs)` when a run reached the forest or gained a pearl.

## See also
- [[observability]] - what every run produces.
- [[data-out]] - where the files go next (the insight store, 07b).
