---
title: Data out, and SurrealDB for insight
description: How ruststeve's results leave the bot (its own binary files as the source of truth, an embedded SurrealDB built from them for insight across runs, a read-only snapshot stream), why SurrealDB stays out of the core, and how steve reads it.
type: decision
tags: [decision, data, storage, surrealdb, frontend]
aliases: [no Postgres, SurrealDB, insight store, ingest, batch file, snapshot stream, steve front end, SurrealKV]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Rust only, backend only. No HTML, no viewer and no dashboard in ruststeve."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A read-only WebSocket or SSE stream of world snapshots, for a live view of one bot (JSON or MessagePack). Snapshots only, never commands."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#gh-pages: the results site and data/batches/<day>.ndjson, every batch since late October. It's the best record of what each commit did."
  - "ruststeve:FUNCTIONAL.md#The core has no I/O and is deterministic."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Stage 18 took about four weeks and never passed. It wasn't for lack of fixes (over 100 stage-18 PRs merged)."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[recording-format]]"
  - "[[observability]]"
  - "[[insight-store]]"
  - "[[measurement-rules]]"
---

# Data out, and SurrealDB for insight

## The choice (2026-11-11)
1. **Files are the source of truth.** On the runners, ruststeve writes its own binary files ([[recording-format]]): a recording per run (at least every failed one), a case file per ending (killer, first hit, task and its why), per-second senses, and one **batch file** per lane batch (every run's stage reached, ending and case signature, with commit, lane kind and start). They upload as workflow artifacts and release assets keyed by run id and commit.
2. **SurrealDB 4 is the insight store, built from the files.** A Rust `ingest` binary (its own crate, so the database never enters the core's build) loads batch and case files into an embedded SurrealDB with SurrealKV storage, versioned. It answers what a pile of files cannot answer cheaply:
   - **graph relations**: commit → run → stage reached; run → ending → killer; run → case → the task and why that was running; stage → checkpoint start;
   - **pre-computed views** that stay current as batches arrive: k/n per stage per commit and start, deaths by killer and stage, coverage per batch;
   - **time travel** on the versioned store: what the results looked like before a merge;
   - **live queries** for the front end when a new batch lands.
3. **steve is the front end** (Bridger, 2026-11-11). It runs as a SvelteKit app only, never the TypeScript bot, and reads SurrealDB through the JavaScript SDK's embedded Node engine (`@surrealdb/node`, `surrealkv://` path) or a SurrealDB server: checkpoints, stage progress, runs, case files. No ruststeve code in steve; the contract is the SurrealDB schema plus the snapshot-stream schema, both versioned files in this repo.
4. **Live view of one bot**: the read-only snapshot stream, snapshots only, never commands; not routed through the database.

## Why both, and what each is for
More data would not have unstuck clojurecraft's stage 18: it already kept case files, ledgers and recordings of every failed run. It stalled on structure: symptom fixes judged on 30–150 attempts, a sim that disagreed with real, resource-burning loops, and recordings folded on newer code ([[cc-stage18-lessons]]). So the two layers have different jobs:
- **The recording format is the debugger.** An exact, commit-pinned replay plus `why` shows what one bot believed and why it chose each move at the failing moment. That is how clojurecraft finally found its real causes, too late.
- **SurrealDB is the pattern finder.** Across thousands of runs: which killer dominates a stage on a commit, digs per block of progress per bot, whether a fix's code path ever fired. One query each, instead of folding recordings by hand.
- Neither replaces the design work that mattered most: progress invariants and sim-real parity ([[progress-invariants]], [[measurement-rules]]).

## Where SurrealDB is not used, and why
- **Not in the core** (world model, memory, the recipe graph, the planner): the core must be deterministic, replayable and free of I/O ([[sans-io-core]], [[determinism-rules]]). A database in the decision path breaks exact replay, and in a 50 ms tick it is the slowest thing in the loop. The tech tree is generated data walked by a pure function ([[cc-plan-and-make]]); memory is an in-core fact log ([[cc-world-memory]]).
- **Not as the recording format**: recordings must replay at their commit, byte for byte; they stay MessagePack files, and the database is rebuilt from them, never the other way round.

## What was rejected
- **Postgres** (the handoff's choice): Bridger does not want it.
- **No database at all**: files alone answer one run well and many runs badly; cross-run insight (which killer dominates stage 15 on which commit) is the database's job.
- **Old tooling** (steve's Postgres tables, clojurecraft's Babashka churn and gh-pages site, ruststeve v1's SQLite and Leptos dashboard): the point of the reset is to start fresh. steve is kept only as the front end.

## Settled (issue 07b)
- **Where the database lives**: a SurrealKV store built by `ingest` on a runner and published as the asset `insight.surrealkv.tar.zst` on the release `insight-latest`; steve downloads it and opens it embedded (no server, no account). SurrealDB Earth (hosted, live updates) stays Bridger's call if a download ever stops being enough ([[insight-store]]).
- **Version**: `surrealdb = "=4.0.1"` in Rust, `@surrealdb/node` 3.2.0 on `@surrealdb/node-native` 4.x in steve; the schema file records both.

## What would change the answer
Ingest or query time on the runners outgrowing an embedded store, or a need for live multi-writer updates: then a SurrealDB server, with only the connection string changed.

## See also
- [[observability]] - what is produced.
- [[measurement-rules]] - what the views must answer.
