---
title: The server model and the sim race
description: How clojurecraft tests the whole bot with no Java server (a pure server model, captured real worlds, the sim race, snapshots judged on base and fix), the closed-loop tool the guide only sketches as bot-sim.
type: reference
tags: [oracle, sim, testing, closed-loop]
aliases: [sim.clj, server model, simrace, captured worlds, snapshots, judge, fault knobs, violations]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:src/clojurecraft/sim.clj#\"A pure model of the vanilla server: enough of it to take a bot from handshake to a held log"
  - "clojurecraft:src/clojurecraft/sim.clj#Fault knobs are inputs, not hidden state: :sim/drop-clicks is a set of click ordinals (0-based)"
  - "clojurecraft:src/clojurecraft/capture.clj#Nothing is translated: a column is the bytes of a level-chunk-with-light, which the bot decodes as it decodes any server's."
  - "clojurecraft:src/clojurecraft/simrace.clj#\"The sim race: the whole bot, from empty hands at a landing to the race's target, against the"
  - "clojurecraft:src/clojurecraft/real.clj#So a real snapshot is kept only when it recurs against the model with the same code; one that does not is a model gap, printed with what the bot did instead (gap), never filed as evidence."
related:
  - "[[oracle/harness/_moc|Harness]]"
  - "[[replay-limits]]"
  - "[[body-boundary]]"
  - "[[cc-record-replay]]"
---

# The server model and the sim race

clojurecraft closes the loop without a server: a second pure reducer models the vanilla server, and the bot plays against it in real worlds captured byte for byte. This is far beyond the guide's optional `bot-sim`, and it is how clojurecraft tests a *changed* bot cheaply ([[replay-limits]]).

## Key files
- `src/clojurecraft/sim.clj` and `sim/{protocol,world,window,mobs,spawns,portal,end}.clj`: the model.
- `src/clojurecraft/capture.clj`, `capture_nether.clj`: real worlds, saved as the server sent them.
- `src/clojurecraft/simrace.clj`: the sim race. `src/clojurecraft/real.clj`: real failures as snapshots.
- `src/clojurecraft/mob.clj`, `fluid.clj`, `fire.clj`, `hunger.clj`, `end.clj`: vanilla rules as pure functions for the model.

## How it works
- `(step sim event) → sim'`, events being a client packet or a tick; packets for the client accumulate in `:sim/out`. Modelled: the handshake, teleports, chunks, keep-alives, block breaking with vanilla timing (an early FINISH is ignored), drops and pickup, window 0 and containers with vanilla click rules and state ids, placement, furnaces, falling blocks, fluids, mobs, fire, hunger, the Nether and the End.
- **Fault knobs are inputs**: `:sim/drop-clicks` (click ordinals the server silently loses), `:sim/drop-breaks` (FINISHes answered with the ack alone); loot rolls from a seeded generator. `:sim/violations` records what a real server would punish.
- **Captured worlds**: a real server's chunk columns round each landing, byte for byte; the bot decodes them as it decodes any server's.
- **The sim race**: the whole bot from empty hands to a target, a run being a pure function of world file, seed and code, so a run is reproduced, never saved. A failure's signature groups runs into one issue.
- **Snapshots and the judge**: a sim run keeps moments; on failure the moment before the failing choice is kept. A fix is checked by playing that moment on the base code (the failure must recur) and on the fix (it must not). A real race failure becomes the same kind of snapshot, kept only when it recurs against the model; otherwise it is a model gap, never evidence.

## For ruststeve
The model is a `Body` implementation ([[body-boundary]]). Porting it (or running clojurecraft's model as a sidecar test oracle) is the cheapest way to get closed-loop tests before ruststeve plays a real server. Decide early; the model is about 4,000 lines of Clojure.

## See also
- [[replay-limits]] - why this tool matters.
- [[cc-roadmap-stages]] - the snapshots corpus the judge replays.
