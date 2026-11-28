---
title: The keep rule
description: What a nearly full pack drops first, as data - config/keep.toml's rows (the stages that need an item and how many, and where each row applies), plan::keep's rank (least needed first), the drop rule in Plan::equip that asks it, the ledger's drop and want lines, and the per-run dropped-then-wanted count.
type: reference
tags: [plan, inventory, keep, junk, drop, ledger]
aliases: [keep.toml, keep table, plan::keep, Keep::rank, Rank, junk list, junk drop, drop rule, want line, dropped-then-wanted, least needed first, keep::Context]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@61bedd6
sourceRefs:
  - "ruststeve:config/keep.toml#name = \"string\""
  - "ruststeve:crates/plan/src/keep.rs#pub fn rank(&self, kind: ItemKind, held: i32, reached: Option<Stage>, ctx: &[Context]) -> Rank {"
  - "ruststeve:crates/plan/src/keep.rs#pub fn least_needed("
  - "ruststeve:crates/plan/src/keep.rs#pub enum Context {"
  - "ruststeve:crates/plan/src/keep.rs#pub(crate) fn craft_want(&mut self, w: &World, f: craft::CraftFailure, tick: u64) {"
  - "ruststeve:crates/plan/src/keep.rs#fn the_rows_are_the_old_junk_lists() {"
  - "ruststeve:crates/plan/src/lib.rs#fn equip(&mut self, w: &World, tick: u64) {"
  - "ruststeve:crates/results/src/keep.rs#pub fn keep_counts(l: &Ledger) -> KeepCounts {"
  - "ruststeve:crates/plan/src/make.rs#pub fn make_traced<'a>("
  - "ruststeve:crates/plan/src/base.rs#pub fn drop_slot_by("
  - "ruststeve:crates/plan/src/keep.rs#pub(crate) fn traced_want<'a>("
  - "ruststeve:crates/results/src/keep.rs#pub fn want_source(what: &str) -> Option<&str> {"
  - "ruststeve:crates/results/src/keep.rs#pub fn render(runs: &[RunKeep]) -> String {"
  - "ruststeve:crates/plan/src/keep.rs#pub fn need_in(&self, kind: ItemKind, stages: &[Stage]) -> i32 {"
  - "ruststeve:crates/plan/src/keep.rs#pub(crate) fn goal_wants(&mut self, goal: &str, short: &[crate::make::Short], tick: u64) {"
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
- **The needs planner's wants**: `make::make_traced` returns, beside the choice, every item need its chosen path meets from a producer rather than from what is held (`make::Short`: the need's key, its items, how many are short). A producer skipped, a spare not planned and a branch that failed leave none. `Plan::decide` passes them to `goal_wants`, which writes `want <Kind>|<Kind> <n> for goal <id>` (or `want fuel <ticks> ...`) once an episode: when a key joins the short set, not again while it stays, again if it leaves and comes back. A new goal starts afresh (`Plan::keep_wants`). The sub-planners' `make` calls write the same lines through `Plan::traced_want`, tagged with their source (`keep::Sub`): `want <kinds> <n> for goal <id> (base|eyes|end_portal|stronghold)`. Each source keeps its own episode memory (`Plan::keep_sub_wants`), so one source's goal never resets another's (the director, 2026-11-28: a path that writes no line reads as "fired 0 times").
- **The forest base** (`plan::base::drop_slot_by`) keeps its own gate: only `[base] drop` kinds, never `NEVER_DROP`, under `free_slots`. Its candidates (each kind's first main stack) go least needed first by `Plan::keep_rank`, then in the list's order. Its drop line `base: drop <Kind> from slot <n>` gives the rank.
- **A leg's need** (`Keep::need_in(kind, stages)`, for 77-stash's stash before the Nether and the End): the largest count any of the kind's rows, in any context, names for a stage in the span, in `needs` or in `during`. It is the largest, not the sum, because a count is what is held by its stage. A row's `during` lists what a stage uses while it runs (the Nether's 64 cobblestone and cobbled deepslate, 16 dirt and 16 gravel). Only need_in reads it: rank and the drop rule never see it (lead-77, 2026-11-28). Letting rank see in-leg needs is a later change, judged on races.
- **The home chest's row** (77-stash, wall 5; [[chests]]): `chest`, `when = []`, `needs = [["nether", 1]]`. No drop gate holds it, so rank calls a carried chest `Unlisted`, and a full pack never drops it, before the Nether or after. That is stronger than `Needed`, and the pinned old lists stay as they were. `need_in` reads it in any context, so the stash never puts the chest into itself.
- **The count**: `results::keep::keep_counts` reads a ledger's notes, the base's drop lines included. `dropped_then_wanted` counts the wants that name a kind dropped at an earlier tick of the run. Notes and facts are read on one timeline. A death (the `Death` fact or the `died (...) and respawned` note) clears the drop memory, so a want after it counts against a drop only if the kind was dropped again after the death. Its kinds, and any the death took within `DEATH_LOSS_TICKS`, count as `wanted_after_death` instead (race 46140744043: all six dropped-then-wanted there had a death between). `under_keep` lists each drop (not the base's) whose next item fact left less than the keep at the drop line's stage and contexts. It should be empty. `wants_by_source` counts the wants by `want_source`: craft, smelt, goal (the main planner), or a sub's tag. The issue's bar is 0. Its summary row waits on issue 56 (`text.rs` is fenced, lead-77). Until then `results keep <dirs>` prints it: a row a run (seed from the nearest `lanes.toml`, bot, drops, wants, every want by source and kinds with its lines and count, dropped-then-wanted with each kind's drop and want ticks, and the stash paths: stash and fetch done or failed, the stash gate's outcomes, home chests placed, the fetch after a respawn), a total a seed, then the total, flagged `!!` over 0.

## Gotchas
- A drop line's reason ending in `the old junk lists`: the keep table failed to parse, and the fallback is running. Look for the `keep table unreadable` note.
- `Needed` ranks are about what the drop leaves. A kind whose drop would go under its count is never a candidate at all.

## Limits
- A want names its need's every kind (a tag's whole set), so a line for logs is long; it is written once an episode, not once a tick.
- Contexts are the old gates, not stages. The Nether's leftovers stay put in the Nether before the home leg and in the End, as before.

## See also
[[goal-table-and-needs]], [[entities-and-inventory]], [[brain-events]].
