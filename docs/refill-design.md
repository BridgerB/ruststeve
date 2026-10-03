# Refill redesign: a sealed scoop station (draft, cycle 3)

## The failure, measured

Across cycle 3's portal batches (regions 45–48), every death that survived the earlier fixes happened during a lava **refill**, inside the pool's own footprint:

| Batch | Death (CAST log) | Where |
|---|---|---|
| A | `fill try … bot=(…,−55.6,…) hp=14 → 4` | 5 s after `fill lava: ENTER`, from the −51 platform into the pool cavity |
| A (walk cap) | `FOOTING overlaps lava`, `hp=7`, then `hdist=6.2 hp=0` | beside a pool whose rim was eroded by earlier scoops |
| B | `ABORT low health=6 at (…,−57.2,…)` | 2 below the −55 sources |
| B | `ABORT … below=lava` at −56.7 (death loop, 5 deaths) | standing on lava inside the cavity |

The fixes so far (stand repair, walk caps, the 14-hp retreat, drop guards) each closed one route into the pool, and the bot keeps finding another. The shared cause: **every refill re-plans a stand beside a pool whose shape it is changing**. Each scoop removes a source, the neighbours flow into the hole, and the "safe stand" next to it becomes a ledge over flowing lava.

## Design: one fixed, sealed station per portal

Terraform once, then execute a fixed script; never re-plan beside live lava. This is the cycle-2 design principle, applied to the refill.

1. **Choose the station once,** at the first scoop: a solid cell S on the pool's rim with feet at `surface + 1`, plus a target source T diagonally or orthogonally adjacent, below S's floor level.
2. **Seal the station:** place cobble on every horizontal neighbour of S at feet and head height except the side facing the frame, and on the cell above S's head. The bot stands in a 1×1×2 cup whose only opening faces the frame and whose floor is solid.
3. **Scoop through a channel:** T is reached by aiming down past S's rim edge, so the bot never enters T's column. After each scoop, refill T's column only if it was a source with lava neighbours; otherwise let flow refill it from the lake.
4. **Fixed path between station and pad:** a cobble walkway at feet `surface + 1`, built once and checked (re-laid) before every refill, never through the pool footprint. Movement uses the pathfinder only along this walkway.
5. **Abort rule:** if S is no longer intact at the start of a refill (floor not solid, or lava in any sealed neighbour), rebuild the seal from the walkway. **Never pick a new stand beside the pool mid-cast.**
6. **Exhaustion:** if T and its neighbours stop yielding (3 failed scoops), re-site to the next cluster and build a new station. Never scoop from a degraded rim.

## Revision before code (2026-10-02 22:40 MDT)

Step 3 above is wrong. **Overworld lava never regenerates sources:** a scooped source's neighbours flow into its cell as flowing lava, which an empty bucket cannot take (the ray uses source-only fluid clipping and passes through it). One target yields one bucket. A race bot carries 2 empty buckets, so a 10-cell frame needs 5 refill trips. The station therefore needs a **lane** of targets, not one:

- **Station** = stand cell S (solid non-lava floor at `fy − 1`, air body) plus an open direction d toward the pool. The open cell O = S + d has air at feet and head height, with the pool below it.
- **Targets reachable through O with nothing dug:** O's column at `fy − 1` down to `fy − 3`, and the next column out (O + d) at `fy − 1`. The ray from the eye crosses O at head height, then O + d at feet height, so both must be air. In a lake that is several levels deep this is 2–5 buckets per station.
- **Seal (as placements, not a cup):** place cobble only on cells that can hurt: lava in S's or O's side cells at feet or head height, and any side cell of S with no floor (a drop). The entrance is S − d. The bot never digs at the station: no occluder digs, no roof digs. A target whose ray is occluded is skipped.
- **Scoop sneaking.** Sneak stops the bot walking off the edge into O.
- **Exhaustion → next station** along the rim (a fresh S with ≥ 1 target, outside the mold footprint), at most 2 re-sites per refill. With no station anywhere, the refill fails and the mold aborts the layer. It never falls back to an improvised stand.
- **Guards:** before each activate, check the footing (no lava under or in the body); at hp < 14 stop sneaking, step back to S − d, and eat.

## Verification

- A gym slug `refill_station`: kit plus a 5×5 lava pool at depth (the existing `LavaPool` geometry), 10 refills in a row. Pass = 10 buckets, 0 deaths, and the bot's feet never inside the pool footprint (logged per tick).
- Then two portal batches on the station build (the two-batch rule). Gate: ≥ 6/10 clean, 0 refill deaths.

## Scope

Touches `portal::fill_bucket` (the stand/approach path) and `portal_mold::refill_lava` (the station path). Retires the per-refill stand search, the stand repair, and both walk caps' refill call sites once the station proves out.
