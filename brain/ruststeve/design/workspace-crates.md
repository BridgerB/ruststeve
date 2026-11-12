---
title: Workspace crates
description: Which crates and binaries the workspace has (the handoff's layout), which may do I/O, and how the boundary is enforced.
type: reference
tags: [crates, layout, architecture]
aliases: [crate layout, protocol crate, world crate, plan crate, sim crate, net crate, bins, bot-core]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve@6907139
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#protocol/    packet specs as data, decode/encode, state transitions; no I/O"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Crate boundaries: core crates (protocol, world, plan, sim) have no I/O dependencies at all. Only net, rcon, record and the binaries touch sockets and files."
  - "ruststeve:FUNCTIONAL.md#A CI check that the core has no I/O dependencies:"
  - "ruststeve:crates/brain-check/src/main.rs#//! The brain's integrity checker."
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[determinism-rules]]"
  - "[[own-protocol-not-azalea]]"
  - "[[data-out]]"
---

# Workspace crates

One cargo workspace (the root `Cargo.toml` already exists, `members = ["crates/*"]`). The layout is the handoff's, which supersedes `FUNCTIONAL.md`'s `bot-types`/`bot-core`/`bot-shell` split; the guide's boundary rules still apply.

## How it works

| Crate | Holds | I/O? |
|---|---|---|
| `data` | generated tables (packets, blocks and states, items, entities, registries, tags, recipes, loot, dimensions, harvest; [[datagen]]) | no |
| `datagen` | generates `data` from the 26.1.2 server jar and reports (runners only) | network, files, java |
| `verify-stream` | logs in to a real server and fails on a packet id `data` lacks (runners only) | sockets |
| `protocol` | primitives, framing, typed packets, states, the upkeep `Session` ([[connection]]) | no |
| `world` | `World`, `Event`, `Effect`, `step`; columns, overlay, the player ([[world-value]]) | no |
| `physics` | vanilla movement as a pure function, collision, fluids; bit-equal to clojurecraft ([[physics]]) | no |
| `memory` | sightings with time, indexed by position and column; `Seen` enforces no X-ray ([[memory-sightings]]) | no |
| `plan` | goal table, needs planner, intents, executors, progress invariants | no |
| `sim` | the server model: blocks, fluids, mobs, combat, simulation distance | no |
| `record` | recordings: zstd MessagePack, commit-pinned, `Folder` ([[recordings]]) | files |
| `replay` | the `replay` (verify, export) and `why` binaries ([[replay-and-why]]) | files |
| `net` | the socket: reader and writer threads over bounded channels ([[connection]]) | sockets |
| `rcon` | a Source RCON client for the race's own server ([[race-and-results]]) | sockets |
| `results` | race config and starts, case and batch files, the k/n summary ([[race-and-results]]) | files |
| `bot` | the bot loop as a library; binaries `bot` and `race` ([[race-and-results]]) | sockets, files |
| `brain-check` | the brain's checker (exists) | files |
| `mcp` (`ruststeve-mcp`) | the MCP server of agent tools, built on `rmcp` ([[agent-tools-over-mcp]]) | `gh`, files |
| `xtask` | repository automation run in CI (`deps-boundary`) | runs `cargo` |

Binaries: `bot` (one bot against a server; exists), `race` (a lane of bots on one server, case and batch files; exists, [[race-and-results]]), `simrace` (many bots against the model, in parallel), `replay` (fold a recording, verify effects), `why` (fold to time t and explain the planner's choice), `datagen` (the vanilla jar and reports into `crates/data`, now a crate of its own). Data files: `config/` (the race config and starts, [[race-and-results]]; goals, thresholds and problem statements join it). Workflows rebuilt lean on the runners ([[runners-not-the-mac]]).

The handoff's `store` crate (a Postgres writer) becomes `ingest`: a binary that loads batch and case files into an embedded SurrealDB for insight, kept out of the core's build ([[data-out]]).

## Enforcement
- The no-I/O crates must not depend on `net`, `rcon`, `record`, `tokio` or any socket or file crate; a CI check on `cargo tree` holds it.
- A `clippy.toml` in each core crate bans clocks, unseeded randomness, `env::var`, `HashMap`/`HashSet` iteration and shared mutation ([[determinism-rules]]).
- `#![forbid(unsafe_code)]`; `unwrap`, `expect` and `panic` denied in core crates.

## Gotchas
- clojurecraft's Clojure is about 45,000 lines in about 100 namespaces; map by responsibility into these crates, never file for file ([[cc-namespaces]]).

## See also
- [[sans-io-core]] - the core's API.
- [[tick-loop]] - what the binaries run.
