# Inventory and chests (issue 77)

**Status: machinery built, judged on the model; the real-server check and the seeds runs are open.** #77 closes only after #69's seeds (`config/seeds.toml`) land. Wall numbers cited elsewhere as "inventory wall N" are this file's.

## Needs
From spawn, a long run on a new seed keeps what later stages will want: a full pack drops what no stage ahead needs before what one does, and what the Nether and the End legs do not need but later stages do is put in a chest at a home point before the leg, and taken back after a death and a respawn (#81).

## Risks
- The old drop rule was three hand lists (`[survival]`, `[stronghold]`, `[fortress_way]` junk in `config/goals.toml`): a listed kind dropped past its count whatever came later. The bow line (issue 20, race 46047707279 lane 1) dropped every feather of four chickens and had to rebuild them.
- Changing where a drop is allowed changes every stage's pack. This issue changes only which stack goes first; the gates stay the old lists' (`keep.toml`'s `when`, pinned by a unit test).
- A chest is known only when seen with an exposed face or placed by the bot, and its contents only as its window showed them (no X-ray).
- A stash trip is a detour: it is skipped (noted) under danger health or with a foe in range, never fought through.
- A home chest made at the Nether portal is far from a respawn on another world's spawn: the fetch depends on the walk finding a route there (noted when it does not).

## Done, in world terms
- Unit tests: a full inventory drops cobblestone before string; a stash then a fetch round-trips the counts. Both hold on CI (`plan::keep`, `crates/plan/tests/stash.rs`, `crates/plan/tests/stash_planner.rs`).
- On a real server, the planner's own Place, Stash and Fetch tasks round-trip the counts (parity row `k01-task`, `plan:chest-trip`).
- From spawn on the seeds of `config/seeds.toml` (more than one world), no run ends for want of an item it had held and dropped: counted from the ledger's drop and want lines by `results::keep::keep_counts` (`dropped_then_wanted`, to be 0 in every run).

## What is built (issue-77-inventory f9ba420b)
- **Keep rule as data**: `config/keep.toml` (compiled in), `plan::keep` (`Keep::rank`: Surplus, Needed, Unlisted; `Keep::need_in`), the drop rule and the forest base's drop ask it. Want lines from failed crafts and smelts, the needs planner's main goal choice and the base, eyes, end_portal and stronghold make calls, once per episode, tagged by source ([`keep-rule`](../../brain/ruststeve/design/keep-rule.md)).
- **Chests**: memory's chest record, the world's chest window fold, the sim chest (the jar's quick-move order, confirmed on a real server by parity run 46120883534: k00 rows 3 of 3 in every field), `plan::stash` Stash and Fetch ([`chests`](../../brain/ruststeve/design/chests.md)).
- **Stash planner**: the home chest (goal rows `home-chest`, `place-chest`, only acts where the bot stands, wood to spare past `READY_LOGS`), the stash before a leg's portal (`config/stash.toml` legs; stash min(need after the leg, held less the leg's need)), the fetch after a revive, a note on every outcome.

## Walls
None yet.
