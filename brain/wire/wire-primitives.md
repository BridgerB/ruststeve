---
title: Wire primitives
description: How every primitive field type is laid out on the wire (varint sign-extension, packed block positions, strings, UUIDs, the LpVec3 velocity), and the traps in each.
type: reference
tags: [wire, protocol, varint, bytes]
aliases: [varint, varlong, LEB128, packed position, LpVec3, read-string, big-endian]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f
sourceRefs:
  - "clojurecraft:src/clojurecraft/bytes.clj#\"LEB128 varint, sign-extended to int32 like the vanilla client.\""
  - "clojurecraft:src/clojurecraft/bytes.clj#\"Negative ints go out as their unsigned 32-bit form (5 bytes), like vanilla.\""
  - "clojurecraft:src/clojurecraft/bytes.clj#\"i64 bitfield x:26 | z:26 | y:12, each signed.\""
  - "clojurecraft:src/clojurecraft/bytes.clj#defn read-lpvec3"
  - "clojurecraft:src/clojurecraft/bytes.clj#\"Varint UTF-8 byte length (not char count), then the bytes.\""
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-packets-as-data]]"
  - "[[wire-framing]]"
---

# Wire primitives

Every packet field is built from a small set of primitives. All fixed-width numbers are big-endian.

## Key files
- clojurecraft `src/clojurecraft/bytes.clj`: readers over a `ByteBuffer` cursor, writers to a `DataOutputStream`, every public function bytes-in, value-out.

## How it works

| Type | Layout |
|---|---|
| `bool` | one byte, any nonzero is true |
| `i8` `u8` `i16` `u16` `i32` `u32` `i64` | big-endian, `u*` read unsigned |
| `f32` `f64` | big-endian IEEE |
| `varint` | LEB128, at most 5 bytes, result sign-extended to i32; a negative goes out as its unsigned 32-bit form, 5 bytes |
| `varlong` | LEB128, at most 10 bytes, signed i64 |
| `string` | varint **byte** length (not char count), then UTF-8 |
| `uuid` | 16 raw bytes, most significant long first |
| `position` | one i64: x 26 bits, z 26 bits, y 12 bits, each signed (`x << 38 | z << 12 | y`) |
| `lpvec3` | an entity velocity: a first byte 0 is the zero vector; else two bytes and a u32 form 48 bits (2 low scale bits, a continuation bit meaning a varint of higher scale bits follows, three 15-bit components at bits 3, 18, 33 in [-1, 1] times the scale) |

## Gotchas
- A varint longer than 5 bytes (or a varlong past 10) is malformed and must be an error, not a loop.
- Unpacking a position needs arithmetic shifts on the signed i64 so negative coordinates come back negative.
- LpVec3 replaced three shorts in 1/8000 blocks before 1.21.9. Older references (and minecraft-data for old versions) show the old form.
- Frame lengths are varints read from the socket, a byte at a time, before any buffer exists.

## Limits
NBT (text components, block-entity data) is its own format; clojurecraft reads it in `nbt.clj`, not covered here.

## See also
- [[wire-framing]] - where varint lengths frame packets.
- [[wire-packets-as-data]] - how these types compose into packets.
