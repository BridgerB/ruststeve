---
title: Glossary
type: glossary
tags: [brain, glossary]
status: verified
lastUpdated: 2026-11-11
---

# Glossary

## ruststeve
- **Brain**: the core's whole state, `{ world, plan, expectations, rng, config }`, owned by the tick loop; see [[sans-io-core]].
- **Core / shell**: `bot-core` decides with no I/O; the shell does everything that touches the world; see [[workspace-crates]].
- **Tick**: one step of the loop and the core's only clock, game ticks never ms; see [[tick-loop]].
- **Input / Output**: everything the core learns in one tick (observations, completions) and decides (commands, requests, events); see [[sans-io-core]].
- **Observation**: an immutable fact the shell reports; the world model is their fold; see [[world-model-fold]].
- **Command / Reason**: an intent for the shell to carry out, with why; see [[brain-events]].
- **BrainEvent / CombatFrame**: decisions as data; the per-tick fight record; see [[brain-events]].
- **Expectation**: what a command should make the world do, with a deadline; a miss is how "stuck" is detected; see [[expectations-and-watchdog]].
- **Watchdog**: fails a task whose progress metric stops improving within its patience.
- **Phase / Task / Tactic**: where in the run, what now, what this tick; see [[phases-and-task-stack]].
- **Recording / Checkpoint / verify**: the binary file of a run (MessagePack, zstd, commit-pinned); a serialized `Brain` every 1,200 ticks; replay compared to the recorded outputs; see [[recordings]].
- **Open loop / closed loop**: replaying recorded inputs vs playing against a world that answers; see [[replay-limits]].
- **Flight recorder / incident**: the last 600 ticks of snapshots, dumped when something trips; see [[incident-to-regression]].
- **Body**: the shell's trait to the game; in ruststeve our own wire layer; see [[body-boundary]].
- **Wire layer**: our own byte codec, packet table and chunk decoder; see [[wire/_moc|Wire]].
- **Oracle**: clojurecraft, until ruststeve passes it; see [[clojure-is-the-oracle]].
- **Runners**: GitHub Actions runners, where all code runs: 20 at once, 16 GB each; see [[runners-not-the-mac]].
- **Checkpoint start**: a kit and a place at the start of one stage, so every lane can play it directly; see [[measurement-rules]].
- **Stuck stage**: the lowest stage under about 5% in the real or sim race; see [[cc-loop-rules]].
- **Parity table**: the same recorded moments run in sim and real, outcome classes compared; a sim number counts only after it; see [[measurement-rules]].
- **Case file**: how a run ended: killer, first hit, task and its why; see [[observability]].
- **Batch file**: one lane batch's runs, stage reached and endings, in the recording format; see [[data-out-no-database]].
- **Progress invariant**: what must improve every tick of a long task, or it fails fast; see [[progress-invariants]].

## Game and protocol
- **26.1.2 / 775**: the Minecraft version and its protocol number, pinned by every bot; see [[wire-packet-ids]].
- **Simulation distance**: chunks round a player in which the server ticks mobs; beyond it they are sent but frozen (3 on the race servers); see [[mc-mob-facts]].
- **State id**: the counter on every window update, echoed in each click; see [[wire-container-click]].
- **Paletted container**: how a chunk section's blocks travel; see [[wire-chunk-sections]].
- **Cast (the portal)**: making obsidian from lava and water in a mould; see [[mc-portal-cast]].
- **Exposed face**: a block with an air or see-through neighbour; only these may be acted on; see [[hist-no-xray]].

## clojurecraft
- **World value**: its one flat map of namespaced attributes in one atom; see [[cc-world-value]].
- **Event / effect**: reducer input (`:start`, `:packet`, `:tick`, `:go`, `:closed`) and output as data; see [[cc-reducer-and-effects]].
- **Goal row / intent**: data in `goals.edn`; the value advanced one tick at a time; see [[cc-goals-and-intents]].
- **Needs planner**: the walk from a target back to the next act; see [[cc-plan-and-make]].
- **Sim / sim race / snapshot**: the pure server model; whole runs against it; a kept moment a fix is judged on; see [[cc-sim]].
- **Gym / race / landing set**: one goal judged by RCON truth; many bots on one server; fixed pre-checked spots; see [[cc-race-and-gym]].
- **Churn**: an intent chosen again every tick while the bot stands still; see [[cc-stall-and-budgets]].

## History
- **steve / typecraft**: the TypeScript bot and its from-scratch SDK; see [[hist-walls]].
- **ruststeve v1**: the first Rust port, reset on 2026-11-11; see [[hist-ruststeve-v1]].
- **The handoff**: clojurecraft's closing document, which outranks `FUNCTIONAL.md`; see [[cc-handoff]].
