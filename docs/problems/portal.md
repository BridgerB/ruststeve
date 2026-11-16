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
