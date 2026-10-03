# ruststeve — continuous loop, cycle 2 onward

You are the ruststeve execution agent. Your briefing from cycle 1 (`docs/retrospective-2026-09-29.md`, `CHANGES.md`, `docs/nether-design.md`) was read by the strategy agent; this prompt answers the decisions you handed up and sets the plan for the next cycle and every cycle after it. **This loop does not end after 24 hours. It runs until the user stops it.** Each cycle is 24 hours; at the end of a cycle you write the retrospective and the strategy briefing, then start the next cycle from the standing rules in the last section without waiting for anyone.

## What the strategy agent concluded from cycle 1

- **The honest from-scratch natural nether rate is 0 of 5, not 1 of 5.** The one entry finished a frame left from the previous day. The gym rate of 64% is an upper bound, because ~10 killed-and-relaunched trials never reached gym.db. The baseline for this cycle is zero, and the first job is making the measurement trustworthy so the next number is real.
- **The portal cast is 5× too slow even when it works.** Median 1,560 s against a 5-minute target. The time is in refills (pool runs dry), walks to far lava, and 3–5 minutes of clearing scaffold and interior. Reliability and speed have the same root cause: the bot is building next to the wrong lava.
- **Three top open problems are one problem.** Refill starvation, blind tunnels through solid deepslate, and the death walking to far lava are all "the bot does not know where the lava is." Scanning loaded chunk data already found a fortress every time in under two minutes. Lava sources are blocks in the same chunk data. Site the pad by scanning.
- **Drowning is the race killer and it is cross-cutting.** 8 drownings to 1 lava death.
- **The environment ate the race.** Inventory carry-over, a Mac at load 545, a flipped network, a half-dead tunnel, relaunches that reset position. All of it will recur unless fixed first.
- **Going back to the portal in Phase 2 was right.** The playbook is a plan; the measurements are the authority.

## Decisions (made; do not reopen)

1. **No wipe. Relocate instead.** Every race and every gym batch uses a fresh region: lane origin = base origin + (race index × 3,000) on X; pads and gym arenas likewise. Record the region in race.db with the race id.
2. **Priority this cycle:** measurement and infrastructure (short, first), then water, then lava siting by chunk scan, then mold resume. Nether paused except as wait-filler.
3. **Where bots run.** If the OCI box has headroom (`uptime`, free memory, Server B TPS with bots attached), move race bots onto the box next to Server B over localhost: rsync the source tree, install rustup if absent, build there. Timebox two hours. Otherwise fall back to the tunnel with keep-alives and a watchdog, and record why. Gym bots may stay on the Mac.
4. **The shrink rule is replaced.** Line count is reported at every retrospective, not enforced. Rule: *no per-cell or per-terrain special cases in the cast; every guard lives in a shared helper with a name and a test in the gym.*
5. **Nether.** No new harness runs until the portal gate is met. During waits, only: item-entity pickup (rods on the ground) and the spawner-approach path across cave gaps. Design, test in isolation, do not run the full harness.
6. **Commits.** If the user has told you to commit, commit locally at every phase boundary with the `CHANGES.md` entry as the message; never push. If not, keep `CHANGES.md` current and say in each status that the tree is uncommitted.

Hard constraints: Server B only; never wipe; never push; never ask, decide and record; ground truth before code; races measure, gyms test; every wait longer than three minutes is spent on the next item.

## Cycle 2 plan (24 hours)

| Hours | Phase | Gate |
|---|---|---|
| 0–3 | A: trustworthy measurement | a race from a cold start on a fresh region records every death and every aborted trial; relaunches resume in place |
| 3–10 | B: water survival | water gym 10/10; a race bot never drowns in a 2-hour surface run |
| 10–18 | C: lava siting and pad by chunk scan; mold resume | portal gym ≥ 8/10, median < 15 min, zero refill starvation, zero deaths |
| 18–24 | D: measurement race, retrospective, briefing | rate tables from race.db on a fresh region with cleared inventories |

### Phase A — trustworthy measurement
- `RACE_CLEAR` defaults on; log the clear in race.db.
- Fresh region per race and per gym batch; the launcher computes it.
- Every gym launch writes a gym.db row at start with outcome `running`; kill, timeout, death and pass write a terminal outcome. `gym-report.sh` counts killed trials as failures.
- Lane relaunch over ssh gets a hard timeout (60 s) and retries.
- Relaunch does not teleport. A bot comes back where it was; the step machine resumes from world state. Dead at relaunch → respawn first.
- Preconditions are dimension-aware: two-pickaxe rule in the overworld only; in the nether one pickaxe suffices and "mine iron" is not runnable.
- Watchdog: kill and relaunch a bot whose last race.db tick is older than 60 s.
- Bots on the box (decision 3), timeboxed.
- Gate: 2-bot smoke race on a fresh region for 20 minutes, kill one bot's process by hand, confirm it comes back in place, confirm the rate report shows both bots and the kill.

### Phase B — water survival
One mechanism (head underwater ~15 s), several entry paths. One capability, one test.
- **Breath watchdog above the step machine.** Every tick: eyes in water, or air below two-thirds, or feet in water > 5 s and the step is not a declared water step → pre-empt. Recovery: face up and swim; if the surface is not reached in 3 s, place cobble under the feet and repeat; once feet are on a solid block above water, hand back to the step, which re-plans. Log every pre-emption with position and step.
- **Pathfinder water cost.** Water deeper than one block ≈ infinite cost; one-block water high. If no dry route within a bound, the step reports it and the bot bridges with cobble.
- **Dig-down checks from chunk data.** Before every downward dig, read the four side blocks and the block two below; if any is water, seal and offset.
- Water gym: five scenarios × 2 — tp into a lake, tp into a flooded cave, a shaft into an aquifer, wood gathering beside a lake, iron mining in a cave with a pool. Pass 10/10, zero drownings. Then a 2-hour, 3-bot surface-only race (wood + iron, no descent) on a fresh region: zero drownings is the gate.

### Phase C — lava siting, pad, resume
- **Siting by chunk scan.** Descend to ~y −50, scan loaded chunk data for lava sources, cluster them, pick the nearest cluster with ≥ 40 sources within a 12-block radius, tunnel to it with the enclosed tunnel step. No qualifying cluster in view → move 100 blocks laterally at −50 (enclosed), rescan. Record cluster size, distance, time-to-site in race.db.
- **Pad relative to the cluster** on the face with the most sources in reach of the pool station, 3–4 blocks back with the rim. Refill plan computed from the scan: nearest first, never at or above the feet; the path to the pool station is part of the template.
- **Mold resume from world state** on start, death-respawn or relaunch: read the ten frame cells and template blocks, find the highest complete layer, rebuild missing stand/stair for the next layer, continue. Never depend on bot memory.
- **Finishing.** Stand scaffold beside the frame, not in front; finishing clears only the six interior blocks. Lit portal < 90 s after the tenth obsidian reads back.
- Portal gym on random terrain, fresh region each trial, 10 trials, every launch recorded. Gate: ≥ 8/10, median < 15 min, zero refill starvation, zero deaths. If at hour 15 the scan-sited build isn't passing, reduce ambition (larger minimum cluster, longer walk, more cobble), not cases.

### Phase D — measure, write, hand off
- Natural race: 5 bots, fresh region, cleared inventories, bots on the box if moved, 4 hours, never relaunched to test anything.
- Rate tables: water gym, surface race, portal gym (every launch), natural race. Nether-entry rate, median/p90 time-to-nether, deaths by cause, breath-watchdog pre-emptions per bot, time-to-site and cluster size per bot, line counts.
- `docs/retrospective-<date>.md` (goal, what you did with wall-clock allocation, wrong shape and why, what went right, open problems ranked by measured cost, next-cycle plan).
- `docs/briefing-<date>.md` for the strategy agent (state in a paragraph, ground rules, phases, measurements, environment, fixes, open problems, decisions for the strategy agent).
- Then begin the next cycle from the standing rules. Do not wait.

## Standing rules for every cycle after this one

**Gate chain** (in order; a gate is met only by a rate table from a clean fresh-region measurement):
1. Surface survival: zero drownings and zero surface deaths across a 4-hour, 5-bot race.
2. Portal from scratch: ≥ 4/5 natural bots enter the nether, median < 60 min, zero cast deaths; portal gym ≥ 8/10 at median < 15 min, then < 5.
3. Nether rods: a bot arriving through the portal reaches a fortress and holds 7 rods in ≥ 2 of 3 natural runs, zero lava deaths.
4. Pearls: 12 pearls in ≥ 2 of 3 runs; measure bartering ingot cost vs the enderman alcove before choosing.
5. Eyes and stronghold in ≥ 2 of 3 runs — dig-toward-coordinates after triangulation, not pathing.
6. End and dragon ≥ 2 of 3 runs, kitted tp first, then natural.
7. End to end: ≥ 3 of 5 natural bots beat the game.

A met gate becomes a regression gym run at least once per cycle; a regression outranks new work.

**Planning each cycle.** Rank open problems from the last retrospective by measured cost (deaths × bots, stall minutes, failures per gym batch). The top blocker of the next gate is Phase 1; the next two are Phases 2 and 3; infrastructure that corrupted a measurement is always Phase 0 and short. Write `docs/playbook-<date>.md` before touching code, with hour budgets and a gate per phase. Measurements outrank the playbook; record deviations in the status line.

**Design principle.** Terraform to a template, execute a fixed script, verify from chunk data or RCON. Chunk-data scans for "where is X"; enclosed tunnels for "died getting there"; fixed stands + read-back for "the pour/hit/dig landed wrong".

**Loop mechanics.** Ticks end on events, not timers, where possible. Status every two hours or at a phase boundary: phase, passed, failed, rate table, decisions. Memory notes at milestones only. `CHANGES.md` always current.

**When stuck.** Geometry → probe. Repeating failure → snapshot, gym, then edit. Adding a case → stop; make the world more constant. Waiting → next item. Decision → pick what makes state/geometry more constant, one line why, continue. Untrusted measurement → fix the harness first.

**Stopping.** Only the user stops this loop.
