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
