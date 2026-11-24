---
title: CI and the probe on the runners
description: How code runs on GitHub Actions (the ci workflow's parallel jobs, the cache rule, the env choices and why, the probe for one-off questions, the core-crate boundary check) and the Rust-on-Actions practice behind it.
type: how-to
tags: [ci, runners, github-actions, probe, caching]
aliases: [ci.yml, probe.yml, how to run code, rust-cache, nextest, workflow_dispatch, deps-boundary, xtask]
status: verified
lastUpdated: 2026-11-24
verifiedAgainst: ruststeve@adfcb00
sourceRefs:
  - "ruststeve:.github/workflows/ci.yml## Incremental compilation only adds overhead and cache churn on a fresh runner."
  - "ruststeve:.github/workflows/ci.yml## Only main writes the cache; branches and PRs restore it."
  - "ruststeve:.github/workflows/ci.yml#- run: cargo nextest run --workspace --locked --no-tests=pass"
  - "ruststeve:.github/workflows/ci.yml#- run: cargo run -q --locked -p brain-check -- --strict"
  - "ruststeve:.github/workflows/probe.yml## workflow_dispatch needs this file on the default branch (main); --ref picks the code to run."
  - "ruststeve:crates/xtask/src/main.rs#\"data\", \"protocol\", \"world\", \"physics\", \"memory\", \"plan\", \"sim\","
  - "ruststeve:rust-toolchain.toml#channel = \"1.96.0\""
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[runners-not-the-mac]]"
  - "[[workspace-crates]]"
---

# CI and the probe on the runners

The Mac never runs code ([[runners-not-the-mac]]). Two workflows are how it runs: `ci` on every push and PR, and `probe` for any one-off question.

## Key files
- `.github/workflows/ci.yml`, `.github/workflows/probe.yml`; the race is `race.yml` ([[race-and-results]]); ci.yml's `worlds` job walks the bot through 1,000 generated worlds against the server model, in release ([[server-model]]); its `walk` job runs issue 09's landing measure and the CPU budget ([[planner]])
- `rust-toolchain.toml`: the one pinned toolchain (1.96.0, rustfmt, clippy); `rustup show` on the runner installs it, so no toolchain action is needed.
- `crates/xtask`: repository automation: `deps-boundary` (CI), `commit-msg` (the git hook), `server` (a local vanilla server, [[connection]]).

## How it works
- **Triggers**: `ci` runs on pushes to `main` and `issue-*` branches, on PRs into `main`, and by hand. A newer push to the same ref cancels the older run (`concurrency`).
- **Jobs, in parallel**: `fmt` (`cargo fmt --all --check`), `clippy` (`--workspace --all-targets --locked -- -D warnings`), `test` (`cargo nextest run --workspace --locked --no-tests=pass`), `brain` (`brain-check --strict` with the sibling repositories fetched at their pins from `brain/sources`, and full history for `ruststeve-v1`), `boundary` (`xtask deps-boundary`), and `ci-ok`, one stable check that fails if any job did.
- **Cache**: `Swatinem/rust-cache@v2` with one `shared-key`; only `main` saves it, branches and PRs restore it, so a branch never evicts `main`'s warm cache.
- **Environment**: `CARGO_INCREMENTAL=0` (incremental compilation only adds overhead and cache churn on a fresh runner), `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0` (smaller `target/`, smaller cache). `-D warnings` is passed to clippy only, never through `RUSTFLAGS`, which would change every cache key.
- **nextest** is installed as a prebuilt binary by `taiki-e/install-action@v2`; it runs tests in parallel processes and reports per test.
- **The boundary**: `xtask deps-boundary` runs `cargo tree -e normal` for each core crate that exists (`data`, `protocol`, `world`, `physics`, `memory`, `plan`, `sim`) and fails if any depends on an async runtime, an I/O library, SurrealDB, or our own `net`, `rcon`, `record`, `ingest`.

## The race's caches (issue 16, `docs/problems/race-tooling.md` wall 3)
- **The server jar** lives at `/home/runner/work/datagen-work` (`DATAGEN_WORK`, key `datagen-work-26.1.2-v2`) in every workflow. The path must be absolute with no `..`: `actions/cache` refuses to save one with relative parts ("Relative pathing '.' and '..' is not allowed"), so the first path (`workspace/../datagen-work`) was never saved by any run and every lane fetched the jar. race.yml saves it from lane 0 on a miss.
- **The race's build** has its own rust-cache key (`shared-key: race`), saved by lane 0 and restored by the other lanes, by `publish` and by later races of the branch. Measured: 152 s from a lane's start to its race before, 129 s cold, 77 s warm (jar 0 s, build 42 s: workspace crates are not cached).
- **A cache is per branch**: one saved on a branch is restored there only; one saved on `main` everywhere. A new branch pays the cold start once.

## Running code on a runner (the probe)
```bash
gh workflow run probe.yml -R BridgerB/ruststeve --ref <branch> -f command='cargo nextest run -p world'
gh run list -R BridgerB/ruststeve --workflow probe.yml --limit 1     # find the run id
gh run download <run id> -R BridgerB/ruststeve                      # probe.log and probe-out/
```
The command arrives through an environment variable, never interpolated into the script. It runs under a timeout (default 60 minutes); anything written to `probe-out/` is uploaded with `probe.log`.

## A probe with the race's server (issue 15)
- `-f server=true` starts the race config's own server (the race seed, RCON on, a password for that run only) before the command; the `rcon` binary (`cargo run -q -p rcon -- "<command>" ...`) sends commands and prints each reply. First use: `locate structure minecraft:fortress` from seven points in the Nether (probe 44386934210), setup data for the `nether` start that the bot never reads.

## A probe on a recording
`replay`'s binaries read a race's recording on a runner: `why` (the world at a moment, a block slice with `--blocks x,y,z --radius 15 --ys lo,hi`), `replan` (the route or the planner's steps asked again there) and `replay verify`. The command lines are in [[replay-and-why]].

## Waiting on a run (the shared API limit)
Every `gh` call of every agent and session draws on one token's 5,000 requests an hour. On 2026-11-23 nine agents' `gh run watch` and release polls spent it in an hour and every call failed 403 until the reset. The rule: never `gh run watch`, never poll in a loop; ask once every 4 minutes or more with one request: the MCP tool `run_done`, or `gh api repos/BridgerB/ruststeve/actions/runs/<id> --jq '"\(.status) \(.conclusion)"'`. `gh api rate_limit` is free.

## Gotchas
- **`workflow_dispatch` only works for a workflow file on the default branch.** That is why the work moved to `main` ([[just-ruststeve]]). `--ref` then chooses which branch's code runs.
- A new workflow file added on a branch cannot be dispatched until it is merged into `main`; a push-triggered workflow runs from any branch that has it.
- **One target directory shared by several worktrees is a trap**: cargo takes another worktree's build of a crate as fresh when this worktree's sources are older than the artifact (a hook here compiled `results` against another worktree's `observe`), and a binary that bakes in `CARGO_MANIFEST_DIR` reads the wrong tree. `brain-check` now takes its root from the working directory; the rest is cured only by a target directory per worktree.
- `brain-check --strict` fails when a source checkout is missing, so CI cannot pass by skipping the sibling repositories; locally, missing checkouts are only counted as unchecked.

## Sources outside the repository
Practice followed here: corrode.dev, "Tips for faster Rust CI builds" (rust-cache, `CARGO_INCREMENTAL=0`, no test debug info, nextest, `--locked`); the `Swatinem/rust-cache` README (`shared-key`, `save-if`); the `taiki-e/install-action` README; GitHub's "Manually run a workflow" documentation (the default-branch rule).

## See also
- [[runners-not-the-mac]] - why code runs here.
- [[workspace-crates]] - the crates the boundary protects.
