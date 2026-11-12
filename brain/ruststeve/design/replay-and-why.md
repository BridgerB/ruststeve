---
title: Replay and why
description: How to record a run, verify it replays exactly (from the start and every checkpoint), export it as text, and ask why for the world at any moment; what each tool is for.
type: how-to
tags: [replay, why, recording, debugging]
aliases: [replay verify, replay export, why binary, --force, debug a run, fold a recording]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@219c53c
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
2. Verify: `cargo run -p replay -- verify <file>` folds every event and compares effects and checkpoints; it prints one line for the fold from the start and one per checkpoint, and fails on any mismatch.
3. Look: `cargo run -p replay -- export <file>` prints one record per line (packets as hex, controls as their fields, checkpoints as a summary) for grep and diff.
4. Ask: `cargo run -p replay --bin why -- <file> --at <server ticks>` (or `--event <n>`) folds to that moment, starting from the last checkpoint before it, and prints the dimension, the player's position, health and footing, the ten nearest entities with their health, the inventory and nearby sightings. The planner's branches print here once there is a planner (issue 09).

## Rules
- Run a recording at its own commit: check out the header's `commit` first. `replay` and `why` refuse another commit's file unless `--force`, because a fold with newer code drifts.
- A mismatch on a commit that should not have changed behaviour is nondeterminism or an unintended change; both are bugs ([[determinism-rules]]).

## Example
The first `why` asked of a 75 s recording showed a zombie 0.4 blocks from the idle bot at tick 600, health 13: the reason earlier idle sessions ended at health 0. Survival is issue 12.

## See also
- [[recordings]] - the format.
- [[replay-limits]] - open loop versus closed loop.
