---
title: Recordings
description: What a recording file holds (header, Input/Output records, checkpoints), how it evolves without breaking old files, and how verify proves determinism.
type: reference
tags: [recording, replay, determinism, format]
aliases: [Record, Header, Checkpoint, replay verify, golden recordings]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#One JSON object per line, header first. Write a Checkpoint every 1,200 ticks (one minute of game time) so replay can start near any tick without folding from spawn."
  - "ruststeve:FUNCTIONAL.md#Run it in CI on every recording in recordings/golden/, twice."
  - "ruststeve:FUNCTIONAL.md#This matters most for the event log, because old recordings are your regression suite."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[replay-limits]]"
  - "[[determinism-rules]]"
  - "[[cc-record-replay]]"
---

# Recordings

Every run writes a recording: enough to replay it exactly, offline, with no server. Old recordings are the regression suite, so the format grows by accretion and never breaks.

## Key files
- `FUNCTIONAL.md`, sections 3.13, 5.5, 9.2 and 9.3.

## How it works
- **Format**: MessagePack with named fields, zstd, a versioned commit-pinned header; not JSON ([[recording-format]]). `Record` is `Input | Output | Checkpoint | Note`; `replay export` prints one as text.
- **Header**: `schema_version`, `seed`, `config` and `config_hash`, `git_commit`, `minecraft_version`, `target_triple`, `previous_segment` (a reconnect chains segments).
- **Checkpoint** every 1,200 ticks (a minute of game time), so replay can start near any tick. `Brain` must therefore serialize, one more reason to keep it plain data.
- **Verify**: build `Brain::new(header.config, header.seed)`, step every recorded input, and compare each output with the recorded one; the first difference is a `Divergence { tick, expected, actual }`. CI runs it over `recordings/golden/` twice, and also checks that starting from any checkpoint gives the same outputs as replaying from the start.
- **Accretion**: add variants and fields, never rename or repurpose; new fields get `#[serde(default)]`; a changed meaning is `ObservationV2` plus an upcaster.

## Gotchas
- `verify` failing on a commit that was not meant to change behaviour means nondeterminism or an unintended change; both are bugs.
- A recording from last week that no longer loads is the smell; the schema broke instead of growing.
- A recording is valid only against the Minecraft version in its header, and it is folded only at its own commit: a newer fold drifts, because the bot's own choices move it (clojurecraft's lesson 4.5, [[cc-stage18-lessons]]).

## See also
- [[replay-limits]] - what a recording can and cannot test.
- [[incident-to-regression]] - how a stuck run becomes a recording in the suite.
- [[cc-record-replay]] - clojurecraft records EDN events today.
