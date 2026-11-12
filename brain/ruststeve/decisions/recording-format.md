---
title: The recording format
description: Why recordings are a compact, self-describing binary format (MessagePack with named fields, zstd, a versioned commit-pinned header) instead of JSON Lines, and how a human still reads one.
type: decision
tags: [decision, recording, format, replay]
aliases: [not JSON, MessagePack, zstd, rmp-serde, bincode, recording header, commit-pinned replay, replay export]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Record (Event, Vec<Effect>) with serde + bincode or MessagePack, compressed with zstd."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Pin the commit in the file's header and refuse to replay with different code unless asked (lesson 4.5)."
  - "ruststeve:FUNCTIONAL.md#Start human-readable. Write recordings as JSON Lines: one Record per line, a header line first with the schema version, seed, config hash, git commit and Minecraft version."
  - "clojurecraft:src/clojurecraft/record.clj#\"Packet keys that are pure functions of the wire bytes, added on the reader thread. A"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[recordings]]"
  - "[[cc-record-replay]]"
---

# The recording format

## The choice
A recording is one zstd-compressed stream of length-delimited **MessagePack** records, encoded by serde with **named fields** (maps, not positional arrays). The first record is a header: format version, git commit, Minecraft version, seed, config and its hash, and the previous segment on a reconnect. Then the run's inputs and outputs, and periodic checkpoints.

- **Commit-pinned**: `replay` refuses a file from another commit unless forced; a new function is evaluated as a pure function over the worlds the old commit folded, never by refolding with new code (lesson 4.5 of the handoff).
- **Readable on demand**: `replay export <file>` prints a recording as text (one record per line) for grep and diff; the binary file is the source of truth.
- **Grows, never breaks**: new fields carry `#[serde(default)]`; new variants are added, never renamed. Named fields make that work on old files.
- Derived values are not recorded: like clojurecraft, keep the chunk bytes and derive the decoded column again on replay.

## What was rejected
- **JSON Lines** (`FUNCTIONAL.md`): Bridger, 2026-11-11: no JSON, "it should be a really nice format". JSON is large, slow, and loses float bits unless `float_roundtrip` is on.
- **bincode or postcard** (the handoff's other option, and the guide's later one): positional and not self-describing, so adding a field to a struct breaks every older recording. Recordings are the regression suite, so that is disqualifying.
- **A custom format**: no reason to own one; serde gives MessagePack for free.

## What would change the answer
Measured size or speed: if replays on the runners are bound by decoding MessagePack, a positional format behind a versioned upcaster becomes worth its cost.

## See also
- [[recordings]] - what a recording holds and how `verify` works.
- [[cc-record-replay]] - clojurecraft's EDN recordings.
