---
title: Workspace crates
description: Which crates and binaries the workspace has (the handoff's layout), which may do I/O, and how the boundary is enforced.
type: reference
tags: [crates, layout, architecture]
aliases: [crate layout, protocol crate, world crate, plan crate, sim crate, net crate, bins, bot-core]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve@c87ac55
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#protocol/    packet specs as data, decode/encode, state transitions; no I/O"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Crate boundaries: core crates (protocol, world, plan, sim) have no I/O dependencies at all. Only net, rcon, record and the binaries touch sockets and files."
  - "ruststeve:FUNCTIONAL.md#A CI check that the core has no I/O dependencies:"
  - "ruststeve:crates/brain-check/src/main.rs#//! The brain's integrity checker."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[determinism-rules]]"
  - "[[own-protocol-not-azalea]]"
  - "[[data-out-no-database]]"
---

# Workspace crates

One cargo workspace (the root `Cargo.toml` already exists, `members = ["crates/*"]`). The layout is the handoff's, which supersedes `FUNCTIONAL.md`'s `bot-types`/`bot-core`/`bot-shell` split; the guide's boundary rules still apply.

## How it works

| Crate | Holds | I/O? |
|---|---|---|
| `data` | generated tables (packets, blocks, items, recipes, hardness, damage types) and loaders | no |
| `protocol` | packet specs, decode/encode, state transitions ([[wire/_moc|Wire]]) | no |
| `world` | `World`, `Event`, `Effect`, the protocol reducer, chunk sections, entities, inventory | no |
| `physics` | vanilla movement as a pure function, collision, fluids | no |
| `memory` | append-only facts with time, spatial indexes | no |
| `plan` | goal table, needs planner, intents, executors, progress invariants | no |
| `sim` | the server model: blocks, fluids, mobs, combat, simulation distance | no |
| `record` | record and replay, versioned, commit-pinned ([[recording-format]]) | files |
| `net` | socket, framing, compression, reader and writer threads | sockets |
| `rcon` | RCON client | sockets |
| `brain-check` | the brain's checker (exists) | files |

Binaries: `bot` (one bot against a server), `race` (many bots, one server, case files), `simrace` (many bots against the model, in parallel), `replay` (fold a recording, verify effects), `why` (fold to time t and explain the planner's choice), `datagen` (the vanilla jar and reports into `data/`). Data files: `data/` (goals, thresholds, starts, problem statements). Workflows rebuilt lean on the runners ([[runners-not-the-mac]]).

The handoff's `store` crate (a Postgres writer) is dropped: no database ([[data-out-no-database]]).

## Enforcement
- The no-I/O crates must not depend on `net`, `rcon`, `record`, `tokio` or any socket or file crate; a CI check on `cargo tree` holds it.
- A `clippy.toml` in each core crate bans clocks, unseeded randomness, `env::var`, `HashMap`/`HashSet` iteration and shared mutation ([[determinism-rules]]).
- `#![forbid(unsafe_code)]`; `unwrap`, `expect` and `panic` denied in core crates.

## Gotchas
- clojurecraft's Clojure is about 45,000 lines in about 100 namespaces; map by responsibility into these crates, never file for file ([[cc-namespaces]]).

## See also
- [[sans-io-core]] - the core's API.
- [[tick-loop]] - what the binaries run.
