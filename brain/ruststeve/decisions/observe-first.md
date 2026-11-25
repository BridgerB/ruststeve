---
title: Observe first
description: Bridger's decision (2026-11-22, issue 16e) that ruststeve runs with full observability before any behaviour fix - every packet decoded or skipped with a reason, a ledger of every task and fact, a senses row a second, moments and server truth per run, a coverage row that prints every blind spot - and that a wall is written from those files, not from guesses or one-off probes.
type: decision
tags: [decision, observability, coverage, ledger, senses]
aliases: [observe first, full observability, the coverage gate, no blind spots]
status: verified
lastUpdated: 2026-11-24
verifiedAgainst: ruststeve@31545c1
sourceRefs:
  - "ruststeve:crates/results/src/observe.rs#fn every_clientbound_packet_is_classified() {"
  - "ruststeve:crates/observe/src/lib.rs#pub fn diff(prev: &Snapshot, next: &mut Snapshot, w: &World) -> Vec<Stamped> {"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[observability]]"
  - "[[one-bot-first]]"
  - "[[measurement-rules]]"
---

# Observe first

## The decision
Bridger, 2026-11-22, after issue 16 stopped on three walls each found only after a 3-to-4-hour race and probes: "one of our main issues is that you have SUPER POOR observability". Full observability is the one major change before more behaviour work (issue 16e, `docs/problems/observability.md`).

- **The coverage row is the gate.** A race's summary prints coverage first. Any `!!` (a missing file, a task with no reason or no end, a dropped ledger event, an item slot or entity data cut short, a packet the stages need thrown away) is fixed before any behaviour PR merges.
- **Every clientbound packet is classified** in `config/observe.toml`: decoded, wanted, or skipped with its reason. A test fails on one in no list, so a game update adds nothing unseen.
- **A wall is written from the files.** Each wall in `docs/problems/` cites the case's story, ledger lines (tools worn and broken with their task, food and health, signs) and senses rows; a probe is for what the files cannot answer.
- **A fix counts its firings** in the ledger before its result is read (clojurecraft had two fixes that never fired).
- **Truth never reaches the bot.** Server truth is asked by the shell over RCON for the case's `blind:` lines; no core crate may depend on `results` or `rcon` (a test holds it). Strict no X-ray stands.

## Why
Before 16e: about 37 of 141 play packets decoded (2.5 million thrown away in one race, unread); tool wear parsed and dropped, so a pickaxe breaking was only "items lost" at the end and the planner could not see wear; a 64-note ring, 4,417 notes "left out" of one case; the same signature for every run. Re-reading three old recordings with the new decoders found in minutes what those races had taken hours and probes to find, and corrected one cause: the pickaxe of wall 7 broke on stairs down for iron, not on the climb ([[observability]]).

## See also
- [[observability]] - what each run leaves and how to read it.
- Issue 18b adds the hunt's facts before the hunt is built: anger, blows, deaths, pearls dropped and picked ([[race-and-results]]).
- [[one-bot-first]] - done is one real bot past the stage; observing is how to see why one did not.
