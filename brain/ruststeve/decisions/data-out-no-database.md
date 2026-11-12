---
title: Data out, with no database
description: How ruststeve's results leave the bot (its own binary files, uploaded from the runners, and a read-only snapshot stream) with no database and none of the old projects' tools, and what is still open about a front end.
type: decision
tags: [decision, data, storage, scope, frontend]
aliases: [no Postgres, no database, results files, batch file, snapshot stream, front end, backend only, no viewer]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Rust only, backend only. No HTML, no viewer and no dashboard in ruststeve."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A read-only WebSocket or SSE stream of world snapshots, for a live view of one bot (JSON or MessagePack). Snapshots only, never commands."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#gh-pages: the results site and data/batches/<day>.ndjson, every batch since late October. It's the best record of what each commit did."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[recording-format]]"
  - "[[observability]]"
  - "[[cc-observability]]"
---

# Data out, with no database

## The choice (recommended, 2026-11-11)
- **No database.** ruststeve writes files, in its own format ([[recording-format]]: MessagePack with named fields, zstd, a versioned commit-pinned header):
  - a **recording** of every run (kept for every failed run, at least);
  - a **case file** for every ending (killer, first hit, intent and its why) and the per-second senses, in the same format;
  - one **batch file** per lane batch: every run's stage reached, ending and case signature, with commit, lane kind and start. clojurecraft's best long-term record was exactly this, one data file per day of batches.
- **Where the files live**: produced on the runners and uploaded as workflow artifacts and release assets, keyed by run id and commit. A `results` binary folds batch files into k/n per stage, per commit, per start.
- **Live view**: a **read-only snapshot stream** (WebSocket or SSE) of one bot's world, snapshots only, never commands. Its schema is a versioned file in the repo; that file plus the batch-file schema is the whole contract with any front end.
- **ruststeve itself has no HTML, viewer or dashboard.**

## What was rejected
- **Postgres** (the handoff's first choice, because steve already read it): Bridger, 2026-11-11, "i do not want postgres". A database also adds a service to keep alive on every runner for data that is written once and read in batches.
- **Reusing the old projects' tooling** (steve's Postgres tables, clojurecraft's Babashka churn and gh-pages site, ruststeve v1's SQLite and Leptos dashboard): the point of the reset is to start from scratch.
- **SQLite or another embedded store**: the data is append-only and read as whole batches; files in the recording format do that with one format instead of two.

## Open
- **Which front end, if any**: steve, something new, or none for now. The snapshot stream and batch-file schemas are written so that the choice can wait. Bridger to decide.

## What would change the answer
Queries across many batches that a fold over files makes slow on a runner; even then, an index file beside the batches comes before a database.

## See also
- [[observability]] - what is produced.
- [[measurement-rules]] - what the batch files must answer.
