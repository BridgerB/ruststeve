# End checkpoints: `crystals_5`, `crystals_1` (issue 20 part 2, E9)

## Needs

A bot near the goal is saved as a snapshot: at 5 or fewer crystals left (`crystals_5`) and at 1 or fewer (`crystals_1`), between `island` and `crystals` so `Ord` stays the ladder (the snapshot taker keeps the highest by `Ord`; the plan reads `stage_reached >= crystals` as every crystal gone).

## Wall 1: the resume transient (fixed)

- **Seen:** races 46044026128 (1b24de5, 6 lanes) and 46045482138 (11466de, 6 lanes): every lane snapshotted `crystals_5` on tick 40. Lane 0 of 46044026128: at tick 20 "crystal#29 seen", "crystal#30 seen"; at tick 40 "crystal#29 gone (gone)" and four more; "stage crystals_5"; at tick 100 all seen again.
- **Cause:** the first count was the crystals tracked (alive within 60 of the fountain, every spike's column loaded). On a resume the server untracks and re-sends the crystals, and for a tick the count read 5 or fewer. The same flaw could fire `crystals` (0 for a tick).
- **Fix:** a crystal is gone only when destroyed: its removal with its blast at its spot (the server removes, then explodes, `EndCrystal.hurtServer`). The destroyed spikes are kept in memory (`crystals_destroyed`), so a snapshot's resume keeps them. An untracking is never a destruction.
- **The End row** in the race summary had the same flaw: `left` was the last senses row's tracked count. It is now 10 less `struck` and `blast`. Race 45994686575 recounted from the ledgers: lane 5 had 1 left (true as reported), lane 4 had 2 (not 1), lane 3 had 6 (not 1), lane 0 had 3 (not 2).

- **A gone the bot did not see go:** a spike seen with no crystal on it for `CRYSTAL_ABSENT_TICKS` (400, 20 s) running counts too, if the bot is within 60 of the fountain and the spike's column is loaded. That covers a development start's RCON kill (`dev-end-no-crystals`) and the sim's bare island (`bot/tests/end_loop.rs` `on_the_island_endermen_about_are_never_stared_at`, whose bunker waits on `crystals`; it failed on the blast-only first fix, CI 46047840723). A resume's re-send is done within about 80 ticks.

## Wall 2: the snapshot on a stand (fixed)

- **Seen:** race 46059621531 (4b23f55), 10 lanes resumed from `crystals_5` (46055583508 lane 2, taken at its 5th strike on the stand at (-33.4, 79, 27.6) by spike [-34, 24]). 7 of 10 were dead within 3,460 ticks with 0 struck: 6 to breath, 1 to an enderman. Lane 3's ledger: 80 "dig the way out ... shut in a pocket"; 160 "spikes: wait (stranded on a spike's top with no pillar down at (-33, 79, 25))"; 220 a breath cloud on the feet; dead at 300. "Stranded" fired 5 to 15 times per lane.
- **Cause:** the stage first holds at a strike, high on a stand. The plan's pillar record is not in a snapshot, so the resumed bot has no way down.
- **Fix:** `crystals_5` and `crystals_1` hold only when `grounded`: on ground, on end stone, with at least 3 of the 4 cells beside the block under the feet full. That is the island, not a 1x1 pillar, a stand, or a spike's obsidian top. `crystals` is unchanged.

## Risks
- A crystal destroyed out of the bot's hearing (its blast packet goes to players within 64) is not counted, and the count stays high. That is safe (no false checkpoint) but can hold a stage back.
