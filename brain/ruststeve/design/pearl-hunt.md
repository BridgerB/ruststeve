---
title: The pearl hunt
description: How the bot gets ender pearls in the warped forest (plan::burrow and plan::pearls, issue 18e and 18f) - a level burrow two high built of filler that an enderman cannot reach into, and a nine-row table that provokes from the mouth, fights from the back cell, collects from the mouth and moves on, each row with its progress invariant.
type: reference
tags: [plan, pearls, endermen, burrow, combat, rules]
aliases: [burrow, pearls hunt, provoke, pearls_task, pearls_step, keep_off_except, B M O]
status: draft
lastUpdated: 2026-11-24
verifiedAgainst: ruststeve@31545c1
sourceRefs:
  - "ruststeve:crates/plan/src/burrow.rs#pub fn site_avoiding("
  - "ruststeve:crates/plan/src/burrow.rs#pub fn missing(w: &World, b: &Burrow) -> Vec<[i32; 3]> {"
  - "ruststeve:crates/plan/src/pearls.rs#pub fn pearls_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/pearls.rs#pub fn angry_endermen(w: &World) -> Vec<Sighting> {"
  - "ruststeve:crates/plan/src/look.rs#pub fn keep_off_except("
  - "ruststeve:crates/plan/src/defend.rs#pub fn box_distance_of(eye: [f64; 3], pos: [f64; 3], kind: EntityKind) -> f64 {"
  - "ruststeve:config/goals.toml#provoke_ticks = 60"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[combat-and-pacing]]"
  - "[[mc-endermen]]"
  - "[[cc-pearls-rule-table]]"
---

# The pearl hunt

Issue 18 (#23): twelve pearls from a bot that cannot heal (food 17, nothing to eat), so the hunt is built never to be hit. Rebuilt from a written problem statement (`docs/problems/pearls.md`, `docs/problems/pearls-hunt.md`), not ported from clojurecraft's 70 rows ([[cc-pearls-rule-table]]).

## Key files
- `crates/plan/src/burrow.rs` (pure): `Burrow { b, m, o, dir }`, `site`/`site_avoiding` (chosen by cell, nearest the feet within 24), `refuse` (why a site is not one), `missing` (the shell's blocks not there, read from the world each time).
- `crates/plan/src/pearls.rs`: `PearlRules` (`config/goals.toml [pearls]`), `PearlState` (`Plan::pearls`), the rows (`Plan::pearls_step`, one fenced call in `step_inner` after the always-rows) and the tasks (`Plan::pearls_task`: a walk to B, a dig to reopen the mouth, a walk on for a site; the base row calls agent P5's `base_task`).
- `crates/plan/src/look.rs`: `keep_off_except` and `stares_except`, the provoke row's one exemption; `crates/plan/src/defend.rs`: `box_distance_of`, the enderman's 2.9-high box.
- `crates/plan/tests/pearls.rs`: the site, the shell, the roof's cut, the strike, the hold, the provoke, the breach, the collect, on hand-built Nether worlds.

## How it works
- **The burrow**: B (the bot), M (the mouth), O (the enderman's cell), level, one wide, two high; the back, side walls and roof (12 blocks at most) from held cobblestone, cobbled deepslate, blackstone or netherrack, none `#enderman_holdable`. O and its five outward neighbours three clear, four walkable cells beyond O, no lava within 4, no drop over 2 within 3, no nether brick within 8. The enderman's reach stops 0.37 short of B; the bot's 3.0 reaches its box; the roof cuts the eye-to-eye line from B (the plan's section 3).
- **The rows**, in order, the first that holds acts, its id noted as `pearls row <id>` when it changes: `pearls-done`, `breach` (hurt by an enderman, or out of B with an angry one within 4: M sealed from B, eyes down, until none ticks within 16), `strike` (in reach, a full charge, the torso checked not a stare, attack then swing), `hold`, `collect` (a pearl within 6 of O, none angry, from M, never farther than 4 from B), `base`, `build` (find, walk, build, mend, reopen), `provoke` (from M, a calm ticking enderman 6 to 36 off in sight, its eye until the creepy flag, at most 60 server ticks), `wait` (in M, eyes level).
- **Invariants**, each a note: 3 provokes per enderman then skipped 2,400; no health fall in 200 ticks in reach; parked (out of reach 600: a 40-tick look from M, alone, no stare); a fight over 2,400 given up with the mouth sealed; a kill per 12,000 else the burrow moves 32+, two moves without a kill end the hunt sealed in; hits, weapon uses, filler and pickaxe uses per pearl in each kill's note; a weapon at 10 uses or fewer switched; one step out of B a 1,200 ticks without a kill or pickup between (clojurecraft bounced 119 to 1,241 times a run).
- **Outside the burrow** an angry enderman still gets issue 17's sealed pocket (`Plan::ender_threat` yields only to a bot in its built burrow); in the hunt the eyes-down hold is the rows'.

## Limits
- Planner-level tests only until the model's endermen walk to a mouth (agent P4); no real race yet.
- `Plan::hunting` reads `ender.left` and the forest at the feet until P2's `EnderPhase::Hunt` replaces it at integration.
