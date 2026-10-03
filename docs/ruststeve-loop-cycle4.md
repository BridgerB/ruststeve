# ruststeve — continuous loop, cycle 4 onward

You are the ruststeve execution agent. The strategy agent read your cycle-3 recap, `CHANGES.md` (Phases 1–6 of cycle 3), `docs/refill-design.md`, the race i6 rate report and the live code. This prompt answers the six decisions you handed up, adds two the recap did not ask, and sets the plan for cycle 4 and every cycle after it. **The loop does not end after 24 hours. It runs until the user stops it.** Each cycle ends with a retrospective and a briefing, then the next cycle starts from the standing rules without waiting for anyone.

The standing rules from `docs/ruststeve-loop-cycle2.md` (gate chain, planning, design principle, loop mechanics, when stuck, stopping) still hold except where this document amends them. Re-read that section once at the start of this cycle.

## What the strategy agent concluded from cycle 3

1. **You crossed the line that mattered.** Race i6 put a bot into the nether from an empty inventory on a fresh region in 177 minutes. Cycles 2–3 started from a true rate of zero. The capability is now real and rare, which is the right place to start measuring it.
2. **Deaths are no longer the portal's problem; movement is.** Since the sealed station, zero bots have died inside a refill. Every death since has been on an old path that still moves freely beside the pool: prepare's approach goto, the early scoop, the tunnel detour, the return to the anchor, the layer-3 return. You patched them one at a time, and each batch found the next one. That is the cycle-1 disease in a new place. One guarded primitive ends it; another round of per-path patches does not.
3. **The race, not the gym, says what blocks gate 2.** Race i6 lost 2 of 5 bots to ocean placement before they moved, 1 of 3 land bots to iron (135 minutes, 74 × "mined 0/22 iron"), and 1 of 3 land bots to a bedrock-band pool (196 minutes). The portal gym's clean rate was not what decided the race. Infrastructure and iron are the cheap wins; the portal is the expensive one.
4. **Speed is real but secondary.** Clean portal passes now run 12–27 minutes. The race gate (median under 60 minutes from spawn) needs the overworld kit plus the portal under an hour; the kit alone takes 40–50 minutes. So the 15-minute portal matters, but not before the bots reliably reach the portal stage at all.
5. **Your stops were right individually and wrong collectively.** Seven portal batches, none completed on one build. Every stop had a reason, and the gate still has no two-batch measurement. The rule below fixes that without taking away the safety stops.
6. **The nether has a known next wall, with data.** i6's bot saw a fortress and failed the enclosed approach every 10 minutes for an hour. That is design input, and the nether stays paused until gate 2.

## Decisions (made; do not reopen)

1. **Disk and world.** The box is clean (14 G free; journald capped, weekly nix GC). The user has authorised resetting Server B's world. **Reset `mc-b` at the start of every cycle** (stop `minecraft-b`, remove its `world`, start it; Server A and the steve project are never touched). Region indexes restart at 0 after a reset. Check `df -h /` at every cycle start and before every batch; below 5 G free, stop launching and clean before anything else.
2. **Movement near lava: one guarded primitive, not more patches.** Add `lava_safe_move(bot, target, reach)` to a shared helper. It uses only the lava-aware pathfinder (lava-adjacent cells already refused) for anything beyond one block, then the sneaking non-jumping `settle_xz` for the last block, never a raw jumping walk. Before it returns true it verifies the post-condition from chunk data: solid non-lava footing, no lava in the 3×3×2 body ring, no missing floor beside the feet. If the check fails it steps back one cell along the path it came and returns false; the caller re-plans, never improvises. Every movement that can end within 4 blocks of lava goes through it: prepare's approach, the early scoop walk, the tunnel detour, the return to the anchor, every mold stance and platform move, the station walk. Then delete the per-call-site walk caps, the three `REFILL_LEGACY` paths and `fill_bucket`'s stand search. The success metric is the same as cycle 1's: zero deaths within 4 blocks of lava across two full batches, and `portal.rs` shorter than 2,787 lines.
3. **The two-batch rule, formalised.** A build is measured by two full 10-trial batches before the next change. A batch may stop early only for (a) a death on a path **this build changed**, or (b) a build that cannot produce a measurement (no bot reaches the step under test). A death on an old, unchanged path is logged with its mechanism and the batch runs to completion; the fix ships in the next build. No build goes into a natural race without at least one complete batch behind it. Each stop writes one line in `CHANGES.md`: which clause, which bot, which line in the log.
4. **Speed: more buckets, bigger lanes, straighter search.** The race kit crafts **5 buckets** (`craft_bucket` 3 → 5). The iron target of 22 already covers it: two iron pickaxes (6), five buckets (15), flint and steel (1). The gym's portal kit mirrors the race kit exactly (2 iron pickaxes, 5 buckets, 1 water bucket, flint and steel, the cobble a descent yields) so the gym measures the race's bot. Station ranking keeps distance first and breaks ties on target count. Cluster minimum stays 100. The 15-minute median stays the gym gate for this cycle; it is not the reason to delay the race.
5. **Gym first, then the race, on a clock.** The race runs at hour 16 whatever the gym says, on the best completed-batch build. Races measure; they are never relaunched to test.
6. **Commits.** Not authorised; the tree stays uncommitted and every status says so. The strategy agent has asked the user to authorise local commits at phase boundaries. If that arrives, commit at each phase boundary with the `CHANGES.md` entry as the message; never push.
7. **Nether.** Paused until gate 2. Allowed this cycle, during waits only: read i6's `find_fortress` logs and write `docs/nether-approach-design.md`. No harness runs.
8. **No X-ray creep.** The project's rule is no X-ray: the bot acts only on blocks it has seen exposed. The iron fix below must stay exposure-only (`find_exposed_blocks`, tunnel-wall sightings, `WorldMemory`). The lava cluster scan has no exposure test; the strategy agent has flagged that to the user as a rules question. Do not add any new scan of unexposed blocks this cycle.

Hard constraints are unchanged: Server B only; never push; never ask, decide and record; ground truth before code; races measure, gyms test; every wait longer than three minutes goes to the next item.

## Cycle 4 plan (24 hours from the moment you start)

| Hours | Phase | Gate |
|---|---|---|
| 0–3 | 0: close cycle 3, reset, validate what is written | smoke race: 2 bots on land, a killed bot resumes in place, alarm at ≤ 6.5 s |
| 3–9 | 1: `lava_safe_move`, legacy removal, two full portal batches | ≥ 6/10 clean per batch, 0 deaths within 4 blocks of lava, median reported |
| 9–13 | 2: iron finding | `mine_iron` gym 5/5 to 22 iron under 20 min on random land, no X-ray |
| 13–16 | 3: speed and the hole climb-out (timeboxed) | at-depth search median under 2 min; water_cave 2/2 in gate time |
| 16–20 | 4: natural race i7 | rate tables from race.db |
| 20–24 | 5: retrospective, briefing, cycle 5 playbook | docs written; next cycle started |

### Phase 0 — close cycle 3, reset, validate (hours 0–3)

- **Close cycle 3 in 45 minutes, no more.** Write `docs/retrospective-2026-10-03.md` and `docs/briefing-2026-10-03.md` from `CHANGES.md`; the strategy agent already has your recap, so these are the record, not a new analysis. Include the rate tables: race i6 per bot, every portal batch with its stop clause, the five water regressions.
- **Reset `mc-b`** (decision 1). Record the reset and the free space in `CHANGES.md`. Region index back to 0.
- **Build 6c** on the box (`target/` for races, `target-gym/` for gyms, as now). It has not run; treat it as untested.
- **Smoke race, 2 bots, 20 minutes, `RACE_MAX_PRIORITY` at the stone pickaxe.** It tests three untested things at once: the beyond-strip land placement (both bots must start on land; if a lane is still water, that is a Phase 0 bug, fix it now), relaunch in place (kill one bot's process by hand at minute 5 and confirm it resumes where it stood), and the rate report (both bots, the kill, the placement rows).
- **Breath-alarm latency.** Race i6's alarm fired at 11.3 s, not 6. Something blocks `drive_tick` for about 5 s; a long underwater dig is the likely suspect. Reproduce in `water_descent` (log the alarm time), find the blocking call, make it yield ticks. The watchdog must fire within 6.5 s or the escape starts with half the air gone.
- Gate: all three smoke checks pass and the alarm fires on time.

### Phase 1 — one primitive for every move near lava (hours 3–9)

This is the portal work for the cycle, and it is a deletion as much as an addition.

- Build `lava_safe_move` as in decision 2, in a shared helper with a name and a doc comment that states the post-condition.
- Replace the call sites, in this order, one build: prepare's approach goto, the early-scoop walk, `tunnel_step`'s detour, the return to the anchor, `cast_cell`'s stance moves and `ensure_platform`'s moves, the station walk. Then delete: the `walk_to_xz` 3.5-block cap at the refill sites, `fill_bucket`'s stand search and its approach, the `REFILL_LEGACY` branches, the per-site "stance FAIL" retries that re-plan beside the pool. Keep `walk_to_xz` only where lava cannot be within 4 blocks (surface wood, centring in a dry tunnel).
- **Gym first, small:** a `lava_safe_move` scenario on the `LavaPool` arena, 10 moves to stands on the rim from the tunnel mouth, pass = 10/10 arrivals with the post-condition true and 0 deaths. Twenty minutes, then on.
- **Portal batch A and batch B, 10 trials each, on this build**, under the formalised two-batch rule. Expected: the near-lava deaths go to zero; what remains are timeouts, which are Phase 3's problem.
- Gate: ≥ 6/10 clean in each batch, zero deaths within 4 blocks of lava, clean median reported. If batch A shows a death on a path this build changed, fix it and restart A (clause a). Anything else runs to the end.

### Phase 2 — iron finding (hours 9–13)

Race i6's bot 002 spent 135 minutes at y 31 logging "mined 0/22 iron" 74 times. `mine_ore` ranges 48–192 blocks when the local ore is gone, but something in that loop finds nothing for two hours on ordinary terrain. This cost one of three land bots the whole race; it is cheaper than any portal work and blocks the gate just as hard.

- **Reproduce first.** `GymSetup::FixedSurface` at bot 002's lane (`race.db` has its last positions), kit = what it held (stone pickaxe, cobble, the sticks and planks a race bot has at that step), step `mine_iron`, 30 minutes. Read why it finds nothing: wrong y band, a ranging loop that returns to the same column, a stuck `strip_tunnel`, ore seen but unreachable, or ore counted wrongly (`iron_ore + iron_ingots`, raw iron excluded?). Probe with RCON before editing: is there iron ore within 32 blocks of where it stood?
- **Fix the mechanism found, exposure-only.** The expected shape: strip-mine at the iron band (y 8–24 is the dense band; y −16..−8 for deepslate iron) in straight 1×2 tunnels that expose walls, record every exposed ore into `WorldMemory`, and range in a straight line, not a loop. No chunk-data ore scan.
- **Gym gate:** a `mine_iron` slug on random land, empty-ish kit, 5/5 reaching 22 iron (ore plus ingots) under 20 minutes, 0 deaths.

### Phase 3 — speed and the climb-out (hours 13–16, timeboxed)

Only after Phases 1–2 are gated; stop at hour 16 whatever is in flight.

- **At-depth search** took 283 s in one slow pass. Site the nearest cluster whose straight tunnel from the −50 floor crosses no lava-floored cells (you already rank stations this way; rank clusters the same way), and log time-to-site per bot. Target: median under 2 minutes.
- **Re-site cost** (5 minutes after a pool runs dry): with 5 buckets the early scoop plus two refills should cover a frame from one cluster of ≥ 100; measure how often a re-site still happens.
- **water_cave hole climb-out:** the bot breathes but cannot leave the 1×1 hole it dug through a cap, because bobbing resets `no_rise`. Make the stuck test position-based (no horizontal progress for 3 s) rather than rise-based, so the dig-a-step fallback fires. Water regression after, all slugs × 2, zero deaths.
- **Spawnpoint obstruction** (a bot stranded at world spawn in batch 9): if the spawnpoint block is not air at setup, move it one block over.

### Phase 4 — natural race i7 (hours 16–20)

5 bots, Server B, fresh region, cleared inventories, 4 hours, `RACE_GOAL=nether`, the best build with a completed batch behind it. Never relaunched to test anything. Watch the server log for deaths and arrivals; everything else waits.

### Phase 5 — measure, write, hand off (hours 20–24)

- Rate tables from race.db and gym.db: nether-entry rate, median and p90 time-to-nether, time-to-portal-stage per bot, deaths by cause and place, breath-watchdog alarms and pre-emptions, time-to-site and cluster size, portal gym per batch with stop clauses, line counts.
- `docs/retrospective-<date>.md` and `docs/briefing-<date>.md` in the established shape.
- `docs/playbook-<date>.md` for cycle 5 from the retrospective's ranked open problems, then start cycle 5. Do not wait.

## Amendments to the standing rules

- **World reset** is allowed for `mc-b` at cycle start (decision 1). It is never allowed mid-cycle, never for Server A.
- **The batch rule** is decision 3, verbatim, for every gym from now on.
- **Movement rule.** No new raw walk may be added anywhere lava can be within 4 blocks. New movement near lava, water or a drop goes through a named guarded primitive with a post-condition check and a gym scenario.
- **Disk rule.** `df -h /` at cycle start and before every batch. Under 5 G free: stop, clean, record.
- **Kit rule.** A gym's kit for a race step is the race's kit for that step. If the race changes a kit, the gym changes with it the same hour.
- **Exposure rule.** No scan of unexposed blocks is added. Existing ones are reported in the retrospective's line-count table under their own heading until the user decides.

## Decisions handed to the user (do not block on them)

1. Commit authorisation for local commits at phase boundaries (three cycles are uncommitted).
2. Whether the lava cluster scan without an exposure test is acceptable under the no-X-ray rule, or must become exposure-only.
3. Whether `mc-b` resets may also happen mid-cycle when a batch needs a clean world (default: no).

## Status line

Every two hours or at a phase boundary: phase, passed, failed, the current rate table, decisions made, stop clauses used, free disk, and "tree uncommitted". Keep `CHANGES.md` current at all times; it is the audit trail and the commit message.
