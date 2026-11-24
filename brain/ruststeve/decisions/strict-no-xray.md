---
title: Strict no X-ray
description: Bridger's decision (2026-11-15) that ruststeve never reads an unexposed cell, not even to refuse a site or a dig - danger hidden in rock is met when a dig exposes it.
type: decision
tags: [decision, xray, perception, memory]
aliases: [strict no X-ray, no danger reads, unexposed lava, refusal-only reads]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@ddae609
sourceRefs:
  - "ruststeve:crates/memory/src/lib.rs#pub fn record_source(&mut self, seen: SeenSource) -> bool {"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[hist-no-xray]]"
  - "[[portal-cast]]"
---

# Strict no X-ray

## The decision
Bridger, 2026-11-15, asked while planning issue 14: ruststeve reads **only cells the eye can see** (a cell with a loaded, see-through neighbour). Not even a refusal (of a cast site, of a dig) may rest on a cell inside rock. The option of refusal-only reads of unexposed fluid was offered and not taken.

## What it settles
`brain/history/hist-no-xray.md` left "danger reads of unexposed blocks" open; v1 left the same question for a decision. This is that decision.

## How it is enforced
- Memory: sightings and fluid pools are made only by `Seen::check` and `SeenSource::check` ([[memory-sightings]]).
- The cast's site choice and block count read a cell through `seen`, which returns nothing for a cell inside rock; such a cell counts as rock ([[portal-cast]]).
- Lava hidden behind a frame hole is met when a dig exposes it: the cast sees lava in its room and gives the site up.

## What would change it
Bridger's call only. The measure to watch is the given-up count in race case files: sites lost to hidden fluid are this decision's cost.

## See also
- [[hist-no-xray]] - the rule's history in the earlier bots.
