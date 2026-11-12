---
title: Three protocol layers compared
description: How typecraft, ruststeve v1 and clojurecraft each turned bytes into packets, and what ruststeve keeps and drops from each.
type: explanation
tags: [wire, history, comparison, boundary]
aliases: [ProtoDef, protocol-schema.json, PValue, minecraft-data, typecraft codec, schema interpreter]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: steve@46c1264 ruststeve-v1@9e1567f clojurecraft@be85a92
sourceRefs:
  - "steve:src/lib/typecraft/protocol/README.md#Minecraft protocol client — packet serialization, framing, encryption, compression, and connection management. Replaces minecraft-protocol + protodef."
  - "steve:src/lib/typecraft/protocol/build-protocol.ts#2. Packet field definitions (protocol-extracted.json, auto-generated from MC bytecode)"
  - "ruststeve-v1:src/protocol/codec.rs#//! Schema-driven packet codec — a Rust interpreter of typecraft's ProtoDef"
  - "ruststeve-v1:src/protocol/value.rs#//! Dynamic protocol value — the runtime representation of any packet field,"
  - "clojurecraft:src/clojurecraft/packet.clj#\"Packet specs as data, and one interpreter that reads and writes them."
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-packets-as-data]]"
  - "[[own-protocol-not-azalea]]"
  - "[[hist-why-v1-failed]]"
---

# Three protocol layers compared

All three earlier bots parsed the server's byte stream themselves. That part worked in every one of them; how the parsed data was *shaped* is where they differ.

## How it works

| | typecraft (steve) | ruststeve v1 | clojurecraft |
|---|---|---|---|
| Field layouts from | a ProtoDef schema assembled from hand-written shared types, fields extracted from the server bytecode, and ids from `--reports` | the same schema, frozen into `protocol-schema.json` and `include_str!`'d | a hand-written spec table of about ninety packets, ids from `--reports` |
| Decoded value | `Record<string, unknown>` | `PValue`: `Num(f64)`, `Str`, `List`, `Compound(Vec<(String, PValue)>)` ... a dynamic tree | a flat map with `:packet/name` and wire names |
| Coverage | every packet | every packet | only what the bot uses; the rest `:unknown`, tails ignored |
| Delivery | `EventEmitter` events per packet name | `async` tasks holding `&mut Bot` read and write packets inline | packets on a core.async channel into one reducer |
| Encryption, Microsoft auth | yes | yes | no (offline only) |

## What ruststeve keeps
- Doing it ourselves: framing, compression, varints, chunk sections, the 775 specifics are all understood and small.
- Ids generated from the server's reports ([[wire-packet-ids]]).
- clojurecraft's scope: model only what the bot uses, ignore tails, count unknowns ([[wire-packets-as-data]]).

## What ruststeve drops
- A runtime schema interpreter producing dynamic values. In v1 every field was a `PValue` looked up by string name (`"windowId"`, `PValue::num(window_id as f64)`): integers as `f64`, typos that compile, nothing exhaustive. That is the "TypeScript accent" at the protocol layer ([[hist-why-v1-failed]]).
- Event emitters and decide-and-send `async` tasks as the delivery mechanism; ruststeve decodes into typed values that become `Observation`s on one queue ([[tick-loop]]).

## See also
- [[own-protocol-not-azalea]] - the decision this supports.
- [[wire-boundary-parsing]] - the typed boundary instead.
