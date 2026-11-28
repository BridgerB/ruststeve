---
title: Development starts
description: Bridger's decision (2026-11-23) that a made-up start (a kit at a place, from config/starts.toml) may be raced to develop a stage, but never counts as an issue's evidence and never becomes the frontier - the win still needs one real bot from a real snapshot - and how that sits beside one-bot-first, which forbade made-up checkpoint starts.
type: decision
tags: [decision, starts, snapshots, frontier, evidence, development]
aliases: [development start, made-up start, dev start, kit start, checkpoint start for development, what counts as evidence]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@a5a7aa4
sourceRefs:
  - "ruststeve:config/starts.toml#name = \"nether\""
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[one-bot-first]]"
  - "[[stage-loop-lessons]]"
  - "[[measurement-rules]]"
---

# Development starts

## The decision
Bridger, 2026-11-23 night, after issue 16 spent twelve hours shipping one fix a race behind a single wall (`docs/problems/blaze-log.md`, "The reassessment"):

- **A made-up start is allowed for development.** A start from `config/starts.toml` (a `kit`, a `place` with its dimension, `places`, `spread`, `setup` commands: the `nether` and `portal` starts are the models) may be raced to work on a stage the real bot has not reached yet, or reaches only after hours.
- **It never counts as an issue's evidence.** No run from a development start goes in a PR body as the done-condition, in a wall's "proven" line, or in a rate.
- **It never becomes the frontier.** `config/frontier.toml` names only a snapshot a real bot made, and no snapshot taken in a race from a development start is published as one.
- **The win needs one real bot from a real snapshot.** An issue closes exactly as before: one real bot passes the stage, starting from the frontier.

## In practice
- Development starts are named `dev-*` in `config/starts.toml` (issues 17 to 20: `dev-spawner`, `dev-fortress`, `dev-stronghold`, `dev-nether-fortress`, `dev-portal-room-armed`, `dev-end-arrival`, `dev-end-island`, `dev-end-no-crystals` and others).
- Some are raced on top of the frontier snapshot (`race.yml -f snapshot=frontier`, e.g. `dev-nether-fortress`): the kit, wear and memory are a real bot's and only the place is made up. That is still a development run.

## How it sits beside one-bot-first
[[one-bot-first]] says every attempt starts at the frontier, "never from spawn or a made-up checkpoint". That rule stands for **attempts**: the runs that can close an issue. This note adds a second kind of run, a **development run**, which can find and fix walls but can prove nothing. One-bot-first's reason for the ban is unchanged and is why the limit is strict: a made-up checkpoint hides real gaps (the teleported `nether` start passed issue 15 while walking into the portal did not exist).

So a fix found on a development start is "shipped, unproven" until a real bot from a real snapshot shows its ledger note. Write which kind of run every number came from.

## Why
- **A chain of stages was being walked serially.** Each race from the clean `ready` snapshot took three to four hours to reach a wall in the Nether's first minute; the stages after it (fortress to spawner, the blaze fight) had never been met by a whole bot, so their walls were unknown. With development starts the stages are worked at once, each by its own agent ([[stage-loop-lessons]]).
- **One world per snapshot.** Every race from a snapshot replays one cave and one portal exit, so fixes get tuned to one place. A development start can put the same kit in several places.

## What was rejected
- **Counting a development run as evidence when it is "close enough" to a real bot's kit.** The kit is the least of what a snapshot carries: position, terrain, memory, wear and what the bot failed to bring are the real gaps.
- **Keeping the ban whole.** It cost a day behind one wall while later stages sat unread.

## What would change the answer
A second real snapshot in each stage (several worlds, taken by real bots) would remove most of the need: develop from those instead.

## See also
- [[one-bot-first]] - the bar this does not move.
- [[stage-loop-lessons]] - the test order a development run fits into.
