---
title: The parity table
description: How ruststeve checks the server model against a real server before any sim number counts - captured worlds, the 50 moments, running them in real and in the model, the outcome classes compared field by field, the stated tolerance, what the table found, and what it leaves out.
type: reference
tags: [parity, sim, measurement, capture, moments, real]
aliases: [parity table, parity report, moments, capture, captured world, landings, outcome class, tolerance, moments compare, parity.yml, simrace, simrace.yml]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@0801e33
sourceRefs:
  - "ruststeve:crates/bot/src/bin/capture.rs#fn capture(a: &Args) -> Result<(), String> {"
  - "ruststeve:crates/bot/src/bin/moments.rs#/// script saw, so when a peaceful heal lands (every 20 ticks) cannot flip it."
  - "ruststeve:crates/results/src/parity.rs#pub fn compare(real: &[MomentResult], sim: &[MomentResult]) -> Vec<Row> {"
  - "ruststeve:crates/bot/src/bin/simrace.rs#fn simrace(a: &Args) -> Result<bool, String> {"
  - "ruststeve:config/moments.toml#id = \"m00-0\""
  - "ruststeve:.github/workflows/parity.yml#./target/release/moments compare parity/real parity/sim --report parity/parity.md"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[server-model]]"
  - "[[measurement-rules]]"
  - "[[cc-stage18-lessons]]"
---

# The parity table

A sim number counts only after the model agrees with a real server at the same moments ([[measurement-rules]]): clojurecraft's sim said 30% where real said 3% for weeks ([[cc-stage18-lessons]]). Issue 08c builds the table for what the model models now; each later issue adds moments for its part.

## Key files
- `crates/bot/src/bin/capture.rs`, `src/captured.rs`: captured worlds.
- `crates/bot/src/bin/moments.rs` (`real`, `sim`, `compare`), `crates/results/src/parity.rs`.
- `config/landings.toml` (10 landings), `config/moments.toml` (50 moments).
- `.github/workflows/parity.yml`; `crates/bot/src/bin/simrace.rs` and `simrace.yml` for sim races.

## How it works
- **Capture**: a spectator joins a fresh server on the race seed, is teleported over each landing by RCON, reports its position once (the server re-centres chunks only on a reported move) and keeps every `level_chunk_with_light` body within 4 columns: one file per landing, byte for byte. A spectator changes nothing, so the terrain is what bots meet.
- **A moment**: a landing, a start offset (the start stands on the captured surface there), a script (`still`, `straight:<yaw>`, `jump:<yaw>`), 160 ticks. Five per landing, six blocks apart, each walking away from the centre (vanilla players push each other).
- **Real**: the same fresh server, peaceful, bots in waves of 10, each with a fresh name (the server saves a player's place and health by name); RCON gives water breathing and teleports to the start; the script begins after 20 physics ticks with the ground loaded and no teleport since.
- **Sim**: the model in the captured world, the bot spawning at the start, in lockstep, the same settle and script.
- **The outcome class**, compared field by field: the final block cell, on the ground, in water, corrected (a teleport after the script began), and the lowest health during the script (full, hurt, dead).
- **The stated tolerance**: at least 48 of 50 moments match in every field. A mismatch is a model or harness bug until shown otherwise, never tuned away.
- **simrace**: lanes of lockstep runs in generated worlds, the same case and batch files with `source = sim`, released as `race-sim-<run>` so the insight store keeps sim beside real, never mixed.

## What the table found
The first table matched 19 of 50. Each cause was named from a recording, then fixed:
1. **A deadlock in the bot**: teleported far from its loaded columns, a body with no ground ran no physics, sent no move, so the server never re-centred its chunks. The world now sends its once-a-second status even without ground ([[physics]]).
2. **Peaceful regeneration** was missing from the model (`ServerPlayer.tickRegeneration`: 1 every 20 ticks below full).
3. **Settle jitter**: the settle before a script counted server ticks, which the bot learns 20 at a time, and began before the start's teleport landed; now 20 physics ticks with ground loaded since the last teleport, the same in real and lockstep.
4. **Drowning while waiting**: real bots sat under water while their chunks came; the model has no air supply yet, so the real side breathes (below).
5. **Saved players**: a reused bot name rejoined where the last run left it, hurt.
Then: **50 of 50** locally.

## Limits
- No mobs (peaceful) and no air or drowning yet: the real side gives water breathing. Both join the table with issue 12 (survival), as rows, before any sim number about them counts.
- Moments test movement, falls and the server's corrections in the terrain the bot meets; digging, items and windows add moments with their issues.

## See also
- [[server-model]] - what the model models.
- [[measurement-rules]] - why the table gates sim numbers.
