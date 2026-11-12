---
title: ruststeve pillar
type: moc
tags: [pillar, hub, ruststeve]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
related:
  - "[[_index]]"
---

# ruststeve pillar

What we are building: a deterministic, replayable Rust speedrun bot ported from clojurecraft. Until the crates exist, the sources are `FUNCTIONAL.md` (c87ac55) and clojurecraft's closing handoff, which wins where they disagree; the decisions here record each case.

## Areas
- [[ruststeve/design/_moc|Design]] - the core, the shell, data, determinism, recordings, observability, errors, tests.
- [[ruststeve/roadmap/_moc|Roadmap]] - the twenty-issue plan and the method for getting unstuck.
- [[ruststeve/decisions/_moc|Decisions]] - what was chosen, what was rejected and why.

## See also
- [[_index]] - the master index.
- [[wire/_moc|Wire]] - the protocol layer ruststeve writes itself.
- [[oracle/_moc|Oracle]] - the bot being ported.
