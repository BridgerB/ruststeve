---
title: No X-ray
description: The rule that a bot may act only on blocks it has seen with an exposed face, why it exists, how each bot enforced it, and what reads of unexposed data remain an open question.
type: decision
tags: [history, perception, rules, no-xray]
aliases: [x-ray, honest perception, exposed face, is_exposed, lava_seen, unexposed chunk data]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve-v1@9e1567f clojurecraft@be85a92 steve@46c1264
sourceRefs:
  - "ruststeve-v1:CLAUDE.md#No X-ray: the bot may act only on blocks it has seen with an exposed face. This is enforced in code (see \"Honest perception\" below), not just policy."
  - "ruststeve-v1:CLAUDE.md#Bot::assert_lava_seen panics in gym mode on any lava-site decision whose target was never seen exposed."
  - "clojurecraft:src/clojurecraft/memory.clj#\"What the bot has seen, as facts with time. The store is a DataScript database held as a value in"
  - "steve:docs/report-2026-10-09.md#Buried blocks break but their drops are lost, since the server checks reach, not line of sight. Dig only cells with an air face."
related:
  - "[[history/_moc|History]]"
  - "[[cc-world-memory]]"
  - "[[domain-types]]"
---

# No X-ray

## The choice
The bot acts only on blocks it has seen with an exposed face (an air or see-through neighbour). The server sends whole chunks, ore and lava buried in rock included; reading that is X-ray, and no bot may decide on it.

## Why
- It is the honest version of the game: a player cannot see through rock.
- It is also practical: steve found buried blocks break but their drops are lost, since the server checks reach, not line of sight.

## How each bot enforced it
- **ruststeve v1** (cycle 6): `find_lava_cluster`, a read of lava through rock, was deleted; `is_exposed` stopped counting lava (registry-transparent) and unloaded cells as faces; `lava_seen` remembers only sources seen with an air face; `assert_lava_seen` panics in gym mode on any lava decision whose target was never seen exposed. Paired against the X-ray build, the honest one passed 3/11 vs 1/11: not worse.
- **clojurecraft**: memory records only watched kinds as they are seen; where water or lava is beside the body now is the terrain's question.

## What would change the answer
Bridger's call only. Open in v1: danger-avoidance reads of unexposed lava (refusing to dig into it) were left for a decision; ruststeve should decide this explicitly and encode it, for example by giving perception a type that only exposed blocks can produce.

## See also
- [[cc-world-memory]] - clojurecraft's memory.
- [[domain-types]] - context types that make the rule structural.
