---
title: Replay and why
description: How to record a run, verify it replays exactly (one fold checked at every checkpoint), export it as text, ask why for the world at any moment, print a slice of its blocks and ask the planner again there (replan); what each tool is for.
type: how-to
tags: [replay, why, recording, debugging]
aliases: [replay verify, replay export, why binary, replan, replan binary, --route, --steps, --blocks, --radius, block slice, --force, debug a run, fold a recording]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@a5ae696
sourceRefs:
  - "ruststeve:crates/replay/src/bin/replay.rs#fn verify(path: &Path, force: bool) -> Result<(), String> {"
  - "ruststeve:crates/replay/src/bin/why.rs#fn print_world(w: &World, events: u64) {"
  - "ruststeve:crates/record/src/lib.rs#pub fn check_commit(header: &Header, own: &str, force: bool) -> Result<(), String> {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[recordings]]"
  - "[[incident-to-regression]]"
---

# Replay and why

## Steps (in `nix develop`)
1. Record: `cargo run -p bot -- --seconds 120 --record target/recordings/<name>.rsr` against `xtask server` ([[connection]]).
2. Verify: `cargo run -p replay -- verify <file>` folds every event and compares effects and checkpoints; it prints one line for the fold from the start and one for each of two refolds, and fails on any mismatch (`replay::verify`, below).
3. Look: `cargo run -p replay -- export <file>` prints one record per line (packets as hex, controls as their fields, checkpoints as a summary) for grep and diff.
4. Ask: `cargo run -p replay --bin why -- <file> --at <server ticks>` (or `--event <n>`) folds to that moment, starting from the last checkpoint before it, and prints the dimension, the player's position, health and footing, the ten nearest entities with their health, the inventory and nearby sightings. The planner's branches print here once there is a planner (issue 09).

## Verify is linear (issue 16, `docs/problems/race-tooling.md` wall 2)
- One fold from the start compares every step's effects and every checkpoint's world. A checkpoint that matches proves a fold from it would go as the running one does, so only the middle checkpoint and the last are folded again (they prove `Folder::from_checkpoint`). The first version folded again from every checkpoint: a three-hour recording has 180, and the lane's replay took most of an hour.
- A run resumed from a snapshot starts with the snapshot's memory, which no event carries: the bot writes that world as a checkpoint before the first event and `Folder::take_start` begins the fold from it. An older resumed recording, whose first checkpoint differs from the fold in the memory alone, is taken as the truth and said so, not counted a mismatch.
- A checkpoint that differs is counted once: the fold goes on from the recorded world.

## Asking the planner again: replan (issue 16, wall 6)
- `cargo run -q --release -p replay --bin replan -- <file.rsr> --at <ticks> --route x,y,z [--range 2] [--nodes 6000] [--force]`: `plan::path::plan` from the bot's feet to within `--range` of the cell (0 is the cell itself): how it ended, waypoints, cost, nodes; whether the feet's cell is a start and why not; when it did not arrive, the moves the route's end has and each cell beside it nearer the goal that no move enters, with the reason (no floor and what is under, the block and class in the feet or head cell, lava beside and where).
- `... --bin replan -- <file.rsr> --at <ticks> --steps N [--goal race] [--force]`: a new planner (a recording does not hold one; a `.rsm` moment does) stepped N times on that world, its notes and acts printed. Open loop ([[replay-limits]]): the body moves by the client's physics, no server answers a dig.
- `... --bin why -- <file.rsr> --at <ticks> --blocks x,y,z --radius 15 --ys lo,hi [--force]`: a 31 by 31 slice a layer; short codes in the grid, every block's full name and count in the legend, `*` the centre, `@` the bot's feet. Without `--radius` and `--ys`: 3 each way, 2 under to 3 over.
- These print blocks the bot had loaded, seen or not: tools for reading a run, never an input to the bot.

## Rules
- Run a recording at its own commit: check out the header's `commit` first. `replay` and `why` refuse another commit's file unless `--force`, because a fold with newer code drifts.
- A mismatch on a commit that should not have changed behaviour is nondeterminism or an unintended change; both are bugs ([[determinism-rules]]).

## Example
The first `why` asked of a 75 s recording showed a zombie 0.4 blocks from the idle bot at tick 600, health 13: the reason earlier idle sessions ended at health 0. Survival is issue 12.

## The planner's decisions
Since issue 09a every planner decision (a task started, ended or failed, with why) is written into the recording as a note; `why` prints the last ones up to the moment ([[planner]]).

## See also
- [[recordings]] - the format.
- [[replay-limits]] - open loop versus closed loop.
