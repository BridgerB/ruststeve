---
title: The insight store
description: How race results become the SurrealDB 4 insight store steve reads - the ingest crate (its own workspace), the schema file that is the whole contract, the k/n, deaths and endings views, how ingest.yml publishes the store to insight-latest after every race, and how to open it.
type: reference
tags: [insight, surrealdb, ingest, results, steve, contract]
aliases: [ingest, insight store, insight-latest, insight.surrealkv.tar.zst, insight-v1.surql, SurrealKV, kn view, deaths view, endings view, steve contract]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@3d86a51
sourceRefs:
  - "ruststeve:crates/ingest/src/main.rs#async fn add_batch(db: &Surreal<Db>, path: &Path, b: &Batch) -> Res<bool> {"
  - "ruststeve:crates/ingest/src/main.rs#fn absent_nulls(v: Value) -> Value {"
  - "ruststeve:crates/ingest/Cargo.toml#surrealdb = { version = \"=4.0.1\""
  - "ruststeve:schema/insight-v1.surql#DEFINE TABLE IF NOT EXISTS kn AS"
  - "ruststeve:.github/workflows/ingest.yml#gh release upload insight-latest insight.surrealkv.tar.zst --clobber"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[data-out]]"
  - "[[race-and-results]]"
  - "[[measurement-rules]]"
---

# The insight store

Issue 07b: the race's files folded into an embedded SurrealDB 4 store, the pattern finder across runs that [[data-out]] decided on. Files stay the truth; the store is built from them and never written back.

## Key files
- `crates/ingest/src/main.rs`: `ingest add | query | case`.
- `schema/insight-v1.surql`: the schema, compiled into ingest and the whole contract with steve.
- `.github/workflows/ingest.yml`: the crate's check, and the publish job race.yml calls.

## How it works
- **Its own workspace.** `crates/ingest` is excluded from the root workspace (its own `Cargo.lock`), so the core's and the root CI's builds never compile SurrealDB. `surrealdb = "=4.0.1"`, SurrealKV, opened `versioned`; a cold release build takes about 10 minutes.
- **What it holds** (namespace `ruststeve`, database `insight`): `commit`, `stage` (the ladder with ranks), `start`, `batch` (one per lane, `batch:⟨run/lane⟩`, coverage as numbers), `run` (one per bot run, `run:⟨run/bot⟩`, the signature fields plus the whole case file as an object), relations `commit->ran->run`, `run->played->start`, `run->in_batch->batch`, `run->reached->stage`, and `attempt` rows (one per counted run and stage: reached 1 or 0).
- **Views kept current by the database**: `kn` (k = sum of reached, n = count, per commit, start, source and stage), `deaths` (by killer and stage reached, per commit), `endings` (by signature, per commit and start).
- **Idempotent adds**: a batch already in the store is skipped, so each publish downloads the store from the release `insight-latest`, adds every `race-*` release's batches (only the new ones land) and replaces the asset `insight.surrealkv.tar.zst`. SurrealKV's versions keep what the store said before each add.
- **Opening it** (steve): unpack the asset, open `surrealkv://<dir>/insight?versioned=true` read-only with `@surrealdb/node` 3.2.0 on `@surrealdb/node-native` 4.x (its peer range includes ^4.0.0).
- Ids match the live stream's: a run is the race run id plus the bot name.

## Evidence
- The first runner race (56 bots, run 43181608927) and the local `table` lane ingested: `kn` gives 56/56 `in_world` and 0/56 above, the same as `results summary`; a second add skipped all 4 lanes; `ingest case` prints a run's case read back from the store.
- The crate's test folds a batch with a cap, a death and a cut run: k/n counts only counted runs, `deaths` holds the one killer, and the case reads back equal.

## Gotchas
- A JSON `null` is not SurrealDB's NONE: an `option<string>` field refuses it. Ingest drops null fields before writing (the test found it).
- SurrealKV holds a lock on its directory: one handle per process; the test shares one.
- `FLEXIBLE` comes after the type: `TYPE object FLEXIBLE`.

## Limits
- Only real races so far; sim batches (issue 08) land in the same tables with `source = "sim"`.
- steve's pages are steve's side; the contract is the schema file.

## See also
- [[race-and-results]] - the files it reads.
- [[data-out]] - why a database here and nowhere else.
