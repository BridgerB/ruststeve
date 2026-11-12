---
title: Framing and compression
description: How packets are framed on the socket, when and how zlib compression applies, and why the protocol state flips before a state-changing packet is sent.
type: reference
tags: [wire, framing, compression, zlib]
aliases: [frame, length prefix, compression threshold, data length, login-compression, Splitter]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92 ruststeve-v1@9e1567f steve@46c1264
sourceRefs:
  - "clojurecraft:src/clojurecraft/conn.clj#\"Frame and flush one packet: raw when no threshold (negative), else a data-length prefix, with zlib only when the payload reaches the threshold.\""
  - "clojurecraft:src/clojurecraft/conn.clj#(when (and (= state :login) (= :login-compression (:packet/name pkt)))"
  - "clojurecraft:src/clojurecraft/conn.clj#current when a frame has fully arrived; the writer thread flips state before sending a"
  - "ruststeve-v1:src/protocol/framing.rs#/// Extracts complete length-prefixed packets from a byte stream, buffering any"
  - "ruststeve-v1:src/protocol/mod.rs#pub use encryption::Cfb8;"
  - "steve:src/lib/typecraft/protocol/README.md#| encryption.ts | AES-128-CFB8 via Node native crypto |"
related:
  - "[[wire/_moc|Wire]]"
  - "[[wire-primitives]]"
  - "[[wire-connection-phases]]"
---

# Framing and compression

Every packet travels as a frame: a varint length, then that many bytes. Once the server enables compression, the frame body starts with a second varint, the uncompressed data length.

## Key files
- clojurecraft `src/clojurecraft/conn.clj` (`read-frame`, `write-frame!`, `open!`), the only socket code.
- ruststeve v1 `src/protocol/framing.rs` (`Splitter`) and `compression.rs`, the same scheme over a byte stream.

## How it works
- **No compression** (threshold negative, the start of every connection): frame = `varint len` + `packet id` + fields.
- **Compression on**: frame = `varint len` + `varint data-length` + payload. `data-length` 0 means the payload is raw (it was under the threshold); otherwise the payload is zlib and inflates to exactly `data-length` bytes.
- Sending with compression on: a payload under the threshold goes as `0` + raw bytes; one at or over it goes as its length + `deflate(payload)`.
- Compression turns on when the client *receives* `login-compression` (login state), carrying the threshold. clojurecraft's reader applies it to every frame after.
- The **state** used to decode a frame is the one current when the frame has fully arrived. The writer flips the state *before* sending a state-changing packet, so the server's reply is decoded in the new state ([[wire-connection-phases]]).
- A stream reader must buffer partial frames across reads; v1's `Splitter` returns only complete payloads and keeps the remainder.

## Gotchas
- Chunk decoding is heavy. clojurecraft decodes chunk columns on the socket reader thread, so a burst of chunks never delays the loop that answers keep-alives. In ruststeve, decoding belongs in the shell's wire layer, not in `Brain::step` ([[tick-loop]]).
- A reader thread that dies silently leaves the bot on a dead wire. clojurecraft catches every `Throwable` and emits a `:closed` packet.
- Encryption (AES-128-CFB8) exists only in online mode. All three bots run offline; typecraft and v1 implement it, clojurecraft does not.

## See also
- [[wire-connection-phases]] - what each state expects.
- [[wire-primitives]] - varints.
