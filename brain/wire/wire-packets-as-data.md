---
title: Packets as data
description: How clojurecraft describes packets as a data table read by one interpreter, why decoding reads only listed fields and ignores the tail, and what that means for a typed Rust decoder.
type: explanation
tags: [wire, codec, packets, specs]
aliases: [packet specs, spec table, interpreter, decode, encode, conditional tail, unknown packet]
status: verified
lastUpdated: 2026-11-12
verifiedAgainst: clojurecraft@be85a92 ruststeve@ff7281f
sourceRefs:
  - "clojurecraft:src/clojurecraft/packet.clj#reports). Decoding reads only the listed fields and ignores the tail of the frame: that is"
  - "clojurecraft:src/clojurecraft/packet.clj#how big packets (chunks) skip the parts we do not model. Unknown ids decode to"
  - "clojurecraft:src/clojurecraft/packet.clj#\"Every primitive wire type the specs use: how to read one from a buffer, how to write one to a"
  - "clojurecraft:src/clojurecraft/packet.clj#\"The numbers some packet fields carry, by name: each enum as 26.1.2 numbers it (its ordinal in"
  - "ruststeve:FUNCTIONAL.md#Parse, don't validate. Turn raw protocol data into Observations at this boundary only."
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-primitives]]"
  - "[[wire-prior-implementations]]"
  - "[[own-protocol-not-azalea]]"
---

# Packets as data

clojurecraft implements protocol 775 as a table of packet specs and one interpreter that reads and writes them. A packet is a flat map with `:packet/name` and the wire's own field names.

## Key files
- clojurecraft `src/clojurecraft/packet.clj`: `specs`, `wire-types`, `values`, `decode`, `encode`, `transitions`.

## How it works
- A spec is a vector of `[key type]` pairs. A type is a primitive keyword, `[:vec T]` (varint-counted), `[:if k v T]` (a conditional tail present only when the earlier field `k` read `v`), or a nested struct.
- `wire-types` is one table of every primitive: how to read it, how to write it, and what a valid value is, so codec and spec cannot disagree.
- `values` names enum fields as 26.1.2 numbers them (`:hand {:main-hand 0 :off-hand 1}`, `:dig {:start-destroy 0 :abort-destroy 1 :stop-destroy 2 :release-use-item 5}`, `:face {:down 0 :up 1 ...}`); code writes the name, the wire carries the number, and an unknown name throws rather than send nil.
- **Decode reads only the listed fields and ignores the rest of the frame.** That is how the bot models just what it uses: the chunk packet's light data, the login packet's tail and every unmodelled field are skipped for free. An unknown or unmodelled id decodes to `{:packet/name :unknown ...}`; nothing throws on foreign data.
- Encode is strict: a missing field throws, because that is a bug in our code, not the server's.
- About ninety packet specs (both directions, all states) out of the several hundred the tables name; everything else is `:unknown` and counted.

## For ruststeve
The Clojure table is data interpreted at runtime. In Rust the same idea becomes closed types parsed once at the boundary: one decoder function per modelled packet into a typed `S2c` enum variant, an `Unknown { id, len }` variant for the rest, and one `C2s` enum written by one encoder. Since issue 08 `S2c` has an encoder too, for the packets the server model sends: the inverse of the decoder, round-trip tested ([[server-model]]). The decoder still reads only what it models and ignores the tail; each variant then becomes `Observation`s ([[wire-boundary-parsing]]). Exhaustive matches replace the runtime "no such spec" error.

## Gotchas
- Never give a field the bare name `name` next to the packet's identity; clojurecraft once overwrote its packet name with the login `hello`'s player name and now spells the field `:username`.
- A slot whose item components cannot be stepped over ends what can be read of a frame ([[wire-slots]]).

## See also
- [[wire-prior-implementations]] - how typecraft and ruststeve v1 did this instead.
- [[wire-boundary-parsing]] - from packets to observations.
