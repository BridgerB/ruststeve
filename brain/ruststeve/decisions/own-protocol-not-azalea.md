---
title: Our own protocol layer, not azalea or a sidecar
description: Why ruststeve parses the server's byte stream itself instead of using azalea or a Node sidecar body, and what would change that.
type: decision
tags: [decision, protocol, body, azalea]
aliases: [why not azalea, no sidecar, from-scratch protocol, parse the datastream ourselves]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 clojurecraft@be85a92 ruststeve-v1@9e1567f steve@46c1264
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Requires nightly Rust. Its ECS is a second world model: never let your core read it directly."
  - "ruststeve:FUNCTIONAL.md#Reuses a body you already know reaches the Nether."
  - "clojurecraft:src/clojurecraft/conn.clj#This is one of the three namespaces that do I/O (with rcon and main)."
  - "ruststeve-v1:src/protocol/framing.rs#//! Packet framing — varint length-prefix encoding/decoding."
  - "steve:src/lib/typecraft/protocol/README.md#Minecraft protocol client — packet serialization, framing, encryption, compression, and connection management. Replaces minecraft-protocol + protodef."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[wire-prior-implementations]]"
  - "[[body-boundary]]"
  - "[[wire/_moc|Wire]]"
---

# Our own protocol layer, not azalea or a sidecar

## The choice
ruststeve reads and writes the Minecraft 26.1.2 (protocol 775) byte stream itself: framing, compression, a typed packet table for what the bot uses, chunk decoding, all in our own crates. Decided by Bridger on 2026-11-11 ("i can get really far from properly parsing the datastream from the server"), amending `FUNCTIONAL.md` section 7.1, which offered only azalea or a sidecar.

## What was rejected
- **azalea** (the guide's option A): requires nightly Rust; its Bevy ECS is a second world model the core must never read; its pathfinder times out on wall clock, so it can only be an actuator, never part of the deterministic core. Adopting it means owning a large framework whose state model is the opposite of one owned `Brain`.
- **A sidecar body** (option B, e.g. Node over JSON Lines): two processes and a serialization boundary, and a non-Rust codebase in a project that is Rust only.
- **Not a capability question.** typecraft, ruststeve v1 and clojurecraft each implemented the protocol from scratch; ruststeve v1 entered the Nether through its own portals on it, and clojurecraft passed stages 1 to 17 in real races from spawn on it. Its whole socket layer is one small namespace, one of only three that do I/O. Every wall the bots hit was in game logic ([[hist-walls]]), never in parsing.

## What would change the answer
- Protocol upkeep dominating the work: if following Mojang's version bumps (new packet ids, changed layouts such as 26.1's fluid count or LpVec3 velocities) costs more than the bot work it supports.
- A maintained Rust client crate on stable Rust that exposes packets as plain typed values with no world model of its own.

## See also
- [[wire-prior-implementations]] - what each earlier layer did, and what we keep.
- [[body-boundary]] - where the layer plugs in.
