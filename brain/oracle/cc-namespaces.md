---
title: clojurecraft namespace index
description: Every clojurecraft source namespace in one table, grouped by area, with what it is and whether it does I/O; the seed of ruststeve's INVENTORY.md.
type: reference
tags: [oracle, inventory, namespaces, index]
aliases: [INVENTORY.md seed, namespace table, what each file does, clojurecraft modules]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/conn.clj#This is one of the three namespaces that do I/O (with rcon and main)."
  - "clojurecraft:src/clojurecraft/main.clj#\"The loop: the one place with an atom, a clock, randomness and effects. Reads packets and"
  - "clojurecraft:src/clojurecraft/game.clj#\"The world as one value, and the protocol as a pure reducer over events."
  - "clojurecraft:src/clojurecraft/plan.clj#\"Goals are data; planning is a function; executing is open."
  - "clojurecraft:src/clojurecraft/sim.clj#\"A pure model of the vanilla server: enough of it to take a bot from handshake to a held log"
related:
  - "[[oracle/_moc|Oracle]]"
  - "[[port-phases]]"
  - "[[cc-to-rust-map]]"
---

# clojurecraft namespace index

114 namespaces under `src/clojurecraft/` at be85a92, about 45,000 lines. Each row's text condenses the namespace's own docstring. Only `conn`, `rcon` and `main` do I/O (plus the harness, gym, race, capture and CLI entry points, which drive processes and servers); everything else is pure. Phase 0 turns this into `docs/INVENTORY.md` with state, side effects and Rust destination per row ([[port-phases]]).

## Wire and connection
| ns | What it is |
|---|---|
| `bytes` | wire primitives, bytes in, value out ([[wire-primitives]]) |
| `packet` | packet specs as data and one interpreter ([[wire-packets-as-data]]) |
| `conn` | **I/O**: the socket, frames, compression, protocol state ([[wire-framing]]) |
| `chunk` | chunk columns as values, paletted sections ([[wire-chunk-sections]]) |
| `nbt`, `text` | network NBT and text components as values |
| `rcon` | **I/O**: minimal RCON client for fixtures and judges |

## Core loop and world
| ns | What it is |
|---|---|
| `main` | **I/O**: the loop, the one atom, clock, randomness, effects ([[cc-main-loop]]) |
| `game` | the world value and the protocol reducer ([[cc-reducer-and-effects]]) |
| `model`, `spec`, `shapes` | the information model in writing; specs ([[cc-world-value]]) |
| `inventory`, `window` | items and screens; clicks as values ([[cc-window-clicks]]) |
| `entity`, `hear`, `ear`, `eye`, `chat`, `said`, `derived` | entities and senses ([[cc-entities]]) |
| `terrain`, `memory`, `memo`, `avoided` | blocks, facts with time, memoised queries, learned avoidance ([[cc-world-memory]]) |
| `physics`, `trap` | client movement; water/lava trap detection ([[cc-physics]]) |
| `blocks`, `registry`, `dimension`, `recipe`, `food`, `drops` | generated tables and lookups ([[cc-datagen]]) |
| `record`, `replay`, `watch`, `senses`, `scene` | recording, replay, observers, one-line-a-second senses ([[cc-record-replay]]) |

## Planning and acting
| ns | What it is |
|---|---|
| `plan` | the planner: goal table, registries, `plan-tick`, watchdog ([[cc-goals-and-intents]]) |
| `make`, `vein` | the needs planner and acts ([[cc-plan-and-make]]) |
| `intent` | intents as values, the open `run` executor, the dig |
| `path` | A* over the block grid, budget in expansions |
| `churn-guard`, `failure` | the churn invariant; failure signatures ([[cc-stall-and-budgets]]) |
| `wood`, `stone`, `stairs`, `scoop`, `place`, `craft`, `smelt`, `table`, `filler` | early-game acts and intents |
| `swim` | escapes from water, lava and fire |
| `eat`, `hunger`, `forage`, `armour`, `sword`, `gear`, `picks` | survival and kit rows |
| `combat`, `dodge`, `pocket`, `threat`, `fire` | fighting, fireball lines, cover ([[cc-blaze-hunt]]) |
| `build` | building from a plan of ops; the portal cast ([[mc-portal-cast]]) |
| `fortress`, `fortress-exit`, `blaze` | reaching a fortress; blaze rods |
| `pearls`, `pearls.*` | the pearl hunt's rule table ([[cc-pearls-rule-table]]) |
| `eyes`, `stronghold`, `end-portal`, `end`, `dragon`, `beds` | eyes, stronghold, the End, the dragon |

## Server model (pure, for tests)
| ns | What it is |
|---|---|
| `sim`, `sim.*`, `mob`, `fluid` | the vanilla server as pure reducers ([[cc-sim]]) |
| `capture`, `capture-nether` | **I/O**: real worlds saved byte for byte |
| `simrace`, `real` | the sim race; real failures as snapshots |

## Harness and tooling
| ns | What it is |
|---|---|
| `gym`, `land`, `landings`, `fixture`, `harness`, `observe` | gyms on real servers ([[cc-race-and-gym]]) |
| `race`, `race-facts`, `claims`, `start`, `servers`, `lanes` | races and their data |
| `cli` | shared command-line parsing and logging |

`bb/clojurecraft/*` (Babashka) holds the fleet, botlogs, churn, failures, page and site tooling; not part of the bot.

## See also
- [[cc-to-rust-map]] - how each kind of namespace maps to Rust.
- [[cc-data-files]] - the `resources/` tables.
