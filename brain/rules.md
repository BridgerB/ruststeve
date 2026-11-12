---
title: Rules
type: moc
tags: [brain, rules]
status: verified
lastUpdated: 2026-11-11
---

# Rules

## The verification bar
A note is `status: verified` only if someone opened every cited source at the pinned commit and read it. Never transcribe from memory, from another note, or from clojurecraft's own brain without opening the code it describes. What could not be checked goes in `draft`, and its Limits section says what is unverified.

## The four invariants (hold at zero)
0 broken links, 0 ambiguous links, 0 orphans, 0 unresolved sourceRefs. Run `cargo run -q -p brain-check` after every batch of edits; it also flags verified notes without refs or pins, and notes with fewer than two related links. A missing sibling checkout counts as unchecked, not unresolved. A resolving ref proves the cited text exists, not that it supports the claim; that judgment belongs to review.

## Maintenance
- A change to the crates updates the matching note in the same commit and bumps `lastUpdated` and the `ruststeve@` pin. `cargo run -q -p brain-check -- touches ruststeve <paths>` lists the notes a diff affects.
- When a sibling moves on (clojurecraft merges, a steve cycle lands), bump its pin in `brain/sources`, run `drift`, and re-read what moved.
- Capture a gotcha the moment it costs something, anchored to where it bit.
- Write a decision note whenever an option a reasonable person would propose is rejected, with the measurement or constraint, and what would change the answer.
- Downgrade to draft when you cannot re-verify. Delete what became false.
- Re-run [[questions]] when an area changes materially.

## Privacy
Never store secrets, credentials or private data anywhere in the brain.
