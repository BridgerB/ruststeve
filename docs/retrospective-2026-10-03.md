# ruststeve — cycle 3 retrospective (2026-10-02 14:21 → 2026-10-03 08:17 MDT)

Cycle 3 ended early: the user stopped the loop at about 07:40 UTC (01:40 MDT) for a strategy call, and cycle 4 started at 08:17 UTC (02:17 MDT). `CHANGES.md` (cycle 3 sections, Phases 1–6) is the evidence for every number here.

## Goal

The playbook (`docs/playbook-2026-10-02.md`) had five phases: the crafting-table loop, eye-height `head_in_water`, long-range wood, the portal's one-death mechanisms, and a natural race. A sixth phase grew out of the race: the sealed refill station and the water fixes it exposed.

## Outcome against the gates

| Gate | Result |
|---|---|
| 1. Surface survival (0 drownings, 4 h, 5 bots) | Met in race i5 (cycle 2). **Regressed in race i6: 2 drownings, both ghost air.** Fixed and regression-tested, not yet re-confirmed in a race. |
| Portal gym (≥ 8/10 clean, each < 15 min) | **Not met.** Best clean batch: 3/3 finished trials clean at 919, 939 and 1110 s (batch 6b), all just over the time limit. One clean pass inside the gate this cycle: 711 s (batch 3). |
| Nether, natural | **1/5 in race i6** (rust-race-001, 177 min, from an empty inventory): the first natural nether entry of cycles 2–3. |

## Rate tables

### Race i6 (`race-20261003-001427-i6`, 4 h, 5 bots, fresh region x 18900)

| Bot | Lane | Furthest | Ended | Deaths |
|---|---|---|---|---|
| 001 | land | 10/28 | find_fortress (nether) | 1 (ghost air) |
| 002 | land | 5/28 | mine_iron, 74 × "mined 0/22 iron" | 1 (ghost air) |
| 003 | land | 8/28 | build_nether_portal, 196 min digging bedrock over a −62 pool | 0 |
| 004 | ocean | 0/28 | gather_wood | 0 |
| 005 | ocean | 0/28 | gather_wood | 0 |

Nether 1/5, i.e. 20% [4%, 62%] (Wilson 95%). Land lanes 1/3, i.e. 33% [6%, 79%].

### Water regressions (7 slugs × 2 trials, 0 deaths is the gate)

| # | Build | Deaths | Note |
|---|---|---|---|
| Phase 2 | eye-height head_in_water | 0/14 | accepted |
| 1 | ghost-air breathable test | 0/14 | accepted |
| 2 | descent head-under-water escape | 1/14 | frozen-swim drowning (cave_iron) |
| 3 | server-ghost re-sync guess | 2/14 | guess disproved, reverted |
| 5 | rising-leg centring + body-side lava seal | **0/14** | 12 pass, 2 non-lethal timeouts; first aquifer pass in many batches |

### Portal batches (gym, 5 bots × 2 trials each)

| Batch | Build | Clean | Clean times | Deaths | Stopped |
|---|---|---|---|---|---|
| A (cycle-3 Phase 4) | walk cap | 4/10 | 13.6–27.2 min | 3 | full |
| B | 14-hp retreat | 1/5 | 38.2 min | 2 + loop | at 5/10 |
| 1 | station v1 | 2 | 398, 663 s | 1 (cast_cell ghost bowl) | early |
| 2 | station fixes | 4 | 1066–1606 s | 1 (descent drowning) | early |
| 3 | reach + descent | 1 | **711 s (in gate)** | 3 (one bot, old paths) | early |
| 4 | early scoop via station | 0 | — | 0 | early (no measurement) |
| 5 | rank from bot | 2 | 1072, 1655 s | 1 | after trial 1 |
| 6 | tunnel "stay put" | — | — | 1 (my change) | early |
| 6b | tunnel step-back | 3/3 finished | 919, 939, 1110 s | 1 (anchor return) | early |

**Since batch 1, no bot died inside the station refill itself.** Every portal death after that was on a different path around the pool: the approach goto, the early scoop's old `fill_bucket`, the tunnel detour, the return to the anchor, a layer return.

## What went right

- **The frozen-swim root cause was proven, not guessed.** RCON on a live frozen bot showed server Pos y exactly 71.2 (head top 73.0), with the 0.6-wide box overlapping the neighbour column by 0.001 and that column solid at head height. The 0.15 centring threshold on rising legs fixed it: water regression 5 had 0 deaths and the first aquifer pass in many batches.
- **Ground truth beat two wrong theories.** "Server ghost cap" (disproved by the new correction log) and "cluster ≥ 100 too strict" (it was bedrock) were both found wrong before they shipped.
- **The station design was revised before coding**, because overworld lava does not regenerate. Without that, the first build would have exhausted itself after one bucket per site.

## The wrong shape, and why

- **Whack-a-mole around the pool.** Each batch found the next legacy path that walks near open lava. They were patched one at a time and each patch cost a batch. A single guarded movement primitive near lava was the structural answer; it is cycle 4's Part 7.
- **No build completed two full batches.** Seven batches were stopped early: five on deaths, two because they could not measure. The two-batch rule never applied cleanly, so no build has a trustworthy rate. Cycle 4 replaces it with a stopping rule.
- **Stalls were unbounded.** Race i6 lost about two bot-hours to iron finding and three to bedrock and fortress loops with no budget.

## Open problems (inherited by cycle 4)

1. Movement near lava has no single guarded primitive (cycle 4, Part 7).
2. Steps have no budget or stall detection (Part 8).
3. Speed: clean passes take 919–1655 s; pool exhaustion and re-siting dominate.
4. Iron finding (race i6 bot 002).
5. The nether "enclosed approach" to a visible fortress.
6. The water_cave hole climb-out (non-lethal).
7. The breath alarm fired at 11.3 s instead of 6 s in race i6.
8. race-b's beyond-strip placement is untested.

## Infrastructure

- On 2026-10-03 the box disk hit 100%. It was cleared to 14 G free, with a journald cap and a weekly nix GC added to `/etc/nixos`.
- The Minecraft ports are now firewalled to the user's IPs. Both servers are online-mode=false with no whitelist.
- Cycles 1–3 were committed and merged by the user (PR #2, `41a8f55`).
