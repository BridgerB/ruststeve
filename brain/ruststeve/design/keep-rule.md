---
title: The keep rule
description: What a nearly full pack drops first, as data - config/keep.toml's rows (the stages that need an item and how many, and where each row applies), plan::keep's rank (least needed first), the drop rule in Plan::equip that asks it, the ledger's drop and want lines, and the per-run dropped-then-wanted count.
type: reference
tags: [plan, inventory, keep, junk, drop, ledger]
aliases: [keep.toml, keep table, plan::keep, Keep::rank, Rank, junk list, junk drop, drop rule, want line, dropped-then-wanted, least needed first, keep::Context]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@f2408e5
sourceRefs:
  - "ruststeve:config/keep.toml#name = \"string\""
  - "ruststeve:crates/plan/src/keep.rs#pub fn rank(&self, kind: ItemKind, held: i32, reached: Option<Stage>, ctx: &[Context]) -> Rank {"
  - "ruststeve:crates/plan/src/keep.rs#pub fn least_needed("
  - "ruststeve:crates/plan/src/keep.rs#pub enum Context {"
  - "ruststeve:crates/plan/src/keep.rs#pub(crate) fn craft_want(&mut self, w: &World, f: craft::CraftFailure, tick: u64) {"
  - "ruststeve:crates/plan/src/keep.rs#fn the_rows_are_the_old_junk_lists() {"
  - "ruststeve:crates/plan/src/lib.rs#fn equip(&mut self, w: &World, tick: u64) {"
  - "ruststeve:crates/results/src/keep.rs#pub fn keep_counts(l: &Ledger) -> KeepCounts {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[goal-table-and-needs]]"
  - "[[entities-and-inventory]]"
  - "[[brain-events]]"
---

# The keep rule

Issue 77. A nearly full pack (fewer than `JUNK_FREE_SLOTS`, six, main slots free) drops a stack. Which stacks may go, and which goes first, is data: `config/keep.toml`. Before this, three hand lists in goals.toml (`[survival]`, `[stronghold]`, `[fortress_way]` `junk`) said what may go, and the first such slot went.

## Key files
- `config/keep.toml`: one `[[item]]` row per item and context: `name`, `when` (default `["always"]`) and `needs = [["<stage>", n]]`, each count with a comment giving its source.
- `crates/plan/src/keep.rs`: `Keep::parse`, `Keep::rank`, `Keep::junk`, `least_needed`, the fallback `Plan::junk_lists`, and the want lines (`craft_want`, `smelt_want`).
- `crates/plan/src/lib.rs`, `Plan::equip`: the drop rule. `junk_slot_by` and `cursor_step_by` choose by rank. `junk_slot` and `cursor_step` are the same with one rank for all, which is the old first-slot order.
- `crates/results/src/keep.rs`: `keep_counts`, the run's drops, wants and `dropped_then_wanted`.

## How it works
- **Stages** are `world::stage::Stage` by their results names (`bow_armed`, `win`, ...). A need counts until the run passes its stage (`w.stage_reached <= stage`; with no stage reached yet, every need counts). The count kept is the largest need still counting.
- **Rank** (`Keep::rank(kind, held, reached, ctx)`), least needed first:
  - `Surplus`: no need counting, or more held than the count kept.
  - `Needed { stage, by }`: the nearest counting stage that needs it. A later stage is less needed.
  - `Unlisted`: no row where the contexts hold. Never dropped.
- **The drop**: among the main-slot stacks whose drop still leaves the kind's count, the one whose remainder ranks least needed goes first. A tie goes to the lowest slot. The ledger line `drop <Kind> from slot <n>` now gives as its reason the rank, the stage reached and the contexts, e.g. `fewer than 6 main slots free; needed: 64 by win; stage nether; always`. A stack dropped off the cursor is `drop <Kind> from the cursor` (it was `cursor stack to slot -999`).
- **Contexts** (`keep::Context`, a row's `when`) are the old gates, exactly as `Plan::equip` had them (lead-77, 2026-11-27). Widening them is a later change, judged on races.
  - `always`: the [survival] list. Everywhere with a goal table (a planner without one dropped nothing), but not in the overland hut, where nothing is dropped.
  - `stronghold_aim`: the [stronghold] list. The race aim in the overworld with an eye held or the room known.
  - `fortress_tunnel`: the [fortress_way] list (Q11). The Nether's home leg once the tunnel has begun.
  - `home_leg`: Q8. The Nether's home leg with the top-up over and the tunnel not begun. Netherrack keeps 192 and basalt 64 (`home::NETHERRACK_KEPT`, `BASALT_KEPT`) as rows.
  - For a kind, the first context in that order that holds and has a row counts: the old lists' find-first order, so survival's gravel 16 shadows the fortress list's 0.
- **Kept in step**: goals.toml keeps its junk lists. `junk_place`, the stone pressing and the full-pack wait in `nether_task` still read them. `the_rows_are_the_old_junk_lists` holds keep.toml's counts equal to those lists, as `Plan::equip` composed them, in every context and at every stage. Change both files together.
- **Compiled in**: keep.toml is `include_str!`'d (commit-pinned like the code) and parsed into `Plan::keep` at construction. An edit needs a rebuild, unlike goals.toml, which the shell reads. A table that does not parse is never an empty table, because an empty table would drop nothing and fill the pack. The drop rule falls back to the goals.toml lists and notes `keep table unreadable: <error>` once. The parse test makes that path unreachable for the committed file.
- **Want lines**: a craft that fails `Missing` writes `want <Kind>|<Kind> <n> for craft <recipe>` for each ingredient set it is short of (counting the inventory, the cursor and the grid). A smelt that fails `Missing` writes `want <Kind> <n> for smelt <Result>` for its input, or `want fuel 1 ...`. When nothing is short (a full pack or a stale grid made the failure), the craft writes `craft <id> missing with every ingredient held` instead, and the smelt `smelt <Result> missing with input and fuel held`.
- **The count**: `results::keep::keep_counts` reads a ledger's notes. `dropped_then_wanted` counts the wants that name a kind dropped at an earlier tick of the run. The issue's bar is 0. Its summary row waits on issue 56 (`text.rs` is fenced, lead-77).

## Gotchas
- A drop line's reason ending in `the old junk lists`: the keep table failed to parse, and the fallback is running. Look for the `keep table unreadable` note.
- `Needed` ranks are about what the drop leaves. A kind whose drop would go under its count is never a candidate at all.
- The forest base has its own drop list (`plan::base::drop_slot`, `[base] drop`). This rule does not cover it.

## Limits
- Only failed crafts and smelts write want lines. A goal the needs planner skips for want of an item writes none yet.
- Contexts are the old gates, not stages. The Nether's leftovers stay put in the Nether before the home leg and in the End, as before.

## See also
[[goal-table-and-needs]], [[entities-and-inventory]], [[brain-events]].
