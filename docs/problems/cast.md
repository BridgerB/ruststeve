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

## 3. Ops out of reach by their centre (baseline `sealava` race 45350748165: 7 casts `Op` at `:place :wall`, 3 at `:dig :room`)

- **What happened**: places 5 and 6 failed `place :wall` in every lane at the wall behind the frame's top cells, (60, -50, -182) from the stand (61, -51, -178) and (-151, -48, 92) from (-147, -50, 92), 53 to 116 ops in (in race 45343519424 lane 1 too, after 116 ops and 6,440 ticks).
- **Cause**: the approach takes an op as in reach when the eye is within `REACH` 4.0 of the cell's centre; from the top stair the wall's centre is 4.12 off and no stand of the room is nearer, so every try failed. The server takes a dig or a click when the eye is within 5.5 of the block's box (`canInteractWithBlock`, the 4.5 range plus 1).
- **Fix**: the approach measures the eye to the cell's box (`cast::box_dist`), the 4.0 kept; the wall is 3.54 off by its box. Unit test `the_wall_behind_the_top_cell_is_in_reach_by_its_box` (plan, rays.rs). Notes: "box reach N" on every cast ending (ticks an op was taken as in reach by the box and not by the centre).

## The baseline (race 45350748165, 8c3d038: the code before walls 1 to 3)

`sealava`, 3 lanes x 9 bots, cap 24,000, goal `portal`: **15 of 27 lit** (55.6%), in 113 casts begun. Places 4 and 8 lit 3 of 3; place 0 none. Casts failed: `Op` `place :wall` 7, `NoRefill` `NoSight` 5, `NoRefill` "no route, tunnels" 3, `Op` `dig :room` 3, `Op` `pour :lava` 3, `NoRefill` `NoWay` 2, `place :floor` 3, `Exposed` 1; "cast out of lava" 38 times (17 at the first cell), "no lava to fetch" 9.

## Raced: walls 1 to 3 (race 45354357884, c073b1a)

**20 of 27 lit** (74.1%) in 57 casts, against 15 of 27 in 113. By place (baseline, then this): 0 0/3, 0/3; 1 2/3, 2/3; 2 2/3, 3/3; 3 1/3, 1/3; 4 3/3, 3/3; 5 1/3, 3/3; 6 2/3, 3/3; 7 1/3, 2/3; 8 3/3, 3/3. `place :wall` failures 7 to 0, `NoRefill` 10 to 1, "cast out of lava" 38 to 12. Firings (failed and resumed casts' notes; a lit cast's note carries only its uses): "box reach" in 17 of 20 endings (336 ticks); "refused sources" 16 in 4 casts; "tunnel down" 24 times, "tunnel level" 13; the fetch walk skipped sources in reach 19 times of 27. Casts failed now: `dig :room` 3 (place 7, "walk to the stand failed NoProgress" four times, 0 ops in), `place :floor` 3 (place 0), `pour :lava` 3, `NoRefill` 1, `fall-clear` 1.

## 4. Stairs down cut the room's own floor (race 45354357884: place 0, 0 of 3, 304 regressions in one cast)

- **What happened**: rs-l00-b00 cast at (28, -52, -64) for 13,280 ticks and failed `Op` at `place :floor [26 -53 -66]` after 366 ops and 304 regressions, each from the first lava pour back to that floor; its refill trail "168 sources, no station / tunnel down toward (28, -55, -71)" over and over. The same in all three lanes (299 to 394 regressions).
- **Cause**: wall 2's stairs down start from the room's floor and the first step cuts the floor cell in front, a cell the plan places; the cast places it again, the next refill cuts it again. And the cast's clock counts an op done again after a regression as progress, so no invariant ended it.
- **Fix**: the stairs down are refused when a straight step down to the source's height opens the frame's rock or a placed cell (the level tunnel as before; trail "tunnel down refused at …"); a cast that goes back `CHURN_LIMIT` (60) times without reaching past its furthest op fails `Churned` (failed casts went back 2 to 10 times, or about 300).

## 5. Lava stands seen by the sampled line only, stairs below the sea (race 45354357884)

- **Read**: every lava scoop routed to a "stand sampled" failed `NoSight` (15 of 15 by tick 11,860; the trail names each), and wall 1's refusal then moved to the next source with the same result. Wall 2's stairs went six down: "feet (-69, -58, -101)", under the sea's surface.
- **Fix** (34352b8): a lava scoop routes only to stands the exact line sees from (water keeps the sampled ones, 109 of 112 filled); the trail counts "sampled skipped". The stairs down stop with the feet one over the source.

## Raced: wall 5 (race 45358066420, 34352b8)

**20 of 27 lit** again, in 61 casts. By place: 0 0/3, 1 3/3, 2 3/3, 3 1/3, 4 2/3, 5 3/3, 6 3/3, 7 2/3, 8 3/3. "cast out of lava" 12, "no lava to fetch" 7; no lava scoop routed to a sampled stand; "tunnel down ended" 13 by mid-race. Casts failed: `place :floor` 4 (place 0's churn, wall 4), `dig :room` 3 (place 7), `pour :lava` 3, `NoRefill` 1.

A real cast beside it (agent O's race 45354021804 lane 0, 9966559, without these fixes): from the `ready` snapshot, a site reached through a shaft the bot dug itself at y -53, lit in one cast of 6,340 ticks (124 ops, 10 fills).

## Raced: wall 4 (race 45361499544, a316d36)

**22 of 27 lit** (81.5%) in 85 casts. By place: 0 0/3, 1 3/3, 2 3/3, 3 2/3, 4 3/3, 5 3/3, 6 3/3, 7 2/3, 8 3/3. `Churned` ended 9 casts (8 bots; places 2 and 4 churned once and lit at their next site); "tunnel down refused" 9 times. Place 0 still churned: its refill fell to the level tunnel, which cut the same floor cell, (28, -52, -67), 61 times. 17b78e2 checks the level tunnel the same way, and a refused tunnel fails the refill to the fetch.

## 6. A failed walk to an op spends a try (sealava place 7, every lane: `dig :room` 0 ops in)

- **Read**: "tries: walk to the stand failed NoProgress" four times, 800 to 940 ticks into the cast, at its first room dig (-32, -53, -124).
- **Fix** (56395f1): a walk to an op that fails on its way digs the straight way's first block, as a walk not found does (wall 22 of portal.md), before a try is spent. Notes: "walk digs N".

## Raced: wall 4's level tunnel and wall 6 (race 45365616984, 17b78e2)

**23 of 27 lit** (85.2%) in 48 casts. By place: 0 0/3, 1 2/3, 2 3/3, 3 3/3, 4 3/3, 5 3/3, 6 3/3, 7 3/3, 8 3/3. Casts failed: 4 in all (`Churned` 1, `Exposed` 1, `NoRefill` 1, `dig :room` 1); "tunnel … refused" 8, `Churned` 1. Wall 6's "walk digs" is printed only on a failed cast's note, and none of the four counted one: its firings are not counted, and place 7 lit 3 of 3 (2 of 3 before).

## Status

- `sealava` (development evidence only), cap 24,000, 27 bots a race: baseline 15 (8c3d038, 113 casts); 20 (c073b1a), 20 (34352b8), 22 (a316d36), 23 (17b78e2, 48 casts).
- Open: place 0 (37.5, -52, -74.5) lights none in any race (its sites have no station and every tunnel would cut its own floor); wall 6's firings uncounted; no real race from `ready` on this code yet.
