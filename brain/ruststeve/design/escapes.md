---
title: Escapes
description: How the bot gets out of water, lava and fire (plan::escape, issue 12b, a port of the core of clojurecraft's trap.clj and swim.clj) - always-rows checked every tick ahead of every goal, the trap re-derived with hysteresis, one bank chosen and held, digging a lid or a wall when stuck, a notch when there is no bank, fleeing lava at once, and the parity rows that run the planner.
type: reference
tags: [plan, escape, survival, water, lava, fire, always-rows]
aliases: [always-rows, escape-water, escape-lava, escape-fire, Escape, Traps, TrapKind, swim-out, flee, bank, notch, lid, Aim::Hold, plan:hold, dry land]
status: verified
lastUpdated: 2026-11-23
verifiedAgainst: ruststeve@3c1b500 clojurecraft@be85a92
sourceRefs:
  - "ruststeve:crates/plan/src/escape.rs#pub fn tick(&mut self, w: &World, body: &physics::Body, door: bool) -> Option<TrapKind> {"
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
- `config/goals.toml`: the `[[always]]` rows (`escape-water`, `escape-lava`, `escape-wall`, `escape-fire`, priority 0), read into `plan::make::Table::always`.
- **Lava at the door** (blaze wall 22): `Traps::tick(w, body, door)` also answers lava over the head or beside the head or feet (`escape::lava_door`) as the lava trap, unless the task is a cast or a scoop. The escape seals that cell with a held block (`MAX_LAVA_SEALS` 16, clicked on a full block beside or under it) before it flees. Nether lava ran down three bots' own stairs and killed each in 80 ticks.
- `crates/plan/src/escape.rs`: `Traps`, `Escape`.
- `crates/plan/src/lib.rs`: `Plan::always`, called every tick before the task and before the "finished" check, so it runs after the aim is met too.
- `crates/bot/tests/escape.rs`: a pond, a shaft under a dirt lid, a pit with no bank (with a wooden pickaxe and by hand), lava and fire, each closed-loop against the model.

## How it works
- **The trap** (`Traps::tick`, `trapped-tick`): lava on any contact; water once the eyes have been under for 20 ticks or the body afloat for 60; a fire block the body's box touches. Water and lava hold until dry land (on the ground, no water in the box or at the eyes, a full block under the feet) has been held 6 ticks. Only the kinds the table's always-rows name are answered.
- **Preemption**: a trap of a kind not already escaping cuts the task in hand (a plan event `escape-<kind>` with what was cut), starts an `Escape`; the trap clearing ends it (`out of the <kind>`) and the planner chooses again at once.
- **Water**: one bank, chosen once and held: the nearest standable cell (a full floor, two clear cells, no lava beside) within 16 on the plane, from 3 below the water's surface cell to 1 above. Forward and jump into it; the physics' out-of-liquid impulse is the only lift. No progress (0.05 blocks) for 40 ticks → dig what is in the way (a lid over the head with the eyes under, then the wall at head height toward the bank, never the bank's floor), else refuse the bank and choose again. No bank → a notch: a cell at the surface beside the water with a full floor and something to dig in it and over it, the quickest to break with what is held (dirt before stone by hand), then the nearest; its cells are dug only with the eyes out of the water (under it the break time is five times longer). **The water it came for** (blaze wall 10): afloat with an empty bucket and no water bucket, the escape first fills the bucket from the nearest remembered source the eye has a clear use on (`Scoop::from_here`), once, then swims out. **A pour from above** (blaze wall 11): with the eyes under and water in the cell over the head, a held block (cobblestone first) is put into that cell, clicking the stone round it (`escape::against`), up to 6 tries; a lid or wall with water over or behind it is never dug (`holds_back_water`). A diagonal bank is steered at through the open cardinal cell first (`escape::waypoint`, blaze wall 14: straight at it a stone corner held a bot 25,000 ticks); the escape keeps a trail of its last 12 choices, printed when it stalls.
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

## Issue 13b
- **A wall** (`TrapKind::Wall`): the eyes in a full block (gravel fallen into a tunnel, `in_wall` damage): the eye and feet cells are dug with the best tool held.
- **A swim off an island** (`Escape::swim`, `Task::Swim`): the water escape's machinery with its bank preset to dry footing 10 to 32 blocks away, started by the planner when it gives up beside water ([[planner]]); the water row does not cut it (it is in water on purpose), lava, fire and a wall do. Model test: an island 3 wide in a lake 41 wide.

## Issue 16: what an escape says (docs/problems/nether-walk.md)
- An escape's end is written with what it did (`Escape::report`: seals, digs, refusals, the trail): on "out of the lava" and when the run ends with the escape running ("escape-lava ended with the run"). Its start says whether the lava was at the door, and where.
- `lava_door` also sees lava one cell over the head's height beside its column (the ring at y + 2), last in its order.
- Soul sand is dry land for a trap to clear on.


## See also
- [[mc-water]] - the water physics the escape relies on.
- [[server-model]] - the damage the trap costs.
- [[food-and-hunting]] - the eat and hunt rows after these.
