---
title: It is just ruststeve
description: Why the new build is called ruststeve (not v2), lives on a reset branch with none of v1's code, and keeps every tool in Rust.
type: decision
tags: [decision, naming, reset, tooling]
aliases: [not v2, the reset, functional branch, all Rust, no TypeScript tools]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55 ruststeve-v1@9e1567f
sourceRefs:
  - "ruststeve:crates/brain-check/src/main.rs#//! Standard library only: the checker must build in seconds with no registry access."
  - "ruststeve-v1:CLAUDE.md#An autonomous Ender-Dragon speedrun bot (random seed, any%, no human input) — a Rust port of the TypeScript steve bot"
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[clojure-is-the-oracle]]"
  - "[[hist-ruststeve-v1]]"
---

# It is just ruststeve

## The choice
- **The name is ruststeve.** Not "v2", not "Rust v2". It is the next thing done in this repository; when it is built out, Bridger merges it on top of main and it is simply ruststeve (Bridger, 2026-11-11). In the brain, the first port is called "ruststeve v1" only where history needs telling apart.
- **A clean start.** Branch `reset` deleted everything but `README.md` and `LICENSE`; branch `functional` added `FUNCTIONAL.md`, then this brain. No v1 code carries over by habit; v1 is read by commit (`ruststeve-v1` in `brain/sources`) when its lessons are needed.
- **Rust only.** Every tool, checker and script in this repository is Rust (Bridger, 2026-11-11: no `.ts` or `.js`). The brain checker is the `brain-check` crate, standard library only so it builds in seconds.
- **Runners, not the Mac.** Code runs only on GitHub Actions runners ([[runners-not-the-mac]]). The `functional` and `reset` branches are not pushed yet; the first push needs Bridger's go.

## What was rejected
- Editing v1 into shape: Bridger's reason for the reset was that incremental modification of v1 would never reach a different architecture.
- TypeScript one-off scripts, the default elsewhere on this machine: not in this repository.

## What would change the answer
Bridger's call only.

## See also
- [[hist-ruststeve-v1]] - what v1 was.
- [[clojure-is-the-oracle]] - what replaces it as the model.
