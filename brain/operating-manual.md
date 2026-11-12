---
title: How to use this brain
type: moc
tags: [brain, manual]
status: verified
lastUpdated: 2026-11-11
---

# How to use this brain

## The one rule
Read, do not dump. A working set is [[_index]], one or two MOCs and five to ten notes. If you are opening the fifteenth note, you want a table index or your question is several questions.

## The navigation path
1. Read [[_index]] and pick a pillar:
   - `ruststeve`: what we are building and the rules it must keep.
   - `wire`: how to read and write the server's byte stream ourselves.
   - `oracle`: clojurecraft, the bot we port.
   - `game`: vanilla 26.1.2 facts.
   - `history`: what steve and the first Rust port learned.
2. Open the pillar hub, then the area MOC. Its one-line hooks say what each note answers, so decline what you do not need.
3. Open the notes and follow links only as far as the question needs.

## How to trust a note
- When `FUNCTIONAL.md` and clojurecraft's handoff disagree, the handoff wins (Bridger, 2026-11-11); the ruststeve decisions say which way each conflict went ([[cc-handoff]]).
- `verified` means the cited text was read at the pinned commit. `draft` means not yet.
- To orient, trust it. To act, open its `sourceRefs` and confirm; exact values drift first.
- When a note and the code disagree, the code wins. Fix the note in the same change.
- The oracle and history pillars describe *other* repositories at a pin. They tell you what clojurecraft did on that day, not what ruststeve must do. The ruststeve pillar says what we do.

## What lives here versus elsewhere
- Here: how ruststeve is shaped and why, what was rejected, the protocol facts it relies on, what to port and what the earlier bots learned.
- Elsewhere: `FUNCTIONAL.md` (the porting guide, the long form of the ruststeve pillar), `CLAUDE.md` (commands), code comments (the line you are on), clojurecraft's own `brain/` (its internals in depth).
