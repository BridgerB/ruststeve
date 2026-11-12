---
title: Recording and replay in clojurecraft
description: How clojurecraft records a run (each event and the effects it produced, as EDN lines), what replay and verify check, and what ruststeve's recordings add.
type: reference
tags: [oracle, recording, replay, verify]
aliases: [record.clj, tap, run.edn, verify, accretes?, replay]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/record.clj#\"Every input is a value, so a run is a file. `tap` writes each event as one EDN line and,"
  - "clojurecraft:src/clojurecraft/record.clj#\"Packet keys that are pure functions of the wire bytes, added on the reader thread. A"
  - "clojurecraft:src/clojurecraft/record.clj#\"The effects the server saw: the :send ones. A log line reworded since the recording is not"
  - "clojurecraft:src/clojurecraft/record.clj#\"Does later hold every fact earlier holds? Maps compare key by key, so attributes added since"
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[recordings]]"
  - "[[cc-sim]]"
---

# Recording and replay in clojurecraft

Every input is a value, so a run is a file.

## Key files
- `src/clojurecraft/record.clj`: `tap!`, `events`, `effects`, `replay`, `verify`, `header`, `accretes?`.
- `src/clojurecraft/replay.clj`: `clojure -M:replay run.edn` folds the whole bot over a recording and prints RESULT, no server.

## How it works
- `tap` writes each event as one EDN line, then the effects that event produced as one `{:record/effects [...]}` line. The first line may be a header `{:record/commit sha}`. Byte arrays print as `#clojurecraft/bytes "base64"`.
- **Derived keys are dropped**: a decoded chunk column is a pure function of the wire bytes, so the recording keeps the bytes and the reducer derives the column again on replay.
- `replay` folds a reducer over the events from an initial world.
- `verify` folds again and compares, event by event, the `:send` effects produced with those recorded; the first mismatch reports the index, the event, both packet lists and a slice of the world. Log lines are ignored, so a reworded log is not a changed run.
- `accretes?` holds that a later value keeps every fact of an earlier one (new keys allowed, changed or missing ones not).

## For ruststeve
- clojurecraft records *events* (one per reducer call) and checks only sent packets. ruststeve records whole `Input`s and `Output`s and compares them exactly, including `BrainEvent`s ([[recordings]]).
- clojurecraft records the wire bytes of chunks; ruststeve's `Input` holds `Observation`s, so a chunk observation must carry enough (the raw section data, or a decoded value) for the core's fold to be exact.
- Checkpoints are new in ruststeve; clojurecraft instead keeps periodic *moments* in sim runs and real snapshots for its judge ([[cc-sim]]).

## See also
- [[recordings]] - the Rust format.
- [[cc-sim]] - snapshots and the judge.
