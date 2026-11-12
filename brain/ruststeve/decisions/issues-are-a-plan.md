---
title: Issues are a plan, not a log
description: Why ruststeve's tracker holds a small numbered plan of capability issues with checkable done-conditions, never roadmap issues or auto-filed failure signatures.
type: decision
tags: [decision, process, issues, planning]
aliases: [no auto-filed issues, no roadmap issue, done-condition, problem statements, docs/problems]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#Issues 1–9 worked because each was one capability with one test that said done."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#How to run the list: one issue at a time. Each issue's checkpoint start exists before its gameplay code."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#A docs/problems/<stage>.md per stage, written before its code: needs, risks, done in world terms, the last recorded failure."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[first-twenty-issues]]"
  - "[[cc-stage18-lessons]]"
---

# Issues are a plan, not a log

## The choice
- The tracker holds a short numbered plan: each issue one capability, small enough for one or a few PRs, with a done-condition a runner can check ([[first-twenty-issues]]).
- Worked one at a time. An issue closes only when its done-condition holds on the real race at the stated k/n. An issue too big splits into numbered sub-issues (17a, 17b), so the plan stays one list.
- A failure inside an issue gets a problem statement in `docs/problems/<stage>.md` (needs, risks, done in world terms, the last recorded failure), not a new issue.
- Failure signatures go to the results data and case files, never the tracker.

## What was rejected
- **A roadmap issue** (clojurecraft #10 held nine stages and had no done-condition of its own; everything after the Nether grew out of it).
- **Auto-filed symptom issues** (256 by the end, unplannable, worked in parallel in the same code).

## What would change the answer
Nothing measured; this is the cheapest lesson the handoff offers.

## See also
- [[cc-stage18-lessons]] - what happened without it.
