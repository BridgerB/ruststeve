---
title: clojurecraft's world value
description: What clojurecraft's whole state is (one flat, sparse map of namespaced attributes in one atom), what lives in it, and how it maps to ruststeve's Brain.
type: reference
tags: [oracle, world, state, information-model]
aliases: [world map, :player/pos, :world/chunks, model.clj, information model, atom]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/game.clj#The world is a flat map of namespaced attributes (:player/pos, :world/chunks, :bot/phase ...),"
  - "clojurecraft:src/clojurecraft/game.clj#\"The world before any event: handshake phase, no position, empty inventory, chunks and facts."
  - "clojurecraft:docs/RICH-HICKEY.md#Information model in writing: model.clj lists every attribute with its meaning and lifetime; a test fails if a running bot writes a key it does not list, or if a listed attribute has no spec."
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[cc-reducer-and-effects]]"
  - "[[world-model-fold]]"
  - "[[cc-world-memory]]"
---

# clojurecraft's world value

All of clojurecraft's state is one immutable map in one atom, written only by the loop thread. The map is flat and sparse: namespaced attributes (`:player/pos`, `:world/chunks`, `:plan/intent` ...), and an attribute the bot does not know yet is simply absent.

## Key files
- `src/clojurecraft/game.clj`: `init` (the world before any event).
- `src/clojurecraft/model.clj`: every attribute with a one-line meaning and lifetime; a test fails when a running bot writes an unlisted key or a listed key has no spec.
- `src/clojurecraft/spec.clj`: the specs.

## How it works
- `init` starts with `:bot/phase :handshake`, empty `:bot/effects`, `:time/now 0`, `:time/tick 0`, the player's velocity, look, on-ground and controls, empty inventory and `:window/grid`, empty `:world/chunks`, `:world/blocks` (the overlay of changed cells), `:world/entities`, `:world/facts` (the memory store) and `:stats/*` counters. Name and UUID are set when known.
- Namespaces by meaning: `:bot/*` (phase, effects, name), `:time/*`, `:player/*` (body, inventory, health, dimension), `:window/*`, `:world/*` (chunks, overlay, entities, facts, chat), `:plan/*` (goals, intent, failures, trap), `:intent/*` (scratch inside one intent), `:stats/*`.
- Positions are `[x y z]` vectors; chunks are a map `{[cx cz] column}`.

## For ruststeve
The Rust form is `Brain { world: WorldModel, plan, expectations, rng, config }` with typed fields instead of keys ([[world-model-fold]]). The *information model in writing* carries over directly: `model.clj`'s attribute list is the inventory of what `WorldModel` and `Plan` must hold, and its lifetimes say which become `Option` in the raw record and which become context types ([[domain-types]]).

## Gotchas
- Open, absent-when-unknown keys are Hickey's "maps know what they know". In Rust, absence must be explicit (`Option`) or a separate context type; do not port it as one struct with every field optional.

## See also
- [[cc-reducer-and-effects]] - how the value changes.
- [[cc-world-memory]] - the fact store inside it.
