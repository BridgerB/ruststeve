---
title: Seeds and the relay in race.yml
description: How race.yml races several worlds (the seed input, lanes per seed, the lane-to-seed map and each snapshot's seed line), where each seed's frontier lives, and how the relay chains legs so every seed climbs the ladder from its own furthest snapshot until it wins or stalls.
type: reference
tags: [race, seeds, relay, frontier, snapshots, workflow]
aliases: [seed input, seed=all, relay mode, relay=true, legs, relay_from, relay-<seed>.toml, lanes.toml, seed-relay, per-seed frontier, config/frontier/, stalled seed]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@0689e5f
sourceRefs:
  - "ruststeve:crates/xtask/src/bin/seed-relay.rs#fn plan(ask: &Ask, config: &Path, from: Option<&Path>) -> Result<Vec<Lane>, String> {"
  - "ruststeve:crates/xtask/src/bin/seed-relay.rs#fn advance(seed: &str, prev: Option<Relay>, found: &[Found], run: &str) -> Relay {"
  - "ruststeve:crates/xtask/src/bin/seed-relay.rs#fn best(found: &[Found]) -> Option<&Found> {"
  - "ruststeve:.github/workflows/race.yml#include: ${{ fromJSON(needs.lanes.outputs.matrix) }}"
  - "ruststeve:.github/workflows/race.yml#gh workflow run race.yml --ref \"$REF\""
  - "ruststeve:.github/workflows/race.yml#if ! grep -q '^seed = ' \"$d/snap.toml\"; then"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[race-and-results]]"
  - "[[one-bot-first]]"
  - "[[ci-on-runners]]"
---

# Seeds and the relay in race.yml

Issue 69: the win came from one lineage of one world, so every later issue is judged on several. race.yml gained a seed input and a relay mode instead of a new workflow, because `workflow_dispatch` only sees workflow files on main, and race.yml is there; a dispatch with `--ref` runs the branch's version and inputs.

## Key files
- `crates/xtask/src/bin/seed-relay.rs`: every choice the workflow makes about seeds (`lanes`, `record`). The shell only moves files.
- `.github/workflows/race.yml`: the `lanes` job (the matrix), the lane's snapshot restore and server start, `publish`, and the `relay` job.
- `config/seeds.toml` and `xtask seeds` / `xtask seed <name>` (the list and each value, owned by the seeds work); `config/frontier.toml`; `config/frontier/<seed>.toml`.

## How it works
- **The seed input.** Empty means config/race.toml's seed, which is today's race: no `--seed` flag goes to the server. `<name>`, `a,b` or `all` pick from `xtask seeds`. The first name is the race seed (config/seeds.toml's test holds it).
- **Lanes per seed.** `lanes` is per seed, and the matrix is seeds × lanes. The lane number runs on through the seeds (0 to n-1 for the first seed, then on), so bot names and asset names stay unique and keep their shape. The total must be 20 or fewer.
- **Each lane's row** (`strategy.matrix.include`): `lane`, `seed` (the job's `SEED`), `start`, `snapshot` (`tag/asset` or empty) and `source` (`frontier`, `relay`, `given` or empty). The `lanes` job uploads `lanes.toml` (the same rows) to the release, and publish adds it to the summary. Each lane's line in the job summary names its seed and start.
- **Starts on another seed.** Every start but `spawn` is a place on the race seed's world (config/starts.toml), so another seed with no snapshot is refused anything else. When `snapshot` resolves to nothing, the `snapshot` start becomes `spawn`.
- **Per-seed frontier.** `snapshot=frontier` reads config/frontier.toml for the race seed and config/frontier/<seed>.toml for the others (same keys). A seed with no file starts from spawn. A `tag/asset` snapshot is one world, so it is refused with more than one seed.
- **A snapshot knows its seed.** Before a lane packs a snapshot, it prepends `seed = "<name>"` to `snap.toml`. Appending would put the key inside the `[items]` table, the same reason `bot::devstart::mark_dev` prepends. The restore step refuses a snapshot whose seed line names another seed. A frontier or relay snapshot is also refused when it is marked dev or has a `kind`, as the frontier always was.

## The relay
- Its inputs: `relay=true`, `legs` (the most legs), `leg` (this leg, 1 by default) and `relay_from` (the previous leg's release tag, set by the relay).
- **A leg.** The `lanes` job downloads `relay-*.toml` from `relay_from`. Each seed still going resumes, with the `snapshot` start, from the snapshot its file names. A seed marked `stalled` or `won` gets no lanes.
- **After publish**, publish uploads every lane's `snap.toml` as a small artifact (`race-<run>-snaps`). The `relay` job then runs `seed-relay record`, which writes `relay-<seed>.toml` for every seed of the input to the release, along with `relay.md` (the leg's table, also in the job summary):
  ```toml
  seed = "ocean"
  stage = "nether"        # the furthest so far; "spawn" before any snapshot
  tag = "race-<run>"      # where that snapshot lives (empty at spawn)
  asset = "snapshot-nether-<run>-race-<run>-lane-3.tar.zst"
  stalled = false         # this leg added no stage: the seed leaves, and stage is its stall
  won = false             # stage is win: the seed leaves
  legs = ["race-<run1>", "race-<run2>"]
  ```
- **The leg's best** is the seed's furthest stage by the ladder's order (`world::Stage`'s `Ord`). Between equal stages, the one reached in fewer server ticks wins, and then the lowest lane, so a rerun picks the same. Development snapshots, snapshots with a `kind` (`at-`, `stuck-`) and directories that are not stage names never count.
- **Moving on.** A stage above the previous leg's moves the seed on. Anything else stalls the seed and keeps its old snapshot. A seed already out is carried unchanged, so the last leg's release holds every seed's state.
- **The next leg.** While any seed is still going and `leg < legs`, the job dispatches race.yml on the same ref (`actions: write`, the workflow token, as the marathon does) with `leg + 1`, `relay_from=race-<this run>` and every other input carried over.

## Gotchas
- A relay leg's lanes job builds xtask (with the race cache), which takes a minute or two before the matrix exists.
- A `relay_from` release without a file for a selected seed fails the `lanes` job by name. Keep `seed` the same across legs, which the dispatch does.
- The frontier files are moved by hand (by the lead) after a relay. The relay never commits.

## See also
- [[race-and-results]]: the race, its snapshots and the frontier.
- [[one-bot-first]]: why every attempt resumes from the furthest snapshot.
- [[ci-on-runners]]: the runners and the race's caches.
