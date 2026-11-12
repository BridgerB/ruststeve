---
title: CI and the probe on the runners
description: How code runs on GitHub Actions (the ci workflow's parallel jobs, the cache rule, the env choices and why, the probe for one-off questions, the core-crate boundary check) and the Rust-on-Actions practice behind it.
type: how-to
tags: [ci, runners, github-actions, probe, caching]
aliases: [ci.yml, probe.yml, how to run code, rust-cache, nextest, workflow_dispatch, deps-boundary, xtask]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@b4d3e95
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

## Running code on a runner (the probe)
```bash
gh workflow run probe.yml -R BridgerB/ruststeve --ref <branch> -f command='cargo nextest run -p world'
gh run list -R BridgerB/ruststeve --workflow probe.yml --limit 1     # find the run id
gh run download <run id> -R BridgerB/ruststeve                      # probe.log and probe-out/
```
The command arrives through an environment variable, never interpolated into the script. It runs under a timeout (default 60 minutes); anything written to `probe-out/` is uploaded with `probe.log`.

## Gotchas
- **`workflow_dispatch` only works for a workflow file on the default branch.** That is why the work moved to `main` ([[just-ruststeve]]). `--ref` then chooses which branch's code runs.
- A new workflow file added on a branch cannot be dispatched until it is merged into `main`; a push-triggered workflow runs from any branch that has it.
- `brain-check --strict` fails when a source checkout is missing, so CI cannot pass by skipping the sibling repositories; locally, missing checkouts are only counted as unchecked.

## Sources outside the repository
Practice followed here: corrode.dev, "Tips for faster Rust CI builds" (rust-cache, `CARGO_INCREMENTAL=0`, no test debug info, nextest, `--locked`); the `Swatinem/rust-cache` README (`shared-key`, `save-if`); the `taiki-e/install-action` README; GitHub's "Manually run a workflow" documentation (the default-branch rule).

## See also
- [[runners-not-the-mac]] - why code runs here.
- [[workspace-crates]] - the crates the boundary protects.
