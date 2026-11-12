---
title: Unmerged leads at shutdown
description: The stage-18 fix branches clojurecraft left pushed but unmerged, what each tried and how far it was proven; leads for ruststeve, not promises.
type: reference
tags: [oracle, handoff, branches, stage-18]
aliases: [unmerged branches, fix/263 branches, cage-line, filler-keep, craft-flip, heading-no-pace]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#All pushed, none merged, with each one's state at shutdown. Each is a lead for ruststeve, not a promise."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| fix/263-cage-line | break-out tunnels hold one direction instead of a 30 s clock; unit green; cage-break digs fell from 877 to 76–111 per bot on R1/17 in the sim; baseline comparison unread |"
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#| fix/263-craft-flip | items in an open table's grid count as held; confirmed on r04's recording; sim pair unread |"
related:
  - "[[oracle/handoff/_moc|Handoff]]"
  - "[[cc-stage18-lessons]]"
  - "[[progress-invariants]]"
---

# Unmerged leads at shutdown

Each branch is pushed to BridgerB/clojurecraft and was never merged. Read the branch before porting its idea; most were never measured to the end.

| Branch | What it does | Proven how far |
|---|---|---|
| `fix/263-cage-line` | break-out tunnels hold one direction instead of a 30 s clock | unit green; cage-break digs fell from 877 to 76–111 per bot in the sim; baseline unread |
| `fix/263-heading-no-pace` | a heading leg fails after two steps that neither dig nor get closer | sim gain never measured |
| `fix/263-filler-keep` | keep 32 building blocks unspent; restock from the tunnel wall before a heading leg | judge never finished |
| `fix/263-heading-floor` | heading legs never sit under the fortress exit's surface | not proven on a snapshot |
| `fix/263-heading-blocked` | remember blocked directions and climb | did not clear its snapshot; the real cause was corridor pacing |
| `fix/263-craft-flip` | items in an open table's grid count as held | confirmed on a real recording; sim pair unread |
| `fix/263-frozen-angry` | angry endermen count even when frozen | a guard, unconfirmed |
| `fix/263-retrace-forward` | fortress retrace follows the inbound trail | level with main; did not fix its case |
| `fix/263-heal-in-cover` | heal under fire only behind cover | cut hits while healing 22→4, deaths rose by one; re-measure unread |
| `fix/263-blaze-range`, `fix/263-shot-tunnel` | hold cover after being shot | never fired in the sim |
| `fix/263-boxed-expiry`, `-burrow-churn`, `-forest-level`, others | parked | |

## For ruststeve
Four of these are the same lesson, a loop with no progress (cage-line, heading-no-pace, filler-keep, craft-flip). ruststeve builds that in structurally rather than patching each case ([[progress-invariants]]). Issue 15's done-condition (median bot keeps 32+ building blocks) and issue 17's (a heading that holds direction, never paces, restocks filler) come straight from these ([[first-twenty-issues]]).

## See also
- [[cc-stage18-lessons]] - the causes behind them.
