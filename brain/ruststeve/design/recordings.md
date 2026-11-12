---
title: Recordings
description: What a ruststeve recording holds (header with commit, raw packets and ticks in, encoded effects out, checkpoints of the whole World), how a killed run still reads, and how verify proves a run replays exactly.
type: reference
tags: [recording, replay, determinism, format]
aliases: [Record, Header, Checkpoint, EventRecord, EffectRecord, Recorder, replay verify, .rsr]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@e25875e clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/record/src/lib.rs#    Controls(physics::Controls),"
  - "ruststeve:crates/record/src/lib.rs#pub enum Record {"
  - "ruststeve:crates/record/src/lib.rs#pub fn step(&mut self, e: &EventRecord) -> Result<Vec<EffectRecord>, String> {"
  - "ruststeve:crates/record/build.rs#println!(\"cargo:rustc-env=RUSTSTEVE_COMMIT={commit}\");"
  - "ruststeve:crates/bot/src/lib.rs#struct Run {"
  - "clojurecraft:src/clojurecraft/record.clj#\"Packet keys that are pure functions of the wire bytes, added on the reader thread. A"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[recording-format]]"
  - "[[replay-and-why]]"
  - "[[replay-limits]]"
  - "[[cc-record-replay]]"
---

# Recordings

Every run can be a file (`bot --record <path>`, `.rsr`) that replays exactly, offline, at its own commit. Old recordings are the regression suite, so the format grows by accretion and never breaks.

## Key files
- `crates/record/src/lib.rs`: `Header`, `Record`, `EventRecord`, `EffectRecord`, `Recorder`, `Recording`, `Folder`, `check_commit`.
- `crates/record/build.rs`: bakes the full commit sha and a dirty flag into every build.
- `crates/bot/src/lib.rs` (`Run::step`): writes each event before the step, its effects after, a checkpoint every 1,200 server ticks.

## What is in a file
A zstd stream of length-prefixed MessagePack records with named fields ([[recording-format]]):
1. **`Header`**: format version, commit, dirty, Minecraft version, protocol, bot name, server label, seed, target triple, previous segment.
2. **`Event`**: either a server packet **as it arrived** (the state it was read in, its id, its decompressed bytes; replay decodes it again, so chunk columns are never stored twice), a **tick** (the loop's clock and a fresh seed, every 50 ms: time and randomness are inputs, [[sync-core-and-time]]), or **controls** (what the body presses: forward, jump, sneak, yaw; [[physics]]), added by accretion in issue 06.
3. **`Effects`**: what that step produced, each packet as its encoded bytes.
4. **`Checkpoint`**: the whole `World` (serde on every world type; kinds as protocol ids) every 1,200 server ticks, a minute of game time.
5. `Note`.

## How verify works
`Folder` steps a fresh `World`, decoding each recorded packet in a decode state that moves exactly as the net reader's does (`State::after_receive`); `replay verify` compares each step's effects byte for byte with the recorded ones and the folded world with every checkpoint, then folds again from **each** checkpoint to the end. Measured: a 75 s local run, 7,227 events in 0.8 s, 0 mismatches from the start and from its checkpoint.

## Gotchas
- A run killed mid-write still reads: the reader stops at the last whole record, and the recorder flushes a zstd block at least once a second.
- A recording is folded only at its own commit; a newer fold drifts because the bot's own choices move it (lesson 4.5, [[cc-stage18-lessons]]). `check_commit` refuses another commit's file unless `--force`.
- Decode errors are not stepped, so they are not recorded either: the world never saw them.

## Limits
- About 3.4 MB for a minute of play; checkpoints carry every column. Sharing unchanged columns between checkpoints waits until size matters.

## See also
- [[replay-and-why]] - the tools.
- [[replay-limits]] - what a recording can and cannot test.
