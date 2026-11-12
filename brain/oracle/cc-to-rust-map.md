---
title: clojurecraft to Rust, mechanism by mechanism
description: The correspondence between each clojurecraft mechanism and its ruststeve form, so a port maps responsibilities rather than files.
type: reference
tags: [oracle, correspondence, porting, boundary]
aliases: [Clojure to Rust map, idiom table, correspondence note, how to port]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Use this as a lookup while porting. Where the Rust column says \"not needed,\" ownership already provides what the Clojure construct was for."
  - "clojurecraft:src/clojurecraft/main.clj#(game/compose game/step plan/step))"
  - "clojurecraft:src/clojurecraft/intent.clj#:player/controls and effects updated. The set of kinds is open: a new kind is a defmethod in a"
related:
  - "[[oracle/_moc|Oracle]]"
  - "[[accents]]"
  - "[[domain-types]]"
---

# clojurecraft to Rust, mechanism by mechanism

A boundary note: each clojurecraft mechanism and what replaces it. Map by responsibility; a one-to-one file mapping is a smell.

| clojurecraft | ruststeve |
|---|---|
| one atom, `swap! world step event` | the tick loop owns `Brain`; `step(&mut self, &Input)` ([[sans-io-core]]) |
| `(compose game/step plan/step)` | one `step` that folds observations, then plans |
| events one at a time (`:packet`, `:tick`, `:go`) | one `Input` per tick holding that tick's observations |
| `:bot/effects` in the world, drained by the loop | `Output.commands` returned, each with a `Reason` |
| flat namespaced map, absent when unknown | typed structs; `Option` only in raw records, context types elsewhere ([[domain-types]]) |
| `model.clj` attribute list and specs | the `WorldModel` and `Plan` types themselves |
| `on-packet` multimethod on `[phase name]` | a typed packet enum matched exhaustively in the wire layer ([[wire-boundary-parsing]]) |
| `intent/run` multimethod on `:intent/kind` | `Task` enum, `step_task` match ([[phases-and-task-stack]]) |
| goal rows with `:goal/done?` and `:goal/act` names | a const table of rows whose predicate and act are enum variants |
| `pearls-rules.edn` + facts table | the shape (small `Rule` tables over named facts), not the rows ([[rule-tables]]) |
| `:time/now` ms and absolute ms deadlines | the clock on the tick event, deadlines against it; caps in server ticks ([[sync-core-and-time]]) |
| `:event/rand` per tick | a random seed on the tick event, recorded ([[cc-randomness-on-the-tick]]) |
| DataScript facts with time | an append-only fact log plus `BTreeMap` indexes ([[cc-world-memory]]) |
| `memo/by` identity memoisation | explicit caches keyed by input, rebuildable |
| EDN recordings, `verify` on sent packets | MessagePack + zstd recordings, commit-pinned, `verify` on whole outputs ([[recording-format]]) |
| `sim` server model | a `Body` implementation for closed-loop tests ([[cc-sim]]) |
| `clojure.spec` + `instrument` | newtypes, parse-at-the-boundary, `proptest` |
| core.async bounded channels | plain threads and bounded `sync_channel`s; no tokio in the core ([[sync-core-and-time]]) |
| `bb` tasks, Babashka tooling, gh-pages results site | Rust crates and binaries only; results as files, no database ([[just-ruststeve]], [[data-out-no-database]]) |
| `ex-info` thrown on bugs | `thiserror` enums; game outcomes as `Failure` values ([[error-kinds]]) |

## Gotchas
- clojurecraft's open multimethods let any namespace add a case; Rust's closed enums make the compiler list every place a new case touches. That is a deliberate trade ([[enums-for-closed-sets]]).

## See also
- [[cc-namespaces]] - the namespaces being mapped.
- [[accents]] - the Clojure accent to avoid.
