---
title: clojurecraft is the oracle
description: Why ruststeve ports clojurecraft's behaviour and architecture (not ruststeve v1's or steve's), and how the Clojure bot stays the bar every phase is measured against.
type: decision
tags: [decision, porting, oracle, clojurecraft]
aliases: [the oracle, port from Clojure, differential testing, baseline]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Keep the Clojure bot alive as the oracle until the Rust bot passes it."
  - "ruststeve:FUNCTIONAL.md#This bot has been written three times: TypeScript first, then a Rust port that copied the TypeScript design, then Clojure."
  - "ruststeve:FUNCTIONAL.md#Port behavior, not syntax. Map Clojure namespaces to Rust modules by responsibility, not by name. A 1:1 file mapping is a smell."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[port-phases]]"
  - "[[oracle/_moc|Oracle]]"
---

# clojurecraft is the oracle

## The choice
ruststeve ports clojurecraft: its architecture (one world value, pure decisions, recorded runs) and its game behaviour (goals, thresholds, rule tables). clojurecraft keeps running as the oracle until ruststeve passes it: differential tests against its pure functions, its early-game success rate and milestone times as the phase-4 bar.

## What was rejected
- **Continuing ruststeve v1.** It copied the TypeScript design into Rust syntax; the opacity stayed and change got slower ([[hist-why-v1-failed]]). Its hard-won protocol and game facts are kept as knowledge ([[hist-v1-load-bearing-fixes]]); its code is not ported.
- **Porting steve.** steve's game knowledge is good, but its state is spread across callbacks and mutable objects, the shape that made failures unreproducible.
- **A line-by-line Clojure port.** Clojure written in Rust syntax would fail the way v1 did; namespaces map to modules by responsibility, never one-to-one ([[accents]]).

## What would change the answer
ruststeve passing clojurecraft's baseline in a phase: from then on ruststeve sets its own bar for that phase.

## See also
- [[oracle/_moc|Oracle]] - what clojurecraft is and does.
- [[port-phases]] - where the oracle sets each gate.
