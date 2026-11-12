---
title: Code runs on runners, never on the Mac
description: Why every test, sim, race, replay and probe runs on GitHub Actions runners (20 at once, 16 GB each) and the Mac only edits, and what may run locally.
type: decision
tags: [decision, compute, ci, runners]
aliases: [no compute on the Mac, GitHub Actions, runners, 20 runners, 16 GB, probe, where code runs]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: clojurecraft@be85a92
sourceRefs:
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#No compute on the Mac. Edit, cargo fmt, cargo clippy and git locally. Tests, sims, races and probes run on GitHub runners."
  - "clojurecraft:docs/HANDOFF-RUSTSTEVE.md#No compute on the dev machine. Everything that runs code goes to a runner (probe.yml for one-off questions). It kept the laptop free and made every result reproducible from a run id."
  - "clojurecraft:LOOP.md#No compute on the Mac: no tests, sims, replays, judges, gyms, probes or builds."
related:
  - "[[ruststeve/decisions/_moc|Decisions]]"
  - "[[testing-kinds]]"
  - "[[first-twenty-issues]]"
---

# Code runs on runners, never on the Mac

## The choice
The Mac edits code. Everything that runs it (tests, sims, races, replays, probes, benchmarks) runs on **GitHub Actions runners: 20 at once, 16 GB each** (Bridger, 2026-11-11). The loop is: change code, push a branch, let a workflow run it, read the run's logs, push again. A one-off question that needs code run goes to a probe workflow, as clojurecraft's `probe.yml` did.

Locally only: editing, `cargo fmt`, `cargo clippy`, git, `gh`, reading files, and the seconds-long brain checker (`cargo run -q -p brain-check`), the same allowance clojurecraft gave `bb check`.

## What was rejected
- **"Mostly local"** (Bridger's word on 2026-11-11, before this decision) and `FUNCTIONAL.md`'s `bacon` on every save. Replaced by runners.
- **Running on the Mac "just this once"**: results then depend on a machine nobody else can reproduce; on the runners every result has a run id.

## Consequences
- Running anything needs a pushed branch, and pushes still need Bridger's go. The `functional` and `reset` branches are not pushed yet; ask before the first push.
- The runners are the race hardware too: a real race lane is one runner with its own vanilla server. clojurecraft's 4-vCPU runners starved the server when the bot JVM took about 3.3 cores; give each bot a CPU budget ([[cc-stage18-lessons]]).
- clojurecraft's workflows are disabled, so all 20 runners are free for ruststeve ([[cc-handoff]]).

## What would change the answer
Bridger only.

## See also
- [[testing-kinds]] - what runs where.
- [[first-twenty-issues]] - issue 7 builds the workflows.
