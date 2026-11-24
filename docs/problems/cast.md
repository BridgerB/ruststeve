# The cast at the lava sea (issue 16)

The overworld leg's portal cast, for a real bot from the clean `ready` snapshot that came down by its own stairs (agent O's `docs/problems/overworld-leg.md` wall 1) and stands in rock or a cave at y -52 to -56 beside the lava sea. The older story of the cast is [`portal.md`](portal.md) (issue 14, walls 1 to 49).

- **Needs**: one site held to the end, ten lava refills from the sea, the water kept, every op of the plan reached from a stand the bot can get to.
- **Risks**: a site whose lava is in sight but out of a bucket's reach from anywhere the bot can stand; a refill that chooses the same failing source again; ops over the head with no stand under them.
- **Done, in world terms**: portals lit in most `sealava` development lanes twice running with each fix's firings counted; then a real race from `ready` lighting at last night's rate (about 1 in 2 bots that reach the sea) or better.

## The read (2026-11-24): race 45343519424 (afa3de5), lanes 0 and 1, 9 casts, 0 lit

| lane | cast | ending | what the files say |
|---|---|---|---|
| 0 | (26, -52, -138) S, lava (32, -55, -134) | `Op` after 13 ops at `:dig :room [30 -47 -136]` | a room cell five over the floor; the same op failed at the same site in race 45337088346 lane 1 |
| 0 | (27, -52, -122) E, lava (30, -55, -119) | out of lava after 74 ops, then "no lava to fetch" | "286 sources, no station", "no route, 1 tunnels"; the fetch walked toward a source it was in reach of, the walk ended at once, the site went the same tick |
| 0 | (16, -55, -109) S, lava level | `Op` after 139 ops at `:dig :fall-clear` | 35 regressions, the lava pour going back to its hole's dig; 7 refills filled |
| 1 | (1, -56, -224) W, lava (-9, -55, -231) | `Op` after 116 ops at `:place :wall`, 6,440 ticks | refills from ten blocks off, 9 filled |
| 1 | (-6, -54, -244) S, lava (-8, -55, -242), four tries | out of lava after 29 ops; out of lava after 0; `NoRefill` after 4 | one source, (-7, -55, -233), "routed" then `NoSight` 14 times over three casts and the fetch |

**Ranked by casts lost**: the lava refill 4 (wall 1: the same failing source chosen again, 2 sites; wall 2: no real station at a site three over the sea, 1; the fetch's walk in reach, the same cast), ops out of reach or undone 3 (wall 3 onward, read next with O's tries text), cut by the cap 1.

## 1. The refill chooses the source that just failed (race 45343519424 lane 1: one site lost after 29 ops, the fetch failing on it too)

- **What happened**: "scoop (-7, -55, -233) routed" then "failed NoSight" 6 to 9 ticks later, four times in a refill, four times in the next cast, and the fetch's own scoop took the same source and failed `NoSight` in 0 ticks. The second fetch, with that source refused by the planner, filled a bucket at (-9, -56, -232) in 160 ticks.
- **Read** (probe 45350515756, `why --at 45020 / 45120 --blocks -7,-55,-233`): the bot at (-5.50, -53, -233.33) in a one-wide tunnel it cut at x -6, y -53; the source two down and one west behind the deepslate at (-7, -53, -233) and (-6, -54, -233). The exact line from the eye to every face enters deepslate.
- **Cause**: the refill takes the nearest sources each time (`Cast::refill`, `station_among`), with nothing kept of which failed; and `Scoop::route_now` returns the first stand that routes, chosen by the sampled line from its middle (`stands`), without asking the exact line the use asks (wall 34).
- **Fix**: the cast keeps the sources its scoop failed at (`Cast::refused`) and leaves them out of the station and the fallback refill; the planner refuses them for the fetch. `route_now` tries stands the exact line sees from first (the sampled ones after, portal.md wall 27). Notes: "refused sources N" on every cast ending; "stand exact" / "stand sampled" on every routed scoop in the trail.

## 2. The fetch walks toward lava already in reach (race 45343519424 lane 0, tick 29,920)

- **What happened**: "cast out of lava after 74 ops … site kept; fetch lava (1 of 2)", then in one tick "walk toward lava to fetch at (29, -55, -118)", "explored to (29, -55, -118)", "no lava to fetch … site given up".
- **Cause**: `lava_fetch` walks to within `scoop::REACH` of the nearest source no scoop routes to; the bot was already there, the walk ended at once, the source was refused by the walk, and the fetch, due again, found nothing and gave the site up.
- **Fix**: sources within `FETCH_NEAR` (reach plus a block) of the eye are no fetch walk's target; the walk note says how many were skipped.
- **Open**: the site itself (floor y -52, the sea's surface y -55) passed `cast::station` by its "lava up to three below the landing, a tunnel reaches it" clause, and at the refill there was no station ("286 sources, no station") and a level tunnel at the floor's height never comes within reach of a surface three below. Read on the `sealava` races before any change: portal.md wall 23 refused such sites and lit fewer.

## Status

- f449f5b: walls 1 and 2 written, clippy clean, not raced. Baseline `sealava` race 45350748165 (8c3d038, the code before them) running.
