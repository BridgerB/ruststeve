# Inventory and chests (issue 77)

**Status: machinery built and proven on a real server (parity 46132490442); the seeds runs are open.** #77 closes only after #69's seeds (`config/seeds.toml`) land. Wall numbers cited elsewhere as "inventory wall N" are this file's.

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

## Real-server evidence
- Parity 46132490442 (issue-77-chests fd33b26a, `only=k0 tolerance=3`): 3 of 3 moments match in every field; the notes check passes. Row k01-task (`plan:chest-trip`) fired `stash done:` 1, `fetch done:` 1, `chest closed at` 2, `chest trip met` 1, real and sim alike.
- Parity 46128707215 (831dbe72): the real k01-task ledger, read from the artifact's recording: the chest placed at plan tick 29, "stash done, 74 of 74 cobblestone, 3 of 3 string" at 37, "fetch done … it shows nothing" at 43, "chest trip met (1 stashes and 1 fetches ended)" at 44.

## Walls

### 1. A green row that could not tell a round trip from nothing (parity 46128707215)
- **What happened**: k01-task passed with 138 cobblestone and 3 string held and the chest placed: also what a bot that only placed the chest would hold. It ended after 60 server ticks of the row's 600.
- **Read**: the real ledger showed the place, stash and fetch at plan ticks 29 to 44; the 60-tick end was the aim met (`p.finished()`), not a cap.
- **Fix**: a moment row may name the ledger notes that must fire, with their counts (`notes`, `results::parity_notes`); k01-task asks for its stash, fetch and trip notes.

### 2. One stash counted twice (parity 46130761031)
- **What happened**: the notes check failed: "stash done" and "fetch done" fired 2 each, real and sim alike.
- **Read**: the recorded notes held one "chest opened" per task; the substring matched both the window's close note and the task's end note.
- **Fix**: each asked note matches one line (`stash done:`, `chest closed at` twice); a miss lists the notes it matched; a unit test with two notes sharing a substring.

### 3. No home chest when the deaths come (race 46140744043; for #81 death recovery)
- **What happened**: the seeds race (cold, pale-garden, far-stronghold from their gear frontiers, far-fortress from its nether frontier; revive on) had 75 deaths in 38 of 56 runs, and every one wrote `fetch after the respawn skipped (no home chest)`. `results keep` (probe 46146387380): stash 0, fetch 0, home chests 0. No gear bot reached `ready` (0 of 42), so none came to a portal; far-fortress began past it.
- **Cause**: by design the home chest is made at the Nether portal, before the first dangerous leg. The deaths on these worlds come earlier, in the gear stage (water, walls, skeletons, a warden), where there is nothing stashed to fetch.
- **Not fixed here**: a home chest before the gear stage is beyond issue 77's design (the director, 2026-11-28). Tagged for #81 (death recovery). The stash's live evidence is taken from typecraft's `ready` snapshot instead, where bots reach the portal.

### 4. A respawn's note says the items were kept; they were not (race 46140744043; for #81)
- **What happened**: pale-garden rs-l01-b12's ledger:
  ```text
     46640 note drop Cobblestone from slot 23 (fewer than 6 main slots free; needed: 64 by win; stage gear; always)
     46680 note died (death.attack.sonic_boom warden) and respawned (death 2; items kept, the aim carries on)
     46700 item cobblestone 64 -> 0
     46760 note want Blackstone|CobbledDeepslate|Cobblestone 3 for goal gear (the needs planner makes tag:stone_tool_materials rather than holds it)
  ```
- **Read**: race.yml never sets `keep_inventory` (only marathon.yml runs `gamerule keep_inventory true`), so a race's server drops the pack on a death; the revive note's "items kept" holds only on the marathon's server.
- **Effect on this issue**: all 6 of the probe's dropped-then-wanted were wants after such a death (cold b03, pale-garden b02 twice, b09, b12 twice): each drop had left the keep count, and the death took the rest. The counter is made death-aware.

### 5. The home chest is never made: no wood to spare at the portal (race 46147028872)
- **What happened**: in the typecraft race from the lineage `ready` snapshot (race-45128866973 lane 6; 2 lanes, revive on), the stash gate fired at portal entry and skipped every time: `stash before the nether skipped: no home chest (wood to spare -11 planks of 8 (the leg's 4 logs kept); 16 dirt, 56 cobblestone, 11 gravel, 3 feather kept)` (lane 0, tick 23,480), and `… -15 planks of 12 …` (tick 17,460).
- **Cause**: `home_chest_task` (`crates/plan/src/stash.rs`) crafts the chest only from wood to spare above `READY_LOGS`, and only at the portal; the bots reach the portal holding fewer than the leg's 4 logs, so the spare is negative and no chest is begun. The gate, the stash list and the skip note all work.
- **Fix** (the director, 2026-11-28; within the design, the home point stays the portal): the chest is crafted the first time wood to spare holds in the overworld before the leg, carried, and placed at the portal; a keep.toml `chest` row ranks the carried chest Needed until it is placed; the early craft writes `home chest crafted early, N planks spare`. Judged on a rerun from the lineage `gear` snapshot (a bot at `ready` has no wood to spare left), with the craft's cost counted from the ledger.
