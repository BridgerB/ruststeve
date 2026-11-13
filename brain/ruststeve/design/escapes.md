---
title: Escapes
description: How the bot gets out of water, lava and fire (plan::escape, issue 12b, a port of the core of clojurecraft's trap.clj and swim.clj) - always-rows checked every tick ahead of every goal, the trap re-derived with hysteresis, one bank chosen and held, digging a lid or a wall when stuck, a notch when there is no bank, fleeing lava at once, and the parity rows that run the planner.
type: reference
tags: [plan, escape, survival, water, lava, fire, always-rows]
aliases: [always-rows, escape-water, escape-lava, escape-fire, Escape, Traps, TrapKind, swim-out, flee, bank, notch, lid, Aim::Hold, plan:hold, dry land]
status: verified
lastUpdated: 2026-11-13
verifiedAgainst: ruststeve@b86d924 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/escape.rs#pub fn tick(&mut self, w: &World, body: &physics::Body) -> Option<TrapKind> {"
  - "ruststeve:crates/plan/src/escape.rs#pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>) {"
  - "ruststeve:crates/plan/src/lib.rs#fn always(&mut self, w: &World, body: &physics::Body, tick: u64) -> Option<Controls> {"
  - "ruststeve:config/goals.toml#id = \"escape-water\""
  - "ruststeve:crates/bot/tests/escape.rs#fn a_pit_with_no_bank_is_notched() {"
  - "clojurecraft:src/clojurecraft/trap.clj#(defn trapped-tick"
  - "clojurecraft:src/clojurecraft/swim.clj#(def bank-radius 16)"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[planner]]"
  - "[[goal-table-and-needs]]"
  - "[[mc-water]]"
  - "[[digging]]"
  - "[[parity-table]]"
---

# Escapes

Issue 12b. clojurecraft's first three always-rows, as data and a small state machine: nothing is stored about the task they cut, since the needs planner re-derives it from what is held once the trap clears.

## Key files
- `config/goals.toml`: the `[[always]]` rows (`escape-water`, `escape-lava`, `escape-fire`, priority 0), read into `plan::make::Table::always`.
- `crates/plan/src/escape.rs`: `Traps`, `Escape`.
- `crates/plan/src/lib.rs`: `Plan::always`, called every tick before the task and before the "finished" check, so it runs after the aim is met too.
- `crates/bot/tests/escape.rs`: a pond, a shaft under a dirt lid, a pit with no bank (with a wooden pickaxe and by hand), lava and fire, each closed-loop against the model.

## How it works
- **The trap** (`Traps::tick`, `trapped-tick`): lava on any contact; water once the eyes have been under for 20 ticks or the body afloat for 60; a fire block the body's box touches. Water and lava hold until dry land (on the ground, no water in the box or at the eyes, a full block under the feet) has been held 6 ticks. Only the kinds the table's always-rows name are answered.
- **Preemption**: a trap of a kind not already escaping cuts the task in hand (a plan event `escape-<kind>` with what was cut), starts an `Escape`; the trap clearing ends it (`out of the <kind>`) and the planner chooses again at once.
- **Water**: one bank, chosen once and held: the nearest standable cell (a full floor, two clear cells, no lava beside) within 16 on the plane, from 3 below the water's surface cell to 1 above. Forward and jump into it; the physics' out-of-liquid impulse is the only lift. No progress (0.05 blocks) for 40 ticks → dig what is in the way (a lid over the head with the eyes under, then the wall at head height toward the bank, never the bank's floor), else refuse the bank and choose again. No bank → a notch: a cell at the surface beside the water with a full floor and something to dig in it and over it, the quickest to break with what is held (dirt before stone by hand), then the nearest; its cells are dug only with the eyes out of the water (under it the break time is five times longer).
- **Lava**: the nearest standable cell within 6 (dy −1 to +2), at once; jump is held in lava.
- **Fire**: the nearest standable cell within 2 (a fire cell is never standable).
- An escape dig collects nothing, and a block water flows into counts as gone ([[digging]]).
- **`Aim::Hold`** (`--goal hold`, `plan:hold` in a moment): no goal, only the always-rows.

## Evidence
- Model tests: all six pass; the lidded shaft takes 619 ticks and costs 8 health; the pit by hand fits in 4000 ticks.
- **Parity** (issue 12b rows, the planner run by `plan:hold`): a stone-walled pond, a lava cup and a fire block on landings 0, 1 and 2: **9 of 9** real against the model locally, down to the final cell.
- The lava rows carry fire resistance: in real the lava is laid under a bot already standing there, which sits in it through the start and the settle before the planner runs and dies in about 80 ticks; the rows check the flee, and 12a's lava rows the damage.

## Limits
- On fire with water near, the bot does not yet walk into it: burning runs out (8 s from a fire block, 15 s from lava).
- An escaped trap leaves no fact for later routes; walks and stairs already refuse water and lava.
- An escape has no timeout; its progress invariant changes the bank or digs, it never gives up while trapped.

## See also
- [[mc-water]] - the water physics the escape relies on.
- [[server-model]] - the damage the trap costs.
