---
title: Randomness on the tick
description: How clojurecraft keeps randomness replayable (one random number arrives on every tick event), and the matching rule for ruststeve's seeded RNG.
type: decision
tags: [oracle, determinism, randomness, replay]
aliases: [rand on the tick, :event/rand, rand-nth bug, seeded randomness]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/intent.clj#new namespace. Every deadline is an absolute ms value compared against :time/now; the only"
  - "clojurecraft:docs/RICH-HICKEY.md#Worse, it calls rand-nth for the detour yaw inside what is supposed to be a pure reducer."
  - "clojurecraft:src/clojurecraft/simrace.clj#A run is a pure function of three inputs: the world file, the seed (the model's loot rolls and"
related:
  - "[[oracle/architecture/_moc|Architecture]]"
  - "[[determinism-rules]]"
  - "[[cc-main-loop]]"
---

# Randomness on the tick

## The choice
clojurecraft's reducers never call `rand`. The loop draws one random number per tick and puts it on the tick event (`:event/rand`); intents that need a random choice (a detour heading) read it from there. Randomness is an input, so it is recorded with the event and a replay reproduces it.

## What was rejected
Calling `rand-nth` inside the walk policy. The design essay names it as the thing that made a replay of the same events diverge, throwing away the design's biggest benefit.

## For ruststeve
The same rule in Rust form: one `ChaCha8Rng` inside `Brain`, seeded from the recording header, its state checkpointed ([[determinism-rules]]). Either works; what matters is that no draw happens outside the recorded inputs. In clojurecraft's sim race the seed also drives the server model's loot rolls, so a run is a pure function of world file, seed and code.

## What would change the answer
Nothing; this is the precondition of replay.

## See also
- [[determinism-rules]] - the whole checklist.
- [[cc-sim]] - where the seed also drives the model.
