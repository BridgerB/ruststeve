# The portal (issue 14)

## Needs
From the `kit` start (an iron pickaxe, two buckets, flint and steel, a stone pickaxe and sword, a table, sticks, 16 cobblestone; spread 128), within 72,000 server ticks: a water bucket filled at surface water, lava seen with an exposed face (a surface pool, a cave, or the lava sea under y -54), a site beside it, the room cut, ten cells cast (a lava source and a water pour each), the inside opened, and flint and steel struck on the frame.

## Risks
- Lava: refills at a pool, deaths at the frame (both earlier bots' dominant costs).
- Strict no X-ray (Bridger, 2026-11-15): lava hidden in the rock is met only when a dig exposes it; then the site is given up.
- Going down to the lava sea through deepslate is slow.

## Done, in world terms
At least 28 of 56 real bots light a portal within 72,000 ticks of the `kit` start, confirmed by two 8-lane races pooling at least 50%.

## Settled before the races
- **No lava is lost** (14c model trace, 12-source pool): each refill takes one source and each cell spends one, 12 down to 2 over ten cells. The earlier "pool spent at cell nine" was the scoop's reach: the sources left lay four or more blocks from the bank (now reached at 4.4 across the flowing edge).

## Walls

### 1. Every scoop sent level (races 43737653668 and 43737660812, be789ee: 0 of 112 each)
- **What happened**: no bot lit a portal; 111 of 112 never began a cast. Failures over the first race: `ScoopNoScoop` 17,327, `ScoopNoSight` 4,950, `ScoopNoWay` 3,058.
- **Read**: rs-l00-b00 ended at the cap holding what it started with, 304 scoops unanswered, each source refused in turn.
- **Cause**: the 14c branch was cut from 14b, which was cut from 14a before 14a's fix that sends the look's pitch with `use_item` (f39f801; the server snaps the player to that rotation before the use). Every scoop went out at pitch 0, level, missing the water.
- **Fix**: main (which has the fix through #57) merged into 14c.

### 2. Idle with no note when no water is in reach (race 43737653668, be789ee)
- **Read**: rs-l00-b03 made no walk and no note for 13,000 ticks (42,408 route expansions): every surface source it knew had no way, and no explore walk left where it stood.
- **Cause**: the scoop's choice, finding nothing, set a retry and went idle without counting a failure, so `MAX_UNWALKABLE` never tripped and the bot never relocated (nor swam).
- **Fix**: that path counts a failed choice and notes it; twelve in a row relocate as any other.

### 3. Explore walks cycling over the same ground (race 43737653668, be789ee)
- **Read**: lane 0 of the first race: 10 of 14 bots scooped within 500 ticks; rs-l00-b04 (310 explores) and rs-l00-b12 (190) never found water, cycling the same six targets on a dry mountain at y 87 for the whole race (a quarter turn after each `NoProgress`, the same two successes in between).
- **Cause**: an explore's target is a heading's distance from the feet, with nothing kept of where the bot has been; on ground that turns it back, it walks back.
- **Fix**: explore targets reached are kept (the newest 24), and a new explore never aims within 12 blocks of one (clojurecraft's hunt never walks back over its ground).

### 4. A site in another cave (race 43744366804, d595300: 1 of 112; official 43745698501: 0 of 56)
- **What happened**: scoops worked (11 unanswered, from 17,327); 44 bots began a cast and none finished but one; lane 0's twelve cast endings: 8 a room dig `Op`, 2 `NoRefill`, 1 a landing dig, 1 lit (76 ops).
- **Read**: rs-l00-b02 chose a site at y -52 beside the lava sea from y -32 (the lava seen down a cave) and failed its first room dig three times: no walk reached the landing, 20 blocks down in another cave.
- **Cause**: `choose_site` took the landing's standability as enough; no route to it was looked for (the churn invariant every other choice keeps).
- **Fix**: the passing sites (12 at most, landings nearest the feet first) are taken best first only when a route to the landing is found (4 tried).

### 5. Lava hunted blind (race 43744366804, d595300)
- **What happened**: of 112 bots, 67 never began a cast (no site: no lava seen within 48 blocks with a passing site); the hunt's stairs failed `Boxed` 25,652 times over the race (up to 2,556 for one bot).
- **Cause**: with no site, the hunt only cut stairs down toward the lava sea on the held heading, whatever lava memory had seen further off; clojurecraft's hunt heads for sources seen out to 160 blocks.
- **Fix**: with no site, the stairs head for the nearest lava memory saw exposed within 160 blocks (the heading set toward it, down to its height); only with none seen do they go down toward the lava sea.

### 6. Lake-middle scoops, a lava bucket that counted as none, and cast stalls resumed (race 43752823642, 3dd00e6: 2 of 112)
- **What happened**: wall 4 held (cast `Op` failures 80 to 11), but 86 bots never began a cast and 24 casts failed (`NoRefill` 21).
- **Read**: rs-l00-b08 (lane 0): thirteen scoops at sources along one lake, each failing `NoWay` within 15 ticks; 111 tunnels for coal; seven casts cut by the planner's watchdog ("cast stalled") and resumed at the same site.
- **Causes**: a scoop started on a partial route (accepted for explores), toward the lake's middle; the `portal` kit counts an empty bucket, and a lava bucket held after a cut cast left it one short, so the planner went for iron, smelting with coal; a cast the watchdog cut kept its site.
- **Fixes**: a scoop starts only on a found route; a lava bucket counts as the cast's bucket; a cast the watchdog cuts gives its site up.

### 7. Hunting lava by stairs in place (official race 43753607071, b49521a: 0 of 56)
- **What happened**: 35 of 56 bots filled their water and never began a cast; 12 never held water; 9 casts failed (`NoRefill` 15).
- **Read**: rs-l00-b00 stood at (-56, 4, 68 to 71) from tick 15,725 to the cap "hunting" lava at (-32, 3, 17), 58 blocks off: 1,239 hunts, each six stairs and a tunnel that left it where it was. rs-l00-b03 looped boxed stairs, relocations and swims (2,002 `Boxed`).
- **Cause**: the hunt only cut stairs and tunnels on a heading set toward the lava; a diagonal heading turns a stair a quarter, so it paced. A walk was never tried, though lava seen 58 blocks off is often reachable over open ground.
- **Fix**: the hunt first walks toward the lava (to within 9), the target refused a while once walked to; stairs only where no route gets nearer.

Also fixed in cc60822's follow-up: a refill's route may be partial when it ends within reach of the source (the walker keeps off cells beside lava); the hunt skips lava within a given-up site's radius (it chased the pool it gave up, under it and out to the world's edge).

### 8. The lava hunt dug south whatever it aimed at (race 43761772012, e7ccfb0: 1 of 112; 1 of 112 and 0 of 56 beside it)
- **What happened**: 73 of 112 bots filled their water and never began a cast; drownings rose to 11.
- **Read**: rs-l00-b01 saw lava 60 blocks down at (-25, 3, 15), came within 43, 25 and 15 blocks of it on the way down with "no site within 48" each time, then spent ticks 13,229 to the cap at (-39, 3, 58), level with the lava and 41 blocks south of it, hunting.
- **Cause**: the mine task turned its heading into a direction only for exact quarters (90, 180, 270), anything else meaning south; the hunt set the heading to the lava's bearing, so every hunt's stairs and tunnels went south.
- **Fix**: the heading's nearest cardinal.
- **Still open**: a site needs a found walk to its landing, and a bot in rock above or beside a lava cave has none; the hunt now has to bring it to the lava's cave before any site is taken.

### 9. A site no walk reaches is never approached (races 43761772012 and before)
- **Read**: wall 8's rs-l00-b01, and others like it, came within 15 to 43 blocks of seen lava, inside the 48-block site radius, and took no site: every candidate's landing had no walk to it, the lava's cave being across rock and aquifer water.
- **Cause**: a site was taken only with a found route to its landing (wall 4's churn rule), and nothing brought the bot to a landing it could not walk to; the hunt walked or cut stairs toward the lava itself.
- **Fix**: with no routed site, the best unrouted candidate's landing is dug toward (stairs when it is below the feet, a level tunnel else), three cuts without coming nearer refusing it and giving its site up; once a walk reaches the landing, the site is taken.

### 10. A site with no station (race 43772126260, c425dbd: 0 of 112; 0 of 112 and 0 of 56 beside it)
- **What happened**: 24 bots began a cast and every one failed; casts now cut their whole room (66 to 75 ops) and failed at the first lava pour, `NoRefill` (27).
- **Read**: rs-l00-b01's site at (29, 43, 55) facing south took lava at (31, 40, 63), three below the room and eight in front of the frame across rock; from the room no walk came within a bucket's reach of it (eye to surface 6.2 against 4.4).
- **Cause**: a site checked its landing's route from the feet, never a way from the landing to its lava (clojurecraft's station: a stand beside the lava with a level way back).
- **Fix**: a site is taken only when a walk from its landing ends within a bucket's reach of its lava source.

### 11. A plateau explored whole, the water below never walked to (race 43779597490, 82de856: 0 of 112; 1 of 112 and 0 of 56 beside it)
- **What happened**: 21 of 112 never held water; wall 9 raised the casts begun to 36, all failing their first pour (`NoRefill` 37, wall 10's).
- **Read**: rs-l00-b00, b03 and b11 spent the race on a plateau at y 86, about 300 explore walks each, every target at y 86 (an explore aims at the feet's height), never going down off it.
- **Cause**: the scoop looked for water within 24 blocks only, then explored; water memory had kept further off was never walked to (clojurecraft walks to a remembered pool within 96).
- **Fix**: with no source within 24, a walk toward the nearest surface water memory kept within 96 (to within 12 of it), before any explore.

### 12. A refill aimed through rock (races 43898865876 and 43898882011, 56ccec2: 0 and 1 of 112)
- **What happened**: wall 10's station check took sites whose lava a walk reaches, yet 33 casts still failed, `NoRefill` 20 at the first pour.
- **Read**: rs-l00-b02 (lane 0 of 43898865876): its world rebuilt from the recording at the failure (a probe through `record::Folder`) showed the lava at (0, 57, 120) in memory, a found route to it, and the cast's own stand already within reach of its surface; the bot sent a hold and nothing more, the look from the stand to the lava crossing the stone at (1, 58, 121).
- **Cause**: the scoop aimed from wherever it first came within reach, never checking the ray; from the cast's stand it had no clear look, failed `NoSight` three times and the cast ended.
- **Fix**: a scoop chooses its stand: a standable cell within reach of the surface with a clear ray to it, nearest the feet with a found route (four tried), and aims only from a cell with a clear look.

### 13. No site beside lava three blocks off (race 43898865876, 56ccec2)
- **What happened**: 61 of 112 bots held water and never began a cast.
- **Read**: rs-l00-b01 and rs-l00-b04 spent most of the race three blocks from exposed lava (1,290 and 1,389 hunts) with no site. Rebuilding b04's world from its recording (a probe through `record::Folder` calling `site_choice`), every one of 140 candidate sites round the pool was refused: 74 for lava within a block of the footprint (diagonals counted), 52 for more than 8 blocks to place, and every one by the station check: no walk from any landing came within a bucket's reach, the routes running out five blocks short. The walker keeps off cells beside lava, and the pool lay in rock; the bot had dug its own way there.
- **Causes**: three rules stricter than the game: lava flows through a face, not a diagonal; 8 blocks against a kit of 16 cobblestone and a room's digs; a station had to be walkable, where clojurecraft digs its way to one.
- **Fixes**: a footprint cell is refused only for lava sharing a face; up to 16 blocks to place; a station is a walk within reach or lava a refill can dig to (no more than three below the landing, one above), and a refill with no walk to its pool tunnels toward the nearest source (four tunnels at most; a tunnel stops short of lava). On b04's world 12 sites now pass. The hunt's note carries the site report (the count each rule refused), for the next race's case files.

### 14. Landings only above the lava (race 43898865876, 56ccec2)
- **Read**: rs-l00-b01 spent the race at (-37, 1, 11), two below lava at (-34, 3, 12); its world rebuilt from the recording gave `site_choice` no landing at all (the site report: sources 3, landings 0).
- **Cause**: a landing was looked for one to three above the lava only, as if the frame had to stand over it; the lava need only be in a bucket's reach for the refills, and a room cell sharing a face with lava is refused anyway.
- **Fix**: landings from three below the lava to three above. On b01's world 12 sites pass (the landing where it stood); on b04's, 12 of 65 landings.

### 15. Hunting a pool too small to cast from (race 43898865876, 56ccec2)
- **Read**: rs-l00-b05, b07 and b11 spent the race at (191, -1, -99 to -105), hunting lava 2 blocks off: 1,193 to 1,207 hunts each. Their worlds rebuilt from the recordings: memory held four lava sources within 48 (a pool of three and one at y -45); every site was refused as spent (164 each): a cast spends one source a cell, ten in all.
- **Cause**: the hunt headed for the nearest lava seen within 160, though that lava, within 48, had been through the site choice and refused; the bot never left it.
- **Fixes**: the hunt heads only for lava further than 48 (with none, down toward the lava sea); a pool's sources are counted within 16 of the site's lava (from 8), and a refill takes any of them.

### 16. A pool above the room, its face out of sight (lava races 43938628723 and 43938666441, db30a81: 5 and 5 of 112)
- **What happened**: from the `lava` checkpoint (bots placed at twelve known cast landings), 107 of 112 began a cast within 30 minutes and 10 of 224 lit, the best yet; the casts that failed ended `NoRefill` (93) and `Op` (52).
- **Read**: rs-l00-b12, at its site (23, 37, 55), failed its first refill at tick 1,277 standing in its own tunnel at (30, 38, 61); its world rebuilt from the recording showed the lava a lake at y 40 in a cave above, dirt between it and the tunnel, its only open face the top. The scoop found no stand: it aimed at the surface only, and no cell in reach saw it.
- **Causes**: the scoop aimed at one point; the cast's room was always cut at height 0, with no access steps up to a pool above (clojurecraft's `h`, the lava's height over the room's floor, which its access steps climb to).
- **Fixes**: the scoop aims at the first of the source's faces in reach with a clear ray (the surface, the sides, the underside), stands looked for from four below to three above; a site's plan carries `h` (the lava one over the top step's feet, at most 6) and the room cuts its access steps up to it.

### 18. Refills chase whatever lava is near, not a fixed station (lava races 43945858654: 3 of 112 on 43a344a)
- **What happened**: walls 10, 12, 13 and 16 each fixed one recorded refill and the rate barely moved; `NoRefill` stays the largest ending of a started cast.
- **Cause**: the refill took the nearest sources and looked for any stand or tunnel to them, a new shape every time. clojurecraft refills from a station: a stand at a height `h` over the room's floor (1 to 6), four to nine in front of the frame and up to eight aside of the landing's column, a firm floor, dry round, sources below the feet and in reach; reached by the room's access steps to `h` and a level way at that height, walked or dug.
- **Fix**: `plan::station` ports `station-among`, `reach-sources`, `stance-block` and the sight dig of `scoop-at-station`; the cast's lava refill goes to the station first (walk to the stand; off its height, to the landing at it; at its height, dig the first block of the straight way), scoops from there through a clear face, else digs the block on the line to a source below the feet. With no station it falls back to the old refill. Strict no X-ray: unseen cells count as rock.

### 19. The checkpoint crowded bots onto one pool (lava races 43938628723, 43938666441, 43945858654, 43945845334)
- **What happened**: rs-l00-b08 (race 43945845334) failed `dig :room` after 15 ops with no dig started; `why` at the failure showed two other players within 2.3 blocks and a dozen dropped items round it.
- **Cause**: the `lava` start's twelve places sat round about five pools (three within 3 blocks at (169, 60, -80)), and bot i of lane L took place (14 L + i) mod 12: on each lane's one server three or four bots cast the same pool, digging each other's cells and taking each other's drops. The `kit` start spreads bots 128 apart and never had this.
- **Fix**: fourteen places from `kit` race casts, their lava 32 blocks apart or more; bot i of every lane takes place i (one bot per pool per server, the same pools in every lane, so lanes pair).

### 20. The station's way dug the room's floor (station races 43955186750 and 43956811013, 37d3b8c: 7 and 7 of 112)
- **What happened**: the station refill lifted the pair to 14 of 224, but `place :floor` became the top failure in lane 0 (6 of 14), each cast going back to a floor op tens or hundreds of times (rs-l00-b02: 360 regressions).
- **Read**: rs-l00-b06 at its site (47, 39, 61) placed cobblestone on (49, 40, 64) at event 196180; the world rebuilt at 196300 shows it at (49, 41, 64), and at 196304 the bot, standing at (48, 41, 64), began to dig it: the refill's straight way to its stand ran through the floor just placed. Place, dig, place, a dozen times.
- **Cause**: `stance-block` in clojurecraft refuses the frame's rock and every cell the build places (`placed?`); the port kept only the frame's rock.
- **Fix**: the way and the sight dig refuse every cell a place op of the plan names.

### 21. Sites with no station (uncrowded lava races 43960034706 and 43961389234, 0347960: 20 and 18 of 112; wall-20 race 43964603501: 18)
- **What happened**: one bot per pool lifted the pair to 38 of 224, but eight of the fourteen places lit 0 of 8, most failing `NoRefill` on the first lava pour (about 70 ops in).
- **Read**: rs-l00-b10's site (30, 43, 55) faced lava at (35, 40, 61), three under the landing; a station stands one to six over the room's floor, its eye five or more over the lava, out of reach.
- **Cause**: the port chooses a site by its landing near the lava and asks for a station after; clojurecraft's `choose-site` builds the site from a stand beside the cell over a source, the origin seven behind it and h (1, 3, 6) under it.
- **Tried and reverted** (6069b5e, race 43966598717: 7 of 112, against 18 and 20 before it): the stand-first site cut the room under the stand, in the rock; `NoRefill` fell to 12, but `dig :landing` failed after 0 or 1 ops (its stand in solid rock, and the executor only walks to a stand) and bots tunnelled toward landings no walk reached; places that lit 3 to 5 of 8 lit none. clojurecraft's executor digs its way to each stand (`toward-stand`, `stance-block`); the stand-first site needs that port with it, not alone.

### 22. An op with no walk to it fails at once (lava races, every commit since 484b257)
- **What happened**: `dig :landing` and `dig :room` fail after 0 to 3 ops within seconds of the cast's start (rs-l00-b03 of race 43945845334: 65 ticks; three of fourteen in lane 0 of 43960034706), and the site is given up.
- **Cause**: the executor's approach walks to an op's stand and, with no route, spends a try at once: three ticks, three tries, the site given up. clojurecraft's `toward-stand` digs the straight way toward the stand when no walk reaches it (`stance-block`).
- **Fix**: with no walk to an op, the first block of the straight way at the feet's height toward its column is dug when it is in reach, never the frame's rock or a cell the plan places, at most 12 such digs per op; then the walk is tried again.

### 23. Landings over lava out of reach (wall-22 pair 43973361819 and 43973376039, c29153b: 26 and 27 of 112)
- **What happened**: places 1, 2, 7, 8, 9, 10 and 12 lit 0 of 16, most failing `NoRefill` at the first lava pour; their landings one to three over the lava.
- **Tried and reverted** (914bf43, pair 43978974380 and 43978988790: 2 and 3 of 112 against 26 and 27): landings at most one over the lava. The places that lit 4 and 5 of 8 (4, 5, 6) lit none: their good sites are two or three over the lava, refilled from the room some way the station model here does not describe. The read was wrong; the next step is to read a lit cast's refills, not to reason about reach.

### 24. Lava behind the frame (wall-22 pair, c29153b; the lit casts lit their second site)
- **What happened**: in lane 0 of 43973361819 the casts that lit (rs-l00-b04) failed `NoRefill` at their first site too and lit a second one; rs-l00-b01 failed `NoRefill` after 100 ops and found no other site.
- **Read**: rs-l00-b01's world rebuilt at its failure (event 95770): the frame at (-29, -53, -33) faces south, its room at z -32 and on, and the lava sea's edge at z -34 to -37, behind the frame's plane. Every line from the room to it crosses the frame's rock (`kept`), so no stand sees it and no dig may open it. The old test (lava farther than the landing from one frame cell) passed it.
- **Fixes**: the lava must lie in front of the frame's plane (its dz in the frame's terms one or more); and with no station, the refill digs the first block on the eye's line to a source in reach (under the feet, not kept or placed, no lava beside it at its height or over it, at most 24 a cast).

### 25. A lava lake under the room's floor (wall-24 pair 44021913774 and 44021924959, 98e43ab: 28 and 24 of 112)
- **Read**: rs-l00-b10's room floor (y 42) is the roof of a cave over a lava lake at y 40, in reach but seen only through the plan's own floor.
- **Tried and reverted** (b4b1969, 3d58a40, 0f6ba8d: pairs 44 of 224, 49 and 46, against 52 on 98e43ab): a refill dug a hole in the floor beside the feet over the lake, then tried it before any route, then kept it open until the bucket filled. Place 10 lit none in every pair; the read of why the scoop through the hole still failed is the next step on it, not another patch.

### 26. A scoop "routed" where it could not see (trail pair 44041169156 and 44041182079, 71a7e20: 23 and 33 of 112)
- **What happened**: with the refill's last steps in the failure note, lane 0's twelve `NoRefill` casts read: seven `scoop X routed` then `failed NoSight` one tick later, four times in four ticks on one source (rs-l00-b01, b02, b07, b09, b10, b00 and b13's twin); three `NoScoop` after 200 ticks each (b04, b12, b13); three water refills with no water route (b00, b05, b06).
- **Cause**: the scoop's fallback walk goes to within 3.5 of the source; a route `Found` counted with no check of sight (only a partial one had to see it), so a bot already that near got an empty route, reached `Aim`, saw nothing and failed, and the refill took the same nearest source again.
- **Fix**: a route counts only when its end (or the eye, when the bot is already there) sees a face of the source; the refill then tries the next source, the tunnel and the sight dig.

### 27. The scoop's ray grazed a corner (trail pair, 71a7e20: `NoScoop` in rs-l00-b04, b12, b13)
- **What happened**: three of lane 0's twelve `NoRefill` casts failed `NoScoop`: the bucket used eight times or more, 200 ticks a try, nothing filled.
- **Read**: rs-l00-b04 rebuilt at its use at event 117544: the held item a bucket, the target (-114, -55, 32) lava at level 0, the look sent (yaw 90.08, pitch 66.3) exactly at the aim [-113.5, -54.15, 32.5], and forty samples along the line all air. Traversed exactly, the line enters (-113, -54, 32), deepslate, crossing x -113 at y -53.02, two hundredths inside its top corner; the server's ray takes the first block it touches.
- **Cause**: `clear_ray` sampled forty points and stepped past the corner (clojurecraft issue #225, the same).
- **Fix**: `scoop::ray_cells`, exact voxel traversal (Amanatides and Woo); `clear_ray` asks every cell the line enters. On b04's world the aim moves to a face the exact line reaches.
- **Raced** (610fd87, pair 44052153072 and 44052157794: 16 and 16 of 112, against 29 and 27 on 6d82b7b; nothing else in the bot changed): worse by half. The first failure of each of 96 bots, from the case notes: 40 `NoRefill` at `:pour :lava` ending "no route, 4 tunnels"; 15 `NoRefill` at `:pour :water` ending "no route to None"; 12 `Op` at `:place :floor`; 12 `Op` at `:pour :lava` after a scoop routed; 8 `Op` at `:dig :room`; 3 `NoSight`. The exact line is right in the one case read (the server refused that use), yet it costs portals: either it refuses lines the server takes, or an honest "no route" is handled worse than a lucky try. Not known which.
- **Now**: the sampled line decides again (the measure that lit more); the exact line is computed beside it at every use, and a cast counts its uses four ways (filled or empty, exact clear or blocked) in its notes (`Scoop::uses`). "Filled, exact blocked" counts the exact line refusing what the server took; "empty, exact clear" counts it passing what the server refused. The next race's counts decide.
- **Counted** (race 44165259286, 1d4eae0, 112 bots; the filled count fixed in 1d4eae0, the cast had dropped its refill before the scoop saw the fill): filled with the exact line clear 435, filled with it blocked 1; empty with it clear 0, empty with it blocked 168. The exact line agrees with the server in 603 of 604 uses, and every one of the 168 wasted uses (200 ticks each, a try spent) was one it would have refused. So the line is right; what lost portals at 610fd87 is what the refill does once no face of the nearest sources has an exact line, not the line. Next: read a refill at 610fd87 that the exact line sent to "no route" where the sampled one went on to fill.

### 28. The water is poured and never comes back (wall-27 pair, 610fd87: 27 casts in race 44052157794)
- **What happened**: 27 casts failed `NoRefill` at `:pour :water` with "no route to None, 0 tunnels" (15 of them a bot's first failure), most at places 4, 5 and 6, the places that light.
- **Read**: rs-l05-b05 (place 5): `cast at (168, 59, -79)`, failed at tick 1,913 after 74 ops at the first cell's water pour; `items lost: water_bucket 1`, `items gained: bucket 1`: it started with a water bucket and an empty one and ended with two empty ones. Its refill trail: a station walk and dig failed, two lava scoops routed and filled, then the water refill found no source. From tick 1,933 to the cap it walked toward water pools at y 11 to 62 and failed `NoPath` 364 times.
- **Known**: the plan's water pour is done when its cell is obsidian, and its scoop back is done when no water source lies in one cell (`Done::NoWaterSource(over)`). Nothing asks that the bot holds its water again, so a pour whose water lands elsewhere, or a scoop back that misses, loses the bucket for the rest of the run; at a cast's depth memory holds no surface water to refill from.
- **Read** (probe 44159270217, b05's acts exported on a runner): the whole cast made three bucket uses: held slot 1 (the empty bucket) and used it twice, scooping lava and pouring it, then held slot 2 (the water bucket) and poured. The water pour's op is done only when its hole is obsidian; no obsidian formed, so the first op not done stayed the water pour and asked for a full water bucket. The scoop back, which would have taken that water up again, comes after the pour in the plan and was never reached. The refill's water sources are memory's surface pools within 24 (`scoop::sources`); the water the cast had just poured, a block from its feet, was not among them.
- **Cause**: the cast's water is spent, not kept: nothing gets back the water a pour that failed left in the room.
- **Fix**: a water refill takes first the water sources seen within 7 of the frame's origin (`cast::own_water`), nearest the origin first, then memory's surface pools; the trail notes "own water N" when there are any, so the next race counts its firings. Why the pour made no obsidian is a separate question, not yet read.

### 29. Defend walks into lava (wall-27 pair, 610fd87: 19 lava and fire deaths in 224 runs)
- **What happened**: lava and fire killed 19 of 224 bots, most at places 3 and 7, beside the lava sea at y -55.
- **Read**: in the case notes, 15 of the 19 had a `defend` note within 300 ticks before their `escape-lava` (creepers fled, skeletons and silverfish fought); three more followed a `cast failed: Exposed` at the water pour. Across both races there were 449 defends: 345 `Fight`, 104 `Retreat`.
- **Cause**: of the defend's moves, a retreat with a route walks it (the walker keeps off cells beside lava); but a fight's close-in and a retreat with no route are a raw step forward along a yaw, asking nothing of the cell ahead.
- **Fix**: both raw steps ask `safe_ahead` first: no lava seen in the cell ahead at the feet or the head, nor under it down to 4, and a seen floor within 4 (an unseen cell counts as rock). A held step stands facing the foe (a fight) or away from it (a retreat), and the `defended` note counts the steps held back, so the next race counts its firings.
- **Raced** (1d4eae0, race 44165259286: 31 of 112 lit): 1,410 steps held back in 13 defends; lava and fire deaths 1, against 6 to 11 a race before. Wall 28's own water fired in 2 bots; water `NoRefill` 9, against 12 and 27.

### 30. A place refused under the bot's own box (b358407 pair: 26 casts failed `:place :floor`)
- **What happened**: 26 casts in race 44159281696 failed `Op` at `:place :floor` with no regressions; the same cells failed in lane after lane ((-18, -50, -67) in eight lanes).
- **Read** (probes 44170721317 and 44171254501 on rs-l00-b06's recording): the cast's place clicked the east face of (-19, -50, -67) four times to put cobblestone in (-18, -50, -67), and nothing appeared. `why` at three of the clicks: the player at (-17.53, -51.00, -65.86), its box z -66.16 to -65.56: 0.16 into the target cell on z, with its feet in (-18, -51, -66). The server refuses a block whose cell a player's box overlaps.
- **Cause**: the place op stepped out of the cell only when the feet's cell or the head's was the target; a box overhanging a neighbouring cell was not checked.
- **Fix**: `cast::overlaps` (the box from `physics::aabb` against the cell, strictly) decides the step out; a unit test holds b06's numbers.

### 31. No cobblestone left, a pack of cobbled deepslate (wall-30 pair, 288fd31: 19 to 24 casts a race failed `:place :floor`)
- **What happened**: after wall 30, 24 and 19 casts still failed `Op` at `:place :floor`, now at cells near the lava sea's level (x 160 to 380, y -51 to -55), most by bots at place 7 working site after site.
- **Read** (probes 44178202679 and 44178625037, rs-l05-b07 of race 44172568506): from the cast at tick 6,631 to its failure at 6,930 at (313, -54, -31) the bot dug and never clicked a face: the place op failed without a use. Its case: `items lost: cobblestone 16`, `items gained: cobbled_deepslate 55`.
- **Cause**: the place op held only the op's block, cobblestone (`Hold::Missing` fails the op at once). Below y 0 a bot digs deepslate and carries cobbled deepslate; once the kit's 16 cobblestone were laid, every place op failed.
- **Fix**: `cast::FILLERS`, clojurecraft's `filler/fillers` (cobblestone, cobbled deepslate, andesite, diorite, granite, tuff, dirt, netherrack, soul soil, blackstone, basalt; never gravel or sand); a place lays the op's block when held, else the first filler held. Not ported: `stone-kept`, the stone kept back for a spare pickaxe.

### 32. A stand the bot already stands in, seen from the cell's middle (wall-31 pair, 8d7a873: 44 and 39 of 112)
- **What happened**: wall 31 lifted the pair to 83 of 224 (37.1%) and floor failures fell to 2 a race; the top failure is now lava `NoRefill` ending `scoop failed NoSight`, 40 and 43 casts a race.
- **Read** (case notes, race 44184615869): `scoop (300, -55, 5) routed`, `failed NoSight` two ticks later, four times on the same source, and the same at (-68, -55, 129) and (-67, -55, 132). A scoop fails `NoSight` only at its aim with no face in sight from the eye, which it reaches with no aim only from a walk to a `Goal::Block` stand that ended at once: the bot was already in the stand's cell.
- **Cause**: `stands` judges a stand's sight from its cell's middle; `route_now` took a stand the bot already stood in without asking its real eye, which sits off the middle and saw no face. Every try chose the same stand.
- **Fix**: a stand whose cell holds the feet counts only when `aim_at_use` from the real eye finds a face; else the scoop routes to the next stand, or the refill tries the next source.
- **Raced** (88c67e8, pair 44220497092 and 44222358936: 55 and 45 of 112, 100 of 224, 44.6%, against 83 of 224 on 8d7a873): `NoSight` refills 11 and 9 a race against 40 and 43. Places 5 and 9 lit 8 of 8; 2, 8 and 10 lit none. The top failure is now lava `NoRefill` ending "no route, 4 tunnels" (31 a race).

### 33. From `kit`, the cast is rarely reached and rarely lit (official race 44226112675, 88c67e8: 1 of 56)
- **What happened**: the done-condition race, 4 lanes of 14 from `kit`, cap 72,000, lit 1 of 56 (1.8%), while the same commit lit 100 of 224 (44.6%) from the `lava` checkpoint.
- **Read** (case notes, `results diagnose`): 36 of 56 scooped their water; 22 began a cast; 53 casts were begun in all and about 29 failed, the rest stalled or capped; the whole race spent its time in 1,510 `explore to`, 771 `relocate`, 781 `hunt lava`, 644 `walk toward lava` and 743 `walk toward water` notes. Of the 22 that cast, one lit.
- **Known**: two walls stand before the cast from `kit` (water: 20 bots never scooped; the lava hunt: 14 more never began a cast), and the cast itself fails at the sites kit bots find (45 casts at eleven distinct pools, most at the lava sea at y -54 and -55, like the `lava` checkpoint's places 2, 8 and 10 that light none).
- **Next**: the `kitlava` start, the kit with its water at those eleven pools (11 bots a lane, one a pool), measures the cast where kit bots cast; the water and the hunt get checkpoints of their own after.

### 34. A bucket used on a line the exact one calls blocked (kitlava race 44236062785, 063e06e: 14 of 88)
- **What happened**: from the `kitlava` start (the cast at the pools kit bots choose) the race lit 14 of 88; 32 casts failed `NoRefill` ending `scoop failed NoScoop`.
- **Read** (the cast notes' use counts, summed over the race): filled with the exact line clear 317, filled with it blocked 0; empty with it clear 0, empty with it blocked 342. Every wasted use (200 ticks and a try each) was on a face only the sampled line passed; `aim_at_use` fell back to it when no face had an exact line.
- **Fix**: the bucket is used only on a face the exact line reaches (`aim_at_use` takes no sampled fallback, and the aim stage fails `NoSight` without one); routes and stands keep the sampled line (wall 27). With no exact face the refill moves on at once instead of spending four 200-tick tries.

### 35. The bot dug the floor it stood on (kitlava pair 44236062785 and 44236067105, 063e06e: place 0 died in lava in 13 of 16 lanes)
- **What happened**: place 0, (35, -52, -77), the site kit bots chose most often, began a cast at (28, -52, -64) facing north and died in lava about 1,900 ticks in, in 13 of 16 lanes.
- **Read** (probe 44239181756 on rs-l00-b00): the bot cut a two-high way along x 26 at y -51 and -52 from z -67 to -72; at event 119456 it stood at (26.50, -52.00, -71.44) on deepslate, then dug (26, -53, -72), the block under its feet; 250 events later it was at y -53.15 standing on air, and at y -55 with 16 health, in the lava sea's cave.
- **Cause**: neither the sight dig (a block on the line to a source below the feet) nor the cast's dig op asked whether the body stands on the block; the probe does not show which sent it.
- **Fix**: `station::sight_block` never takes a block one under the feet whose column the box overlaps, and the cast's dig op steps off a block it stands on before digging it (`cast::standing_on`); a unit test holds the probe's numbers.
- **Raced** (wall 34, 8693d2c, kitlava pair 44239909331 and 44239913230: 26 and 24 of 88, 50 of 176 against 28): `NoScoop` refills gone from the table. Wall 35 (a25046d, pair 44243676244 and 44243679788: 20 and 28 of 88, 48 of 176): lava deaths 0 against 15, the lit count level. Places 0, 1, 4, 5 and 7 light none, mostly at lava refills ending `NoSight` or "no route".

### 36. "In reach" judged by one line, the use by another (wall-35 pair, a25046d: places 0, 1, 4, 5 and 7 light none)
- **Read** (the station trail, race 44243676244, rs-l00-b05 at place 5): `station at (-69, -53, -95) h 1 with 8 sources, feet (-70, -53, -96)`, then `scoop (-70, -55, -95) failed NoSight`, three ticks apart, again and again. The refill found a source "already in reach with a clear face" by the sampled line (`aim_for`); the scoop, since wall 34, uses only a face the exact line reaches, found none and failed; the refill chose the same source.
- **Cause**: four checks that mean "a use from here would fill" (the station's two in-reach checks, and the end of a scoop's route) still asked the sampled line after the use itself asked the exact one.
- **Fix**: all four ask `aim_at_use`; only `stands`, which finds places to walk to, keeps the sampled line (wall 27).
- **Raced** (7b42f9d, kitlava pair 44248178757 and 44248183443: 22 and 23 of 88, 45 of 176 against 48): no change; `NoSight` still 25 and 29 a race.

### 37. A stand reached off its middle (wall-36 pair: `NoSight` 25 and 29 a race)
- **Read** (race 44248183443's trails): `scoop (-37, -55, -94) routed`, `failed NoSight` 14 ticks later, the same again twice; `scoop (-18, -56, -86) routed`, `failed NoSight` 4 to 11 ticks later, four times. The route's end is judged from the stand cell's middle (and since wall 36 by the exact line); the walk ends with the body off the middle, the real eye sees no face, and the next route picks the same stand.
- **Fix**: when a walk to a stand ends and the eye has no face, the bot steps to the cell's middle, sneaking so no edge is walked off, for up to 20 ticks, then aims (`CENTRE_NEAR` 0.1, `CENTRE_TICKS` 20).
