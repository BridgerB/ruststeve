---
title: Conventions
type: moc
tags: [brain, conventions]
status: verified
lastUpdated: 2026-11-11
---

# Conventions

## Frontmatter schema
- `title`: display name.
- `description`: one line saying what the note *answers*; the area MOC shows the same line.
- `type`: `index | moc | reference | explanation | decision | how-to | glossary`.
- `tags`, `aliases`: discovery; aliases are the phrasings someone would search for.
- `status`: `verified` (the cited sources were opened and read at the pins) or `draft` (not yet, or no longer).
- `lastUpdated`: the date last edited.
- `verifiedAgainst`: one `source@sha` token per source the note cites, e.g. `clojurecraft@be85a92 ruststeve@c87ac55`.
- `sourceRefs`: the places the claims come from (below).
- `related`: at least two links, including the area MOC.

## sourceRefs
A ref is `source:path#anchor`.
- `source` names a line of `brain/sources`: `ruststeve` (this repo, read from the working tree), `clojurecraft`, `steve` and `ruststeve-v1` (read with `git show <pin>:<path>` at the pin the note records).
- `#anchor` is the symbol for code (`#defn junk-slot`, `#pub fn attack`) and the quoted text for prose, config and EDN.
- Never a line number. An anchor must match exactly once; when it matches more, lengthen it to a whole line or sentence, never a fragment.
- Spell it the way the source spells it. The checker ignores whitespace, list markers and `* _ \` #`, so reformatting does not break it.

## Pins
- `ruststeve` pillar: this repo's commit. Until the crates exist that is `FUNCTIONAL.md`'s commit, c87ac55.
- `wire`, `oracle`, `game` pillars: the clojurecraft, steve or ruststeve-v1 commit each note was read at.
- `history` pillar: ruststeve-v1 and steve commits.
- The current pin of every source is in `brain/sources`. `cargo run -q -p brain-check -- drift` lists notes whose cited files changed since their own pin.

## Body template
summary → key files → how it works → gotchas → limits → see also. Gotchas are symptom-first. Decision notes carry the choice, what was rejected with the measured reason or constraint, and what would change the answer.

## Naming
kebab-case, globally unique stems.

| Pillar | Prefix | Example |
|---|---|---|
| ruststeve | bare names | `sans-io-core` |
| wire | `wire-` | `wire-varint` |
| oracle | `cc-` | `cc-world-value` |
| game | `mc-` | `mc-blaze` |
| history | `hist-` | `hist-why-v1-failed` |

Hubs are `_moc.md` and `_index.md` and are linked by full path.

## Linking
Bare names when unique; full path for hubs. Notes in the clojurecraft repo's own brain are cited as sourceRefs (`clojurecraft:brain/...#...`), never as `[[links]]`, because they are another graph.

## Size
One concept per note, about 1 to 6 KB. The discovery tier (index plus every MOC hook) stays small enough to load without thinking.

## Privacy
No secrets, RCON passwords, tokens or private data, ever.
