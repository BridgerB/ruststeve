---
title: One bot first
description: Bridger's decision (2026-11-19) that ruststeve's goal is one bot from spawn to the dead dragon: each stage is done when one real bot passes it from the previous stage's snapshot, every attempt resumes from the furthest snapshot, and rates and speed wait until the game is won.
type: decision
tags: [decision, roadmap, snapshots, frontier, done-conditions]
aliases: [one bot first, win once, the frontier, snapshot and resume, no rates until the win]
status: verified
lastUpdated: 2026-11-19
verifiedAgainst: ruststeve@d0e92fc
sourceRefs:
  - "ruststeve:crates/results/src/snapshot.rs#pub fn offline_uuid(name: &str) -> String {"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[measurement-rules]]"
  - "[[first-twenty-issues]]"
  - "[[race-and-results]]"
---

# One bot first

## The decision
Bridger, 2026-11-19: "the goal is 1 bot ... 1 full end to end bot that goes from spawn to killing ender dragon ... we can keep taking from its snapshot after the checkpoint and resume all bots from there ... only AFTER we win the game will i go back and generalize to win MORE games and IMPROVE speed."

- **Done for a stage** = one real bot passed it, starting from the previous stage's snapshot, and its snapshot is saved. Rates are reported, never a gate.
- **Every attempt starts at the frontier**: the furthest snapshot a real bot has made (`config/frontier.toml`), never from spawn or a made-up checkpoint.
- **Generalising** (more bots passing, speed) is a phase after the dragon dies.

## Why
- Rate targets spent days polishing stages the bot already passes (the portal from 10% to 19%) while the stages after them were never reached; the unknown walls are there. clojurecraft lost four weeks on a late stage it reached late ([[cc-stage18-lessons]]).
- A made-up checkpoint hides real gaps: the teleported `nether` start passed issue 15 while walking into the portal did not exist. A snapshot of a bot that got there carries the real inventory, tools, position, terrain and memory.
- A stage passed one time in twenty is enough when the next attempt starts right before it: the runners work the frontier instead of replaying the opening.

## How
- **Capture**: `race` takes a snapshot the first time any bot in a lane reaches a stage above the lane's floor: `save-all flush` over RCON, the server's `world/` copied, the bot's memory and `snap.toml` (stage, bot, offline UUID, position, dimension, commit). Races and the marathon publish each as `snapshot-<stage>-<run>-<lane>.tar.zst`.
- **Resume**: `race.yml` `snapshot=frontier` (or `<tag>/<asset>`) restores that world before the server starts, gives every bot the snapshot bot's player file under its own offline UUID (`results offline-uuid`), and seeds each bot's memory (`--memory`). The `snapshot` start gives nothing more.
- **The frontier** moves by a commit to `config/frontier.toml` each time a bot first passes a stage.
- **The marathon** stays end to end from spawn on the latest main: the proof the whole chain runs.

## Supersedes
The % done-conditions in [[first-twenty-issues]] for issues 16 to 20, until the dragon is dead; [[measurement-rules]]' pooled pairs are for the generalising phase.
