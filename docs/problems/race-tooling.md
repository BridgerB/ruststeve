# Race tooling: walls in the experiment loop (issue 16)

The walls here are not the bot's: they are what made each experiment on issue 16 slow or lossy. Each has what happened, the read, the cause, the fix and its proof. Runs named "development" came from a made-up start and prove tooling only, never a stage (`brain/ruststeve/decisions/development-starts.md`).

## State (2026-11-23, night)

| Wall | Fix | Commit | Proof |
|---|---|---|---|
| 1. A lane at the job limit uploads nothing | upload before the replay; the race ended from the job's start | 62b1c9f | race 45334668363: artifact present, replay failed visibly, lane green, job 7.4 minutes |
| 2. `replay verify` is quadratic and wrong on resumed runs | one fold, two refolds; the start state recorded | 91246cc | race 45335753205: `verified, 0 mismatches` in 2 s on a resumed run; probe 45337136275: the 130,750-event lane-5 recording in 1.8 s |
| 3. The caches were never saved | an absolute jar path; a `race` rust-cache key saved by lane 0 | 62b1c9f | setup 129 s cold (race 45334668363), 77 s warm (race 45335753205); 152 s before (race 45194253932 lane 5) |
| 4. Evidence only at a run's end | ledger, senses and story rewritten every 2,400 ticks and uploaded | 982cf5c | race 45336625960: `evidence-lane-0.tar.zst` on the release 5 minutes in, while the bot ran |
| 5. A bot digging in place never ends | progress is a stage, a new item kind, more of a goal item, a new cube | 86d45e5 | unit tests (`bot::progress`); no race has run 24,000 idle ticks on it yet |
| 6. No way to ask the planner again offline | `replan`, `why --blocks --radius --ys` | a5ae696 | probe 45337136275 (below) |
| 7. A development start could become the frontier | `dev = true` in `snap.toml`, a `dev-` asset name, the frontier refuses both | 982cf5c | unit tests (`bot::devstart`); see wall 7 for the race |
| 8. The insight job fails while another race runs | a release without batches is left out | c525230 | race 45336625960: insight green, 7 releases left out |
| 9. `brain-check` read another worktree's brain | the root is the working directory's | bbb3210 | unit test; `cargo run -q -p brain-check` from this worktree: four zeros |

## Wall 1: a lane at the job limit uploads nothing

- **What happened**: in races 45164371770 to 45204423376 lanes that reached `timeout-minutes` left no artifact: no ledger, no case, no recording. Hours of play were lost per lane.
- **Read**: `.github/workflows/race.yml` at ddae609. `Replay one recording at this commit` ran before `Keep the server log` and `upload-artifact`; the race step had no limit of its own and its wall limit was counted from the step's start, not the job's; the replay's output went through `| tee`, so its exit code was the tee's.
- **Cause**: everything a lane keeps left the runner in the last step, behind a step of unbounded length (wall 2), inside a job with a hard limit.
- **Fix**: the server log and `upload-artifact` run straight after the race. The race is ended from the job's start (`JOB_START`): bots end `Cut` 10 minutes before the job's limit, `timeout` kills the race 7 minutes before it, and the step's own `timeout-minutes` is the job's less 9 (worked out in the `lanes` job, since expressions cannot subtract). The replay runs last, under `timeout` (5 minutes, and never past a minute before the job's end) and a 6-minute step limit, with `pipefail` and `continue-on-error`: its result is a line in the job summary, a warning annotation when it fails, and `verify-lane-N.txt` on the release. Each lane prints `setup`, `race`, `after the race` and `job` seconds to the summary.
- **Proof**: race 45334668363 (62b1c9f, 1 lane, the Nether snapshot, cap 6000, minutes 30). Race 45336625960's summary line: `lane 0: setup 63 s, race 452 s (exit 0), after the race 11 s, job 526 s`. Lane steps: race 05:40:35 to 05:45:39 UTC, upload at 05:45:39, replay 05:45:40 to 05:45:44 with `FAILED (exit 1)` (the false mismatches of wall 2, on the old verify) and the lane still green; the artifact `race-45334668363-lane-0` holds 12 files (batch, case, ledger, senses, moments, truth, recording, both logs). The job ran 05:38:23 to 05:45:49, 7.4 minutes.
- **Not proven**: a lane actually reaching its limit. The arithmetic is in the step; no race here ran that long.

## Wall 2: `replay verify` took 12 to 60 minutes and failed every resumed run

- **What happened**: after a three-hour race the replay step ran for most of an hour; on every run resumed from a snapshot it reported one mismatch per checkpoint. The "Stopped (4:30 PM)" note read the lost time as slow servers; the servers ran 20 ticks a second and the time went here.
- **Read**: `crates/replay/src/bin/replay.rs` at ddae609, lines 148 to 165: after the fold from the start, one more fold from every checkpoint to the end. A recording has a checkpoint every 1,200 server ticks, so 180 of them in three hours and 90 times the work of one fold. `crates/bot/src/lib.rs` 452: `run.world.memory = m` seeds the snapshot's memory into the world, and no record carries it, so a fold from `World::new()` can never equal the first checkpoint, nor any later one.
- **Cause**: quadratic work; and a start state that was not in the recording.
- **Fix**: `replay::verify` (a new library in `crates/replay`, unit-tested). One fold from the start compares every step's effects and every checkpoint's world. A checkpoint that matches proves a fold from it would go as the running fold does, so only the middle checkpoint and the last are folded again (they prove `Folder::from_checkpoint`). A checkpoint that differs is counted once and the fold goes on from the recorded world. The bot now writes the seeded world as a checkpoint before the first event (`event_index` 0, the existing record variant: the format grows by accretion) and `Folder::take_start` begins a fold from it (`replay verify`, `ledger`; `why` already started from the last checkpoint). An older resumed recording, whose first checkpoint differs from the fold in the memory alone, is taken as the truth and said so in a line, not counted.
- **Proof**: race 45335753205 (86d45e5, resumed from the Nether snapshot): `from the start: 40930 events, 3661 effects, 5 checkpoints, 0 mismatches`, `the start state is recorded`, two refolds, `51913 events stepped in 1.3 s`, `verified`. Probe 45337136275 on `rs-l05-b00.rsr` of race 45194253932 lane 5 (recorded before the fix): `130750 events, 21 checkpoints, 0 mismatches`, the memory-only line, `190686 events stepped in 1.8 s`. The old code stepped the events once per checkpoint (21 folds there).
- **Not measured**: a three-hour recording on the new code. The work is at most 2.5 folds whatever the length; reading the file (every checkpoint's world held in memory at once) is unchanged.

## Wall 3: the caches were never saved

- **What happened**: every lane spent about 44 s fetching the server jar and 85 s building (race 45194253932 lane 5: 152 s from the job's start to the race).
- **Read**: datagen run 44386256865's log: `Invalid pattern '/home/runner/work/ruststeve/ruststeve/../datagen-work'. Relative pathing '.' and '..' is not allowed.` from the cache's save step: the jar cache was never saved by any workflow, and `gh cache list` shows no `datagen-work` entry. The lanes' rust-cache used the datagen workflow's key (a debug build of other crates) with `save-if: false`.
- **Fix**: `DATAGEN_WORK` is `/home/runner/work/datagen-work` in every workflow (key `datagen-work-26.1.2-v2`); race.yml saves it from lane 0 on a miss. The lanes and `publish` use rust-cache key `race`, saved by lane 0 (a cache is written only when its key is new).
- **Proof**: `gh cache list` after race 45334668363: `datagen-work-26.1.2-v2` (115 MB) and `v0-rust-race-...` (150 MB). Race 45335753205 restored both: jar step 0 s, build 42 s, 77 s from the job's start to the race (129 s cold).
- **Limits**: a cache saved on a branch is restored only on that branch (and one saved on `main` everywhere): each agent's branch pays the cold start once. Workspace crates are not cached, so the build keeps about 40 s.

## Wall 4: a race could not be read until it ended

- **What happened**: case, ledger and senses were written when a bot's run ended; a snapshot was written when its stage was reached but left the runner only in the end-of-job artifact. A 12,000-tick race showed one progress line and nothing else for its whole length, and a lane that never finished (wall 1) left none of it.
- **Fix**: every 2,400 ticks (`PROGRESS_EVERY_TICKS`, 12,000 before) the bot rewrites its progress line and, beside it, `<bot>.ledger.txt`, `<bot>.senses.txt` and `<bot>.case.txt` (`bot::evidence`: copies taken on the loop thread, rendered and written on a thread of its own, each file whole by rename). The running case text starts `RUNNING` with the progress line and holds the story so far (`results::story`); the run's end writes the real case over it. The lane's loop looks every 30 s: a snapshot whose `snap.toml` is there (written last) is uploaded at once, named as `publish` names it; every 5 minutes (10 with more than 8 lanes) the progress lines and `evidence-lane-N.tar.zst` go up. The interval is a budget: the workflow token has 1,000 requests an hour for the repository and an upload with `--clobber` is about three.
- **Proof**: development race 45336625960 (portal start, c525230): the release held `progress-lane-0.txt` and `evidence-lane-0.tar.zst` at 06:01:20 UTC, about five minutes into the race and before it ended; the tarball's case text began `RUNNING`, with the stages, the longest tasks and the last two minutes, and the ledger had 144 lines to tick 4,760. The snapshot upload: see wall 7.
- **Read while racing**: `gh release download race-<id> -p 'evidence-lane-*' -p 'progress-lane-*'`, then `tar --zstd -xf evidence-lane-N.tar.zst`.

## Wall 5: a bot digging in place never ended `NoProgress`

- **What happened**: bots dug stairs and tunnels inside one 16-block cube for hours and ran to the cap.
- **Read**: `crates/bot/src/lib.rs` at ddae609, 716 to 745: any `Fact::Item` whose count rose reset the 24,000-tick clock. A dig yields a netherrack every few seconds.
- **Fix**: `bot::progress::Progress` (pure, unit-tested). Progress is a new stage, an item kind the run has not held, more of a goal item than the run ever held (up to the most any goal row asks for), or a 16-block cube not stood in before. Goal items are the items `config/goals.toml`'s rows name, with `[progress] items` of `config/race.toml` for a stage the table has no row for (7 blaze rods).
- **Proof**: unit tests: 2,000 digs of netherrack in one cube leave the clock at 24,000; a third blaze rod resets it, a rod lost and found again does not; the fourth cobblestone (3 asked) does not. In development race 45336625960 the progress line read `no progress for 1900 ticks` while the bot explored inside known cubes.
- **Not proven**: no race has ended `no-progress` on the new rule. A new item kind is progress once each, so a dig through varied rock still gets a few resets.

## Wall 6: no way to ask the planner again on a recorded world

- **Fix**: `replan`, a third binary of `crates/replay`, and a wider `why --blocks`. Both fold the recording to a moment from the last checkpoint before it (`replay::fold_to`). No planner code changed: `replan` uses `plan::path::{plan, Cells, neighbors}` and `Plan::step` as they are.
- **Commands** (on a runner, through the probe; the recording is another commit's, hence `--force`):

```bash
gh workflow run probe.yml -R BridgerB/ruststeve --ref <branch> -f timeout=30 -f command='gh run download 45194253932 -n race-45194253932-lane-5 -D lane && cargo build -q --release -p replay && ./target/release/replan lane/rs-l05-b00.rsr --at 25700 --route 61,33,95 --force > probe-out/route.txt 2>&1; ./target/release/replan lane/rs-l05-b00.rsr --at 25700 --steps 400 --goal race --force > probe-out/steps.txt 2>&1; ./target/release/why lane/rs-l05-b00.rsr --at 25700 --blocks 22,34,-19 --radius 15 --ys 29,36 --force > probe-out/slice.txt 2>&1'
gh run download <probe run id> -R BridgerB/ruststeve
```

- `replan <rsr> --at <tick> --route x,y,z [--range 2] [--nodes 6000]`: the route from the feet to within `--range` of the cell (0 is the cell itself): status, waypoints, cost, nodes; whether the feet's cell is a start and why not; when it did not arrive, the moves the route's end has and each cell beside it nearer the goal that no move enters, with the reason in the planner's terms (no floor and what is under, the feet or head cell's block and class, lava beside and where).
- `replan <rsr> --at <tick> --steps N [--goal race]`: a new planner (the recording does not hold one) stepped N times on that world, its notes and acts printed. Open loop: the body moves by the client's physics, no server answers a dig, so a dig that "finishes" leaves its block.
- `why <rsr> --at <tick> --blocks x,y,z --radius 15 --ys lo,hi`: a 31 by 31 slice a layer, short codes in the grid, every block's full name and count in the legend, `*` the centre, `@` the bot's feet.
- **Proof** (probe 45337136275, `rs-l05-b00.rsr` at tick 25,700):

```text
rs-l05-b00 at 25700 server ticks (129128 events), in minecraft:the_nether: the bot at (30.50, 31.00, -18.30), feet cell (30, 31, -19)
route from (30, 31, -19) to Near { pos: (61, 33, 95), range: 2.0 }, at most 6000 nodes: 128.8 blocks on the plane, 2 up
  the goal cell is nether_bricks, nether_bricks under it
  start: the feet cell (30, 31, -19) stands
  result: None, 0 waypoints, cost 0.0, 5 nodes expanded, from Some((30, 31, -19))
  moves from there: 1
    JumpUp to (30, 32, -20) (128.8 from the goal)
  cells beside it nearer the goal that no move enters:
    (31, 31, -19): the feet cell is netherrack (Solid); the head cell is netherrack (Solid)
    (30, 31, -18): no floor: air under it is Clear
```

  The route search dies after 5 nodes: at that tick the bot stands in its own cut, walled by netherrack, with one move (a jump up). The 400 steps print `stairs down toward the fortress at (61, 30, 95)`, three digs of (30, 31, -20) and `stairs failed: DigFailed after 0 stairs (last refusal: magma under (29, 29, -19); turn the heading)`. The slice's legend: netherrack 2,945, air 2,256, lava 1,524, magma_block 558, soul_soil 210, gravel 116, soul_sand 31, with lava filling y 31 north of z -24.

## Wall 7: a development start could become the frontier

- **What happened**: nothing yet; since 2026-11-23 made-up starts are allowed for development, and nothing stopped a snapshot taken in such a race from being raced as `snapshot=frontier` or written into `config/frontier.toml`.
- **Fix**: `bot::devstart` (pure, unit-tested). A start other than `snapshot`, `fresh` or `spawn` is a development start (a test holds those three to no kit and no place in `config/starts.toml`). A race from one, or resumed from a snapshot marked so, writes `dev = true` as the first line of each `snap.toml`; race.yml names such an asset `dev-snapshot-...` (in the lane's loop and in `publish`), and its restore step refuses `snapshot=frontier` when the snapshot is marked or its asset name starts `dev-`. A test fails CI when `config/frontier.toml` names a `dev-` asset.
- **Known gap**: the present frontier (`race-44678197603`, the `portal` start) and tonight's Nether snapshot were taken before the mark existed. The first is a development snapshot by this rule and carries no mark; the guard cannot see it.

## Wall 8: the insight job failed while another race ran

- **What happened**: race 45334668363 ended `failure` with every lane green: `insight / publish` stopped at `no assets to download`.
- **Cause**: `ingest.yml` downloads `batches-*` from every `race-*` release, and a race still running has its release (opened for progress) with no batches yet.
- **Fix**: such a release is left out with a line, and counted.
- **Proof**: race 45336625960's insight job passed with six other races running: seven lines `left out: race-... has no batches`.

## Wall 9: `brain-check` read whichever worktree built it last

- **Cause**: `crates/brain-check/src/main.rs` took the repository from `env!("CARGO_MANIFEST_DIR")`, baked in at build time; worktrees sharing one target directory share the binary.
- **Fix**: the root is the nearest directory at or above the working directory that holds `brain/` and `Cargo.toml`; outside any checkout, the build's own, as before. CI runs it from the root, unchanged.
- **The same hazard, not fixed**: with one target directory, cargo treats another worktree's build of a crate as fresh when that worktree's sources are older than the artifact. A commit hook here failed to compile `results` against another worktree's `observe`; touching the sources forces a rebuild. One target directory per worktree is the cure.
