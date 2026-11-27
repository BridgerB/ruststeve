# End checkpoints: `crystals_5`, `crystals_1` (issue 20 part 2, E9)

## Needs

A bot near the goal is saved as a snapshot: at 5 or fewer crystals left (`crystals_5`) and at 1 or fewer (`crystals_1`), between `island` and `crystals` so `Ord` stays the ladder (the snapshot taker keeps the highest by `Ord`; the plan reads `stage_reached >= crystals` as every crystal gone).

## Wall 1: the resume transient (fixed)

- **Seen:** races 46044026128 (1b24de5, 6 lanes) and 46045482138 (11466de, 6 lanes): every lane snapshotted `crystals_5` on tick 40. Lane 0 of 46044026128: at tick 20 "crystal#29 seen", "crystal#30 seen"; at tick 40 "crystal#29 gone (gone)" and four more; "stage crystals_5"; at tick 100 all seen again.
- **Cause:** the first count was the crystals tracked (alive within 60 of the fountain, every spike's column loaded). On a resume the server untracks and re-sends the crystals, and for a tick the count read 5 or fewer. The same flaw could fire `crystals` (0 for a tick).
- **Fix:** a crystal is gone only when destroyed: its removal with its blast at its spot (the server removes, then explodes, `EndCrystal.hurtServer`). The destroyed spikes are kept in memory (`crystals_destroyed`), so a snapshot's resume keeps them. An untracking is never a destruction.
- **The End row** in the race summary had the same flaw: `left` was the last senses row's tracked count. It is now 10 less `struck` and `blast`. Race 45994686575 recounted from the ledgers: lane 5 had 1 left (true as reported), lane 4 had 2 (not 1), lane 3 had 6 (not 1), lane 0 had 3 (not 2).

## Risks

- A development start that kills the crystals by RCON (`dev-end-no-crystals`) has no blasts, so it never reaches `crystals`. The plan's `end::crystals_done` (the bunker after the crystals) then stays false there. Development only; never evidence.
- A crystal destroyed out of the bot's hearing (its blast packet goes to players within 64) is not counted, and the count stays high. That is safe (no false checkpoint) but can hold a stage back.
