---
title: The port, phase by phase
description: The eight phases of building ruststeve from clojurecraft, what each delivers, and the gate that must pass before the next starts.
type: reference
tags: [roadmap, phases, gates, porting]
aliases: [porting procedure, phase 0, inventory, gates, roadmap]
status: verified
lastUpdated: 2026-11-11
verifiedAgainst: ruststeve@c87ac55
sourceRefs:
  - "ruststeve:FUNCTIONAL.md#Port in eight phases. Each ends at a gate with a concrete definition of done."
  - "ruststeve:FUNCTIONAL.md#docs/PORT_NOTES.md: every dispatch keyword (each becomes an enum variant), every threshold and magic number (each becomes a Config field), how the Clojure bot records and replays runs, which protocol body it uses (decides 7.1), and any code you will not port, with the reason."
  - "ruststeve:FUNCTIONAL.md#Gate: a 10-minute recorded run on the local server."
  - "ruststeve:FUNCTIONAL.md#Gate: the Rust success rate and milestone times are at least the Clojure baseline over the same N attempts. No todo!() anywhere in bot-core."
related:
  - "[[ruststeve/roadmap/_moc|Roadmap]]"
  - "[[cc-namespaces]]"
  - "[[clojure-is-the-oracle]]"
---

# The port, phase by phase

> **Superseded for order and gates by [[first-twenty-issues]]** (the clojurecraft handoff, which Bridger trusts over this guide). Kept for the guide's intent: observability and replay before intelligence, and the phase-0 inventory, which this brain already covers ([[cc-namespaces]], [[cc-lessons]]).

Observability and replay are built *before* intelligence, so every behaviour ported is debuggable from its first run. No phase starts until the previous gate passes.

## Key files
- `FUNCTIONAL.md`, section 15.

## How it works

| Phase | Delivers | Gate |
|---|---|---|
| 0 Inventory | read all of clojurecraft; write `docs/INVENTORY.md` (one row per namespace: responsibility, state, side effects, Rust destination), `docs/LESSONS.md` (the lessons register), `docs/PORT_NOTES.md` (dispatch keywords → enum variants, thresholds → `Config`, how it records and replays, which body, what is not ported) | Bridger approves all three |
| 1 Skeleton | workspace, crates, CI (fmt, clippy with `clippy.toml`, nextest, the `cargo tree` I/O check, `verify` over golden); core types; a `Brain` whose `step` returns empty; `bot-replay` load/verify/print | CI green; a hand-written recording round-trips and verifies |
| 2 Body and recording | the body adapter, tick loop, recorder; a scripted brain (walk a square, mine a block); snapshot endpoint and visualizer | a 10-minute recorded run verifies; the world rebuilt from the recording equals the live snapshot; viewable from the file alone |
| 3 Port the core | bottom-up: kinds; `WorldModel::apply`; queries (sight, nearest, counts, reach); pathfinding; tasks one at a time; tactics and rule tables, combat last; planner, recovery, watchdog; phases | per module: tests green, differential agreement investigated, no `todo!()` |
| 4 Early-game parity | measure clojurecraft's early game N times first; Rust spawn → Nether with a scenario per milestone; golden recordings | Rust success rate and milestone times at least clojurecraft's over the same N; no `todo!()` in `bot-core` |
| 5 Nether and blazes | fortress, spawner approach, blaze fights, with [[blaze-debugging-method]] for every failure | a blaze-rod success rate Bridger sets, plus a regression recording per distinct failure |
| 6 Endgame | pearls, eyes, stronghold, portal, End, dragon | spawn-to-dragon runs at a rate Bridger sets |
| 7 Hardening | performance budget under soak; schema upcasters; `docs/ARCHITECTURE.md` current | |

Working rules: one module or task kind per PR with its tests; docs move in the same PR; never port dead code (record it in `PORT_NOTES.md`); stuck an hour on a borrow error → re-read the ownership ladder, the fix is a layout change.

## Gotchas
- Phase 0 overlaps this brain: [[cc-namespaces]] and the oracle pillar are the start of `INVENTORY.md`, and [[cc-lessons]] of `LESSONS.md`. Bridger put the brain first (2026-11-11).
- The guide's phase 2 "body" choice is already made: our own protocol layer ([[own-protocol-not-azalea]]).

## See also
- [[clojure-is-the-oracle]] - why clojurecraft sets every bar.
- [[blaze-debugging-method]] - phase 5's method.
