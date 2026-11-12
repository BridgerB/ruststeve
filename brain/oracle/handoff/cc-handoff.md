---
title: The clojurecraft handoff
description: What clojurecraft's closing handoff says (where the bot got, stage by stage, what was shut down, what data remains) and why it outranks FUNCTIONAL.md wherever the two disagree.
type: reference
tags: [oracle, handoff, status, shutdown]
aliases: [HANDOFF-RUSTSTEVE.md, PR 871, clojurecraft shutdown, where clojurecraft got, stage results]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#This is the last change to clojurecraft. Work stops here on 2026-11-12, and everything moves to a new Rust bot, ruststeve v2:"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| 17: blaze rods (7) | passed: real 184 of 2,518 bots (7.3%), sim 90 of 688 (13%) on 0ace52e |"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| 18: ender pearls (12) | never passed."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Disabled at shutdown (gh workflow disable, reversible with gh workflow enable):"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Releases: botlogs-<day>-<hour> (case files, ledgers, recordings of failed runs), jfr-<run id> (lane 1 profiles), win releases."
related:
  - "[[oracle/handoff/_moc|Handoff]]"
  - "[[cc-stage18-lessons]]"
  - "[[first-twenty-issues]]"
---

# The clojurecraft handoff

clojurecraft's last change (PR #871, `docs/HANDOFF-RUSTSTEVE.md`) closes the project on 2026-11-12 and hands the effort to ruststeve. Bridger, 2026-11-11: the clojurecraft project is "the real thing and much more knowledgeable" than `FUNCTIONAL.md`, which was written blind from a basic prompt. **Where the handoff and `FUNCTIONAL.md` disagree, the handoff wins**; the ruststeve pillar's decisions record each case.

## Where clojurecraft got
- 32 days (2026-10-09 to 11-12), 578 merged PRs, 256 issues, about 45,000 lines of Clojure, all compute on up to 20 GitHub runners.
- About 25 stages from spawn to dragon. A stage *passes* when about 5% or more of the bots that start it complete it, in both the real race and the sim race.

| Stage | Result |
|---|---|
| 1–16: wood to the fortress | passed, proven repeatedly from spawn by Oct 31 |
| 17: blaze rods (7) | passed: real 184 of 2,518 (7.3%), sim 90 of 688 (13%) on 0ace52e |
| 18: ender pearls (12) | never passed. First real pass 2026-11-09. At shutdown: real rods start ~0–1%, real forest start ~3–4%, sim rods ~5%, sim forest ~30% |
| 19–25: eyes, stronghold, end portal, End, dragon | code exists for most; never reached in a race |

The real race is the honest metric; the gap between sim (30%) and real (3%) on the forest start is the lesson ([[cc-stage18-lessons]]).

## What is shut down
Every compute workflow is disabled with `gh workflow disable` (reversible with `gh workflow enable`): `fleet`, `race`, `simrace`, `probe`, `test`, `unit`, `issue`, `gym`, `world`, `simworld`, `pages`. Every running and queued run was cancelled. This frees the 20 runners for ruststeve ([[runners-not-the-mac]]).

## What remains to mine
- gh-pages: the results site and `data/batches/<day>.ndjson`, every batch since late October by commit.
- Releases: `botlogs-<day>-<hour>` (case files, ledgers, recordings of failed runs), `jfr-<run id>` (profiles), win releases. These hold real recordings of stage-18 failures, worth mining for ruststeve's parity tests.
- Unmerged branches: [[cc-unmerged-leads]].

## See also
- [[first-twenty-issues]] - the plan the handoff gives ruststeve.
- [[cc-loop-rules]] - the operating loop clojurecraft ran under.
