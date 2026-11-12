---
title: The race and its results
description: How ruststeve is measured - the race binary and race.yml (one server per runner, a lane of bots, caps in server ticks), the shared race config, checkpoint starts as data applied over RCON, the stage ladder, endings, case and batch files, coverage and the k/n summary.
type: reference
tags: [race, results, measurement, case-files, starts, stages, coverage]
aliases: [race binary, race.yml, lane, batch file, case file, batch.rsb, case.rsc, Stage, stage ladder, stage reached, Ending, coverage, k/n, starts.toml, race.toml, checkpoint start, results summary, RCON]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@d65565e clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/bot/src/bin/race.rs#fn race(a: &Args) -> Result<bool, String> {"
  - "ruststeve:crates/bot/src/lib.rs#pub fn run(opts: Opts) -> Result<Outcome, String> {"
  - "ruststeve:crates/world/src/stage.rs#pub fn highest(w: &World) -> Option<Stage> {"
  - "ruststeve:crates/results/src/lib.rs#pub fn summarize(batches: &[Batch]) -> Summary {"
  - "ruststeve:crates/results/src/lib.rs#pub fn signature(&self) -> [String; 4] {"
  - "ruststeve:crates/results/src/config.rs#pub fn start_commands(start: &Start, bot: &str) -> Vec<String> {"
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
- **The stage ladder** (`world::Stage`, core): `in_world`, `log`, `table`, `stone_pickaxe`, `iron_kit`, `nether`, `fortress`, `rods_7`, `pearls_12`, `end`, each a predicate over the world (inventory, dimension, sightings: `fortress` is nether bricks sighted, through memory's kind index). `World::stage_reached` is raised every tick and never lowered: items get spent. Stages the world cannot see yet (a lit portal, the portal room, the dragon) are added by their issues; files carry stages by name, so a new one never moves an old one.
- **Endings** (`results::Ending`, a closed set): `Death { killer }` (the server's `player_combat_kill`, [[wire-nbt-text]]; the run ends there, never a respawn), `Cap` (the start's server ticks reached, 20 per `set_time`), `Kicked`, `Lost`, `Error`, `Cut` (the job's end; not counted in n).
- **A case file per ended run** (`<bot>.case.rsc`, named MessagePack + zstd, and `<bot>.case.txt`): run, bot, commit, lane, start, stage reached, ending, the death text, the first hit and every hit in the last minute (damage type by name, attacker kinds), what the bot was doing and why, items gained and lost against the start inventory, swings, health, place, server ticks, wall time, the recording and its replay command. With the live stream on, its sent, dropped and failed counts ([[snapshot-stream]]). With an aim (`--goal log:<min>`), the planner's summary: reached, stuck, walks, failures by kind, the longest stall, expansions, the brain's time per tick ([[planner]]). A bot that never connects still gets one.
- **A batch file per lane** (`batch.rsb`): the header (run, commit, lane, start, source, config hash, bots, server, the server's tick rate), a row per run with its signature `[ending killer doing why]`, and **coverage**: cases/runs, recordings/runs, events dropped, unknown packets. The race exits non-zero when cases/runs is under 1.0.
- **k/n**: `results summary <dirs>` folds batches into k/n per stage for each (commit, start, source); k counts runs whose stage reached is at least the stage, n every run but `Cut`. It also prints coverage and the ranked signatures.
- **race.yml** (dispatch): one runner per lane (up to 20), Java 25, the cached sha1-checked jar, the server with RCON on, `race`, one recording replayed at the commit; each lane uploads its directory; `publish` writes the summary into the job summary and a `race-<run id>` release with the batches and cases, then calls `ingest.yml` to add the race to the insight store ([[insight-store]]).

## Evidence
- Runners, run 43181608927 on main 409c473: 4 lanes × 14 bots on `spawn`, cap 6000: **56/56 `in_world`, 0/56 above, coverage cases 56/56 and recordings 56/56**, every lane in about 7 minutes; the server ticked 20.1 a second with 14 bots (headroom for more).
- Local, 3 bots on `table`, cap 600: the kit arrived over RCON, 3/3 reached `table`, coverage 1.0, a recording verified with 0 mismatches over 10,846 events; the local server ticked 21.3 a second.

## Gotchas
- The killer groups across bots only without the victim's name: `death.attack.mob zombie`; the case keeps the whole text.
- `workflow_dispatch` needs the workflow on `main`: a new race workflow runs only after its PR merges.
- Unknown packets are counted, not errors: 16,085 over three 30 s runs, the packets the protocol layer does not model yet.

## Limits
- Today's bots stand still (no planner until issue 09): `doing` is `idle` with that why, and the honest ladder is `in_world` and the kit.
- A batch row carries the planner's aim and whether it was met (`Row::aim`), and `results summary` prints "aim <aim>: met k/n"; `race.yml` takes a `goal` input ([[planner]]).
- Sim batches (`simrace`, [[parity-table]]) are the same files with `source = sim`, released as `race-sim-<run>`; scripts (`--walk square|straight:<yaw>|jump:<yaw>|still`) start after the start settles.
- Swings are 0 until combat exists; the fields are there.

## See also
- [[observability]] - what every run produces.
- [[data-out]] - where the files go next (the insight store, 07b).
