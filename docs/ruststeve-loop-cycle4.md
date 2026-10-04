# ruststeve — cycle 4: the learning-system cycle

You are the ruststeve execution agent. This document is complete on its own. You do not need any other planning document to run this cycle; `CHANGES.md` stays your audit trail, and the server log, `race.db`, `gym.db` and RCON stay your ground truth. Read this whole document once before touching anything, because the shape of the work changes this cycle and the old plan's assumptions are revoked where this document says so.

**The loop does not end after 24 hours. It runs until the user stops it.** At the end of this cycle you write a retrospective and a briefing for the strategy agent, then start cycle 5 from the standing rules at the end of this document, without waiting for anyone.

---

## Part 1 — Why the method changes

For four cycles the project has run the same loop: pick the most recent failure, patch it, run a 10-trial gym batch, read the logs, patch the next failure. It produced real things. Measurement is trustworthy now. Water deaths went from eight a race to zero across fifty gym trials. A sealed refill station ended deaths inside refills. One bot reached the nether from an empty inventory in race i6, in 177 minutes. All of that was earned.

It also produced a pace: roughly one sub-capability per day, and the portal, which is step 11 of 28, has taken four days and still passes clean about a third of the time. The dragon is step 28. At this pace a win is months away, and most of those months would be spent on failures nobody has seen yet, because nothing after the portal has ever been exercised.

The strategy agent read the last four cycles the way a machine-learning engineer reads a training run, and five problems stand out. None is fixed by a neural network. All are fixed by treating the project as a learning system.

1. **The late game is starving.** The whole run is serial from spawn. Fortress, blazes, pearls, stronghold, End and dragon have had zero practice. In a learning system that is the classic failure: early stages soak up all the experience and late stages are never learned. The cure is a reverse curriculum: practise the end first, from a kitted teleport, and move the start earlier as it works.
2. **The measurements cannot resolve the decisions being made on them.** A batch that scores 6 of 10 has a 95% confidence interval of about 31% to 83%. Two batches of 10 cannot tell a 60% build from an 80% build. Seven stopped-early batches carry even less. Decisions were made on noise, and the fix is to print the interval next to every rate and to stop batches by a statistical rule rather than by a count.
3. **Whack-a-mole is the method.** "Patched one path at a time, and each batch found the next one" is the exact anti-pattern. Structure, not magnitude: one guarded movement primitive near lava, one near water, one near drops. Then nothing new is patched at a call site again.
4. **Stalls are unbounded.** Race i6 bot 002 logged "mined 0/22 iron" 74 times over 135 minutes. Another bot failed the same fortress approach every 10 minutes for an hour. No step has a budget, so one bad step eats a race. A budget plus a stall detector would have saved about three bot-hours in that one race.
5. **Constants are set by argument.** Cluster minimum 100, five buckets, iron band y 8–24, a 3.5-block walk cap: each was decided by a strategy agent reading logs. Each is a parameter that data sets better and cheaper, and a race with five bots is five parallel experiments if you let it be.

This cycle builds the learning system: a shared event log, budgets and stall detection, honest intervals, bandits over the argued constants, and the first late-game gyms. The portal keeps improving in parallel, with one structural fix instead of ten patches. By the end of the cycle the strategy agent wants, in order of importance: the funnel table with intervals; the bandit posteriors; the hours of stall time recovered; and the first failure log from a dragon gym. Those four are the deliverables. Everything else is in service of them.

---

## Part 2 — Hard constraints and decisions

These are fixed. Do not reopen them; record them when they bind.

**Constraints that never change**
- Server B only (port 25566, RCON 25576, service `minecraft-b`, data in `/var/lib/mc-b`). Server A and the steve project on the same box are never touched, never read for state, never restarted.
- Never push. Never open a pull request.
- Commits are **not authorised**. The tree holds three cycles of uncommitted work; the strategy agent has asked the user to authorise local commits at phase boundaries. Until the user says so in this session, do not commit, and say "tree uncommitted" in every status line.
- Never ask; decide, write one line saying why, continue.
- Ground truth before code: an RCON probe, a server-log line, a race.db row or a gym.db row before every edit. A hypothesis is not a cause.
- Races measure, gyms test. A natural race is never relaunched to test anything.
- Every wait longer than three minutes goes to the next item in this plan.
- **No X-ray.** The bot acts only on blocks it has seen exposed. No new scan of unexposed chunk data may be added. (The existing lava cluster scan has no exposure test; the strategy agent has flagged it to the user as a rules question. Leave it as it is and report it under its own heading in the retrospective.) This rule extends to learning: no model feature may use information the bot did not legitimately sense.

**Decisions for this cycle**
1. **World.** The user has authorised resetting Server B's world. Reset `mc-b` once at the start of this cycle (stop `minecraft-b`, remove its `world` directory, start it, wait for "Done"). Never mid-cycle, never Server A. Region indexes restart at 0. Check `df -h /` at cycle start and before every batch; under 5 G free, stop launching and clean up before anything else.
2. **The nether pause is lifted for gyms.** The old rule "no nether work until the portal gate is met" is revoked. Late-game gyms run on spare bots during every wait (Part 6). Natural races still only measure.
3. **Movement near lava: one primitive, then deletion.** `lava_safe_move` replaces every raw walk that can end within four blocks of lava (Part 7). The legacy refill paths and per-site caps are deleted in the same build.
4. **Batches stop by rule, not by hand.** A batch may stop early only for (a) a death on a path this build changed, or (b) a build that produces no measurement (no bot reaches the step under test). Any other death is logged with its mechanism and the batch runs to completion. On top of that, the sequential test in Part 5 may end a comparison early when it has decided. Each stop writes one line in `CHANGES.md`: which clause, which bot, which log line.
5. **Race kit:** five buckets (`craft_bucket` 3 → 5; the iron target of 22 already covers two pickaxes, five buckets and flint and steel). The gym kit for any race step is that step's race kit, updated the same hour the race kit changes.
6. **Scripts are TypeScript run with node** (`node script.ts`, no build step, no python). Models ship as plain JSON files that either bot can load.

---

## Part 3 — The decomposition: twelve skills with contracts

The run is split into twelve skills. A skill is a function with a contract: a precondition (a kit and a situation that a gym can construct with server commands), a postcondition the harness checks from ground truth (RCON or server state, never the bot's own belief), a time budget, and an allowed death rate. A skill assumes only its precondition, never how the previous skill went.

| # | Skill | Precondition (gym kit) | Postcondition | Starting budget |
|---|---|---|---|---|
| 1 | Wood and stone tools | empty inventory, on land | stone pickaxe, crafting table, 16 logs | 6 min |
| 2 | Iron | stone pickaxe, 16 cobble | 22 iron (ore + ingots) | 20 min |
| 3 | Kit craft | 22 iron, coal, logs | 2 iron pickaxes, 5 buckets, water bucket, flint and steel | 5 min |
| 4 | Lava site | kit | standing at a sealed station beside a lava body with ≥ N sources | 10 min |
| 5 | Portal cast and light | at station, kit | lit portal, bot in the nether | 15 min |
| 6 | Fortress find | nether, food, 64 cobble, pickaxe | inside a fortress, spawner seen | 15 min |
| 7 | Blaze rods | at a spawner, sword, 64 cobble, food | 7 rods | 12 min |
| 8 | Pearls | nether, sword, 64 cobble, food | 14 pearls | 20 min |
| 9 | Stronghold | overworld, 14 eyes of ender, pickaxe | standing in the portal room | 15 min |
| 10 | End entry | portal room, eyes | in the End, on the platform | 3 min |
| 11 | Crystals | End, 128 cobble, water bucket, (bow if any) | all crystals destroyed | 10 min |
| 12 | Dragon | crystals gone, 6 beds, obsidian, food | dragon dead | 10 min |

The budgets are starting guesses. Replace each with the measured 80th-percentile time of successful attempts as soon as a skill has ten passes. Four rules make the table useful:

- Every skill gets a gym that starts from its precondition, built with RCON (`tp`, `give`, `execute in minecraft:the_nether run …`, `execute in minecraft:the_end run …`, `kill @e[type=end_crystal]`, `summon ender_dragon`, `time set`). The gym for skill 12 does not wait for skills 1–11 to work.
- The postcondition is checked by the harness from ground truth, never from inventory as the bot sees it. For the dragon that is the server's "Ender Dragon was killed" line or an `execute if entity @e[type=ender_dragon]` test that fails.
- Every skill attempt writes one event row (Part 4).
- A skill that exceeds its budget returns `failed(reason)` to the planner. It does not loop.

Each skill decomposes again into guarded primitives with their own post-conditions, in the pattern `lava_safe_move` sets (Part 7): named, guarded, verified from world data, steps back on failure, caller re-plans.

Your existing steps map onto this table: steps 1–5 of the table are the current `gather_wood` … `enter_nether` chain; 6 and 7 are `find_fortress` and `kill_blaze`; 8–12 do not exist yet. You are not rewriting the step machine this cycle. You are adding the contract (budget, postcondition check, event row) to every existing step, and building gyms for the missing skills in reverse order.

---

## Part 4 — The event log: nothing learns without it

Before any model, the bot writes one row per skill attempt and one row per guarded-primitive call to a shared schema. This is the dataset every later part trains on. `race.db` and `gym.db` are inputs to it, not replacements.

**Canonical file:** `data/attempts.jsonl`, one JSON object per line, append-only. Mirror it into a sqlite table `attempts` in `race.db` for queries, but the JSONL is what models read, and it is what the TypeScript bot will also write once its agent adopts the same schema. Write `docs/events-schema.md` describing it so the steve agent can match it field for field.

**Attempt row**

```json
{
  "run_id": "race-20261003-…-i7",
  "bot_impl": "rs",
  "build": "6c+lsm",
  "world_seed": -1234567890,
  "skill": "iron",
  "step_id": "mine_iron",
  "source": "race",
  "bot": "rust-race-002",
  "start_ms": 1760000000000,
  "duration_s": 812.4,
  "outcome": "timeout",
  "reason": "no ore gain for 300 s",
  "death_cause": null,
  "pos": [18640, 31, 300],
  "params": { "iron_band_lo": 8, "iron_band_hi": 24, "buckets": 5, "cluster_min": 100 },
  "context": { "y": 31, "health": 20, "food": 17, "time_of_day": 6000, "cobble": 64, "iron": 0,
               "hostiles_seen": 0, "lava_cells_8": 0, "water_cells_8": 3, "light": 0,
               "ore_sightings_memory": 2, "in_water": false, "air": 300 }
}
```

`outcome` is one of `ok`, `timeout`, `death`, `failed`. `params` is every constant in force for that attempt (the bandit reads and writes these, Part 5). `context` is the feature vector: what the bot knew when the attempt started.

**Primitive row:** same shape with `skill` set to the primitive's name (`lava_safe_move`, `leave_water`, `tunnel_step`, `station_refill`), `context` including the target and the local hazard counts, and `outcome` including `vetoed` when the post-condition check refused the arrival.

**Three disciplines**

1. **Grade with truth, observe only what is real.** The harness may use RCON and full chunk data to label outcomes. The `context` features may contain only what the bot legitimately sensed: its own position, health, food, air, inventory, blocks it has seen exposed, entities it has seen, light at its position, time of day. This is the no-X-ray rule restated as an ML rule, for the same reason: a model trained on leaked truth looks brilliant in the gym and fails in the race. Write the feature list in `docs/events-schema.md` with one line per feature saying how the bot senses it. Audit it once.
2. **Seeds are recorded and replayable.** Record the world seed (RCON `seed`) and the exact spawn position per attempt. The same seed, spawn and kit is a comparable trial, and paired comparisons on the same seeds need far fewer trials than unpaired ones.
3. **Append-only, one object per line.** Models, the rate report and the retrospective all read the same file.

---

## Part 5 — Honest numbers: intervals, sequential stopping, bandits

### 5.1 Print the interval next to every rate

A pass rate of k successes in n trials is printed as the Wilson 95% interval, never as a bare fraction. With z = 1.96 and p̂ = k/n:

```
centre = (p̂ + z²/(2n)) / (1 + z²/n)
half   = z · sqrt(p̂(1−p̂)/n + z²/(4n²)) / (1 + z²/n)
```

6/10 prints as `60% [31%, 83%]`. The rate report (`scripts/rate-report.sh`) and `gym-report.sh` print this for every row from now on. When someone reads "6/10 beats 4/10" they will see the intervals overlap and stop treating it as a result.

### 5.2 Stop comparisons by a statistical rule

When build B is compared against build A on the same skill, keep a Beta posterior per build: `Beta(successes + 1, failures + 1)`. After every trial, estimate `P(B > A)` by drawing 10,000 samples from each posterior and counting how often B's draw exceeds A's. Stop the comparison when that probability passes 0.95 (B wins) or drops below 0.05 (B loses). Otherwise keep running, up to a cap of 30 trials each. This ends obvious wins and obvious regressions early and keeps running on close calls, which is what the "two full batches" rule was trying to do by hand. Implement it as `scripts/ml/compare.ts` reading `attempts.jsonl` and printing the posterior means, the probability and the decision.

Pairing: when two builds run on the same seeds, also report the number of seeds where they differ and which way. Paired differences are a sharper test than the pooled rates.

### 5.3 Bandits for argued constants

Every constant anyone has argued about becomes an arm in a Thompson-sampling bandit. The mechanism is twenty lines:

- `data/params.json` holds, for each parameter, its candidate values and a Beta posterior per value: `{"buckets": {"arms": {"3": [a, b], "4": [a, b], "5": [a, b]}}}` where `a` = successes + 1 and `b` = failures + 1.
- At the start of an attempt, for each parameter, draw one sample from each arm's `Beta(a, b)` and use the arm with the largest draw. Record the chosen arms in the attempt row's `params`.
- When the attempt ends, increment that arm's `a` on `ok` and `b` otherwise. Write the file back.
- The bot reads `params.json` at the start of every attempt, so a value changes without a rebuild. The gym launcher and race launcher share the same file.

Sampling from Beta without a library: for integer-ish `a` and `b` of modest size, use the Gamma-ratio method (draw Gamma(a) and Gamma(b) by Marsaglia–Tsang, then `x / (x + y)`); Marsaglia–Tsang is about fifteen lines and is what you want in both languages. Write `scripts/ml/bandit.ts` as the reference implementation with a test that checks the mean of 10,000 draws from `Beta(3, 7)` is within 0.02 of 0.3, then port the same function into the bot and run the same test there. Two implementations that agree is how every learned component is checked in this project.

First arms, in this order and only after a skill's structure is right (a bandit tuning a broken skill produces a well-tuned broken skill):

| Parameter | Arms | Lives in |
|---|---|---|
| `buckets` | 3, 4, 5 | race kit, gym kit |
| `cluster_min` | 60, 100, 150 | lava siting |
| `iron_band` | (8, 24), (0, 16), (−8, 8) | `mine_ore` descent target |
| `retreat_hp` | 10, 14 | `fill_bucket` / station guards |
| `station_tiebreak` | distance-first, targets-first | `pick_station` |

Two bandits per cycle is enough. Report each arm's posterior mean and count in the retrospective; a parameter whose arms have not separated after 40 trials is reported as "no evidence", not as a choice.

### 5.4 Throughput

More trials per hour is the cheapest statistical power there is.

- **Tick acceleration.** Minecraft 26.x has `/tick rate <n>` and `/tick sprint`. Try `tick rate 60` on one water slug (shore, 10 trials) and compare its pass rate and durations in game ticks against `tick rate 20`. Accept it only if the rates match within their intervals and no wall-clock timer in the bot misbehaves (your combat pacing uses `wait_real_ms`; the breath watchdog uses wall time; both must be audited). Timebox 45 minutes. If it holds, gyms run at 60 and the race stays at 20, and every duration in the event log is recorded in ticks as well as seconds.
- **Lanes.** Six gym bots per batch are already fine on the box; measure TPS with `tick query` and the "Can't keep up" warnings before going to eight.

---

## Part 6 — Reverse curriculum: build the dragon gym this cycle

This is the single biggest change. The nether pause is revoked for gyms (decision 2). On spare bots, during waits, you build late-game gyms in reverse order, each with a scripted attempt and a failure log. The first deliverable is only the gym and the log; a pass rate is a bonus.

**Order:** dragon (12) → crystals (11) → End entry (10) → stronghold (9) → blaze rods (7) → fortress find (6) → pearls (8).

**Why backwards:** a reinforcement-learning curriculum that starts agents near the goal and moves the start earlier as the completion rate rises is how a drone project the user ran went from 0% to finishing. The same logic applies to engineering effort: each late gym surfaces a wall nobody has seen (the fortress enclosed approach is already one, from race i6). Finding those walls now, while the portal work continues, converts months of serial discovery into weeks of parallel discovery.

**The dragon gym, concretely (`SLUG=dragon`)**

Setup by RCON: `execute in minecraft:the_end run forceload add -64 -64 64 64`, wait for `execute in minecraft:the_end run execute if loaded 0 0 0`, then `execute in minecraft:the_end run tp <bot> 0 70 0` (the obsidian platform is at about (100, 49, 0); the fountain at (0, ~62, 0); adjust after a probe). Kit: 6 beds, 32 obsidian, iron sword, 16 cooked beef, 128 cobble, a water bucket. For skill 12 the crystals are already gone: `execute in minecraft:the_end run kill @e[type=end_crystal]`. For a re-run after a kill: `execute in minecraft:the_end run summon minecraft:ender_dragon 0 100 0`.

Scripted attempt, first version: stand on the fountain's top rim behind an obsidian block, wait for the dragon's perch (its position within a few blocks of the fountain and low), place a bed on the fountain beside its head, right-click it to detonate, repeat. Beds explode when used in the End and deal heavy damage to the dragon's head when it is perched; this is the deterministic, bot-friendly kill, not the bow. Guard with health: at hp < 10 retreat two blocks behind obsidian and eat. Log every detonation with the dragon's position, the damage you believe you dealt and the bot's hp. Pass = the server's dragon-death line within the budget.

**The crystals gym (`SLUG=crystals`)**: bot on the platform with full crystals. Scripted attempt: for each of the ten towers, pillar up a cobble column beside it (sneaking, `pillar_up` with a water bucket for the fall), break the iron cage with the pickaxe where there is one, and destroy the crystal from the side with any hit, stepping back first because the explosion damages. Projectiles also destroy crystals; verify in the gym whether a snowball or an egg does before relying on it, since that would remove the climb. Pass = `execute if entity @e[type=end_crystal]` fails.

**Stronghold (`SLUG=stronghold`)**: tp the bot to the surface 500 blocks from a stronghold (`locate structure stronghold` by RCON for the gym only; the race bot triangulates with eyes), kit of 14 eyes and a pickaxe. Scripted attempt: throw an eye, read its flight direction, walk 100 blocks, throw again, intersect the two bearings, then dig a 1×2 tunnel at the intersection down toward y 30 in a spiral until stone bricks are seen, then search the structure for the portal room. Design the dig as "dig toward coordinates", never as open pathing. Pass = standing within 3 blocks of an end portal frame block.

**Blaze and fortress (`SLUG=blaze`, `SLUG=fortress`)**: the existing harness scripts kit and tp a bot; convert them to gym slugs that write event rows. Fortress find starts where race i6's bot stood: a fortress visible in loaded chunks and every approach failing. Read that bot's log for the mechanism first (the tunnel toward the brick is blocked by the lava sea, the fallback `goto_near` over open ground never arrives). The design to test: tunnel at the fortress's own y through netherrack, bridging lava with cobble one block at a time, never crossing the sea in the open.

**Pearls (`SLUG=pearls`)**: the bot-friendly route is endermen in a nether warped forest, from a two-block-high alcove the bot digs so endermen cannot reach it while it hits their legs. Bartering is deterministic but the arithmetic is bad (roughly 180 gold ingots for 14 pearls). Gym: tp into a warped forest (`locate biome minecraft:warped_forest`), kit of sword, cobble, food. Scripted attempt: dig the alcove, look at an enderman to aggro it, strike when it arrives, collect the pearl, repeat. Pass = 14 pearls.

Each gym writes attempt rows with `source: "gym"`. The retrospective's funnel table (Part 9) will show, for the first time, a number for every one of the twelve skills, even if most late ones read `0/3`.

---

## Part 7 — The portal: one structural fix, then measure

The portal work this cycle is a deletion as much as an addition.

**`lava_safe_move(bot, target, reach) -> bool`** in a shared helper, with a doc comment that states the post-condition. Behaviour:

1. For anything farther than one block, use only the lava-aware pathfinder (it already refuses lava-adjacent cells and lava-ringed drop landings). No raw jumping walk, ever.
2. For the last block, the sneaking, non-jumping `settle_xz`.
3. Before returning true, verify from chunk data: solid non-lava footing under both feet columns, no lava in the 3×3×2 body ring, no missing floor in the four side cells.
4. If the check fails, step back one cell along the path just walked, write a primitive row with `outcome: "vetoed"`, and return false. The caller re-plans; it never improvises a stand.

Replace the call sites in one build: prepare's approach goto, the early-scoop walk, `tunnel_step`'s detour, the return to the anchor, every `cast_cell` stance and `ensure_platform` move, the station walk. Then delete: the 3.5-block `walk_to_xz` caps at the refill sites, `fill_bucket`'s stand search and approach, the three `REFILL_LEGACY` branches, the stance retries that re-plan beside the pool. `walk_to_xz` stays only where lava cannot be within four blocks (surface wood, centring in a dry tunnel).

Small gym first (`SLUG=lava_safe_move`): the existing synthetic lava pool arena, ten moves from the tunnel mouth to rim stands, pass = 10 arrivals with the post-condition true and 0 deaths. Twenty minutes, then on.

Then portal batches under Part 5's rules: run the new build against the previous build's recorded trials with `compare.ts`, stop when it decides or at 30 trials. Expected: deaths within four blocks of lava go to zero; what remains are timeouts. Every portal attempt writes its row with `cluster_min`, `buckets` and `station_tiebreak` drawn from the bandit.

Also in this build, the stall detector (Part 8) wraps every step, and the iron skill gets its budget, because race i6 showed iron finding losing a land bot for two hours. Reproduce bot 002 first: a `FixedSurface` gym at its lane with its kit, step `mine_iron`, 30 minutes, and an RCON probe for iron ore within 32 blocks of where it stood, so the fix is to a mechanism you read, not a guess. Expected shape: strip-mine in straight 1×2 tunnels at the band the bandit picks, record every exposed ore sighting in `WorldMemory`, range in a straight line rather than a loop. No chunk-data ore scan.

---

## Part 8 — Budgets and stall detection: the cheapest breakthrough

This is Level 1 of the learning system and it needs no training. For each step, two numbers from the log: the 80th-percentile duration of successful attempts (the **budget**), and a **progress metric** that only ratchets upward toward the postcondition: best iron count so far, closest horizontal distance to the lava target so far, highest mold layer completed, most rods held. "Progress" that can be increased without finishing (blocks dug, distance walked) is not a progress metric; the bot would farm it forever.

Rule: an attempt whose progress metric has not improved for `stall_s` seconds (start at 180, make it a bandit arm later) is abandoned with `outcome: "timeout", reason: "no progress in 180 s"`, and the planner picks an alternative: a new site, a new band, a different heading, a re-site. An attempt past its budget is abandoned the same way. No step may return the same `failed(reason)` more than three times in a row without changing something it controls (heading, site, band); the fourth identical failure escalates to the planner's alternative list.

Until each step has ten passes, use the budgets in Part 3's table. Write the measured budgets into `data/params.json` under `budgets` as they become available, so they hot-reload too.

Report in the retrospective: hours of stall time in race i6 that this rule would have cut (from its logs), and hours it actually cut in race i7.

---

## Part 9 — The plan, by the hour

Cycle 4 is 24 hours from the moment you start. Phase 0 is short on purpose: the longer the infrastructure takes, the less data the cycle produces.

| Hours | Phase | Gate |
|---|---|---|
| 0–1 | 0a: close cycle 3, reset `mc-b`, disk check, build | retrospective and briefing for cycle 3 written in 45 min from `CHANGES.md`; `mc-b` reset; `df` recorded |
| 1–4 | 0b: event log, intervals, budgets and stall detection, `params.json` and bandit | smoke race of 2 bots for 20 min writes attempt rows for every step; rate report prints intervals; a killed bot resumes in place; alarm ≤ 6.5 s |
| 4–9 | 1: `lava_safe_move`, legacy deletion, iron reproduction and fix, portal comparison | `compare.ts` decides new build vs old, or 30 trials each; 0 deaths within 4 blocks of lava; iron gym 5/5 to 22 under 20 min |
| 9–12 | 2: bandits live on `buckets` and `cluster_min`; tick-rate experiment | arms recorded in every row; tick-rate accepted or rejected with numbers |
| 12–16 | 3: late-game gyms: dragon, crystals, then stronghold | each gym runs, writes rows, has a failure log; the dragon gym has ≥ 3 attempts |
| 16–20 | 4: natural race i7 | 5 bots, 4 h, fresh region, cleared inventories, bandit arms live, never relaunched |
| 20–24 | 5: refit, funnel, documents, cycle 5 | posteriors updated; funnel table; retrospective and briefing; cycle-5 playbook written and started |

Late-game gyms (Phase 3) also run in every wait of three minutes or more in Phases 1 and 2, on gym bots 007–009, as long as the box has free disk and TPS holds.

**Phase 0a.** Write `docs/retrospective-2026-10-03.md` and `docs/briefing-2026-10-03.md` for cycle 3 from `CHANGES.md` in 45 minutes; the strategy agent already has your recap, so these are the record. Reset `mc-b` (decision 1). Build 6c on the box; it has never run, treat it as untested.

**Phase 0b.** The event log (Part 4) in the bot and both launchers; the Wilson interval in `rate-report.sh` and `gym-report.sh`; the stall detector and budgets (Part 8) around every step; `data/params.json` with the bandit reader in the bot and `scripts/ml/bandit.ts`; `scripts/ml/compare.ts`. Then the smoke race: two bots, 20 minutes, priority capped at the stone pickaxe, both must start on land (the beyond-strip placement is untested; if a lane is water, that is a Phase 0 bug), kill one bot's process by hand at minute 5 and confirm it resumes where it stood, and confirm the rate report shows both bots, the kill, and intervals. Also measure the breath-alarm latency in `water_descent`: race i6 fired it at 11.3 s against a 6 s rule, so something blocks the tick loop for about 5 s; find the blocking call and make it yield ticks.

**Phase 1.** Part 7 in full. The iron reproduction runs on gym bots while the portal comparison runs.

**Phase 2.** Turn on the `buckets` and `cluster_min` bandits for every portal attempt and the race. Run the tick-rate experiment (Part 5.4) on one water slug.

**Phase 3.** Part 6, dragon first. Three attempts minimum, each with its log. If the dragon gym passes once, run crystals; if crystals pass, chain them (crystals then dragon with no kit reset). Stronghold after that.

**Phase 4.** Race i7: 5 bots, Server B, fresh region, cleared inventories, 4 hours, `RACE_GOAL=nether`, bandit arms drawn per bot at start and logged. Watch the server log for deaths and arrivals. Everything else waits except late-game gyms on spare bots, which keep running if TPS holds.

**Phase 5.** Refit: update every bandit posterior from the cycle's rows (`bandit.ts refit`), print each arm's mean and count. The funnel table. The documents. Start cycle 5.

---

## Part 10 — What the strategy agent wants back

The briefing at the end of this cycle has these sections, in this order, every number with its interval and its row count.

1. **The funnel.** For each of the twelve skills: attempts, pass rate with interval, median time, deaths, source (gym or race), and whether the postcondition was checked from ground truth. Late skills with `0/3` are expected and are still rows.
2. **Stall time recovered.** Hours of no-progress time in race i6 (from its logs, applying the Part 8 rule after the fact) and in race i7 (actual). This is the cheapest result of the cycle; show it.
3. **Bandit posteriors.** Each parameter, each arm, mean and count, and whether the arms separated. "No evidence yet" is a valid finding.
4. **The portal comparison.** Old build vs `lava_safe_move` build: `P(new > old)`, trials used, deaths within four blocks of lava per build, line count of `portal.rs` before and after.
5. **The dragon gym's first failure log**, verbatim: what the scripted attempt did, where it died or stalled, what the server log shows. Same for crystals and stronghold if reached.
6. **Tick-rate result:** accepted or rejected, with the two rates and their intervals.
7. **Measurement integrity:** every launch has a row; any attempt without a row is a bug, counted.
8. **Open problems ranked by expected minutes saved per race**, computed from the funnel (attempts × failure rate × median loss), not by which failure was most recent.
9. **Line counts** under their own heading, including the existing lava cluster scan flagged as "reads unexposed blocks".
10. **Decisions for the strategy agent**, and the standing request to the user for commit authorisation.

Also in the retrospective, in plain words: what the learning-system changes made easier, what they made harder, and what you would not do again.

---

## Part 11 — Standing rules for every cycle after this one

**The gate chain.** The project advances through gates, in order, and work is chosen by which gate is next; a gate is met only by a rate table with intervals from a clean, fresh-region measurement. (1) Surface survival: zero drownings and surface deaths across a 4-hour, 5-bot race — met in race i5, regressed and re-fixed in i6, to be re-confirmed in i7. (2) Portal from scratch: ≥ 4 of 5 natural bots enter the nether with a median under 60 minutes from spawn and zero cast deaths. (3) Rods: 7 blaze rods in ≥ 2 of 3 natural runs, zero lava deaths. (4) Pearls: 14 in ≥ 2 of 3 runs. (5) Stronghold: portal room reached in ≥ 2 of 3 runs, by dig-toward-coordinates. (6) End and dragon in ≥ 2 of 3 runs from a kitted teleport first, then naturally. (7) End to end: ≥ 3 of 5 natural bots beat the game.

**The curriculum runs alongside the gates.** Gates set what the natural race must prove next. Gyms for every later skill run in parallel on spare bots, in reverse order, from the first cycle in which they exist. A gate being unmet never pauses a later skill's gym.

**Planning each cycle.** Rank open problems from the funnel by expected minutes saved per race. The top item that blocks the next gate is Phase 1; the next two are Phases 2 and 3; infrastructure that corrupted a measurement is Phase 0 and short. Write `docs/playbook-<date>.md` before touching code, with hour budgets and a gate per phase. Measurements outrank the playbook; record deviations in the status line.

**Design principle.** Terraform to a template, execute a fixed script, verify from chunk data or RCON before the next step. Chunk-data scans of exposed blocks for "where is X"; enclosed tunnels for "died getting there"; fixed stands and read-back for "the pour, the hit, or the dig landed wrong"; a guarded primitive with a post-condition for any movement near lava, water or a drop.

**Learning rules.**
- No constant is changed by argument if a bandit arm can test it.
- No model or feature may use unexposed world data.
- Every learned component ships as JSON, with a hand-written inference function and a cross-implementation agreement test, and a flag that restores the scripted rule.
- A change that improves a gym rate and not the race funnel is recorded as unproven.
- Tuning comes after structure: a bandit never tunes a skill whose failures are structural.

**Batch rule** (decision 4) and **disk rule** (decision 1) apply to every gym and race.

**Loop mechanics.** Ticks end on events, not timers, wherever the harness allows. Status every two hours or at any phase boundary: phase, passed, failed, the current funnel rows touched, decisions made, stop clauses used, free disk, "tree uncommitted". Deliverables go out as soon as they exist. `CHANGES.md` current at all times.

**When stuck.** Geometry question: probe. Repeating failure: snapshot, gym, then edit. Adding a case: stop; make the world more constant. A constant: make it an arm. Waiting: the next late-game gym. A measurement you cannot trust: fix the harness first, however small the bot bug looks.

**Stopping.** Only the user stops this loop. If you have nothing measured and nothing to fix, you have a measurement problem: run the regression gyms and the funnel.
