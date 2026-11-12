---
title: The wire boundary
description: Where ruststeve turns packets into Observations and Commands into packets, what a boundary error becomes, and why the core never sees a byte.
type: explanation
tags: [wire, boundary, observations, design]
aliases: [adapter, parse don't validate, S2c enum, C2s enum, Unknown variant, decode errors]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Parse, don't validate. Turn raw protocol data into Observations at this boundary only."
  - "ruststeve:FUNCTIONAL.md#Report what happened to commands. The server can silently ignore you."
  - "ruststeve:FUNCTIONAL.md#Don't swallow. Every Err that's handled by dropping something produces a log line or a counter."
  - "clojurecraft:src/clojurecraft/conn.clj#{:packet/name :decode-error"
  - "clojurecraft:src/clojurecraft/game.clj#(defmethod on-packet [:play :keep-alive]"
related:
  - "[[wire/_moc|Wire]]"
  - "[[error-kinds]]"
  - "[[body-boundary]]"
  - "[[wire-packets-as-data]]"
---

# The wire boundary

The wire layer is the only place bytes exist. It turns frames into typed packets and typed packets into `Observation`s, and turns `Command`s into packets. The core sees neither bytes nor packets.

## How it works
1. Frame bytes → (decompress) → a typed server packet (one enum variant per modelled packet, `Unknown { id, len }` for the rest).
2. A packet → zero or more `Observation`s (`EntityMoved`, `BlockChanged`, `InventoryChanged` ...), batched to the client tick boundary into `Input` ([[tick-loop]]).
3. A `Command` → one or more client packets, through the actuator, which tracks the current movement and dig so a newer command supersedes them.
4. What the adapter knows failed locally (no path, item not held) comes back as `CommandRejected { id, why }`; what the server silently ignored is caught by the core's expectations.

## Boundary errors
- A malformed frame or NaN position is a `ParseError` (a `thiserror` enum carrying the data), never an `Observation`. Unknown registry ids become `Unknown(u32)` kinds instead of failing.
- Every dropped error is logged or counted. clojurecraft does the same in its reader: a frame that fails to decode becomes a `:decode-error` packet with the state, the error and the length, which the loop can count; it never silently disappears.

## Gotchas
- Decode the chunk columns here, off the tick path ([[wire-framing]]).
- Keep-alive replies are protocol upkeep, not decisions. clojurecraft answers them in its packet reducer (`on-packet [:play :keep-alive]` emits the echo as an effect), far from its planner. ruststeve answers them in `protocol`'s `Session`, before any planning ([[connection]]).

## Limits
Whether the packet → observation mapping lives in the wire crate or in `bot-shell` is not decided.

## See also
- [[body-boundary]] - the `Body` this implements.
- [[error-kinds]] - the four kinds of failure.
