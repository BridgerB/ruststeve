# Issue 20, E7: never stare at an enderman in the End

**Status: open.** Agent E7 (gaze). Owner of `crates/plan/src/look.rs` and the gaze paths in `crates/plan/src/endermen.rs` (`keep_look`, `count_near_misses`). Index: [`end.md`](end.md).

## Needs
Zero stares in the End, by discipline: no look the bot sends has its ray inside the stare cone of any enderman's eyes within 64, and none within the race's near-miss band round it. Bridger: "I can avoid it when I play, so can you." On the bow line a carved pumpkin replaces this (`look::pumpkin_worn` already exempts every look); on the pillar line it is discipline.

## Done (world terms)
A race's End row reads `gaze near 0 (stares 0)` for every lane, and no ledger carries `near miss: enderman N stared at`, with the outcome of no stage dropped.

## The rule (26.1.2 jar, javap in probe 45472254333)
- `EnderMan.isBeingStaredBy(Player)`: false if `LivingEntity.PLAYER_NOT_WEARING_DISGUISE_ITEM` fails (a carved pumpkin on the head); else `isLookingAtMe(player, 0.025, true, false, getEyeY())`.
- `LivingEntity.isLookingAtMe(target, tolerance, scaleByDistance, visualClip, ys...)`: `v` = the player's view vector, normalized; for the one y (the enderman's eye, feet + 2.55): `d` = (enderman x - player x, y - player eye y, enderman z - player z); true when `v · d/|d| > 1 - 0.025/|d|` and `player.hasLineOfSight(enderman, COLLIDER, NONE, y)` (block collision shapes stop it, fluids do not). So the cone's half-angle is about `sqrt(0.05/|d|)`: 6.4 degrees at 4 blocks, 3.2 at 16, 1.6 at 64.
- `EndermanLookForPlayerGoal`: a `NearestAttackableTargetGoal` for players within the follow range, 64, selected by `isBeingStaredBy`. `start()` sets `aggroTime = adjustedTickDelay(5)` and `setBeingStaredAt()` (metadata 18, the stared-at flag, goes true at once). While pending, `canContinueToUse` re-tests the stare each run and drops the target if it fails; when `aggroTime` runs out the player becomes the target (anger). So a look held in the cone for about 5 ticks angers; one broken sooner sets only the flag.
- Our test of it: `look::stares` (exact, the eye under the controls, a seen full block stops the line). The race's count: `observe::gazes` (the look sent last, the standing eye 1.62, no line test; a stare inside 1 tolerance, a near miss inside `NEAR_MISS` = 4).

## The stares (races 45994686575 at 3ac92df, 45988967677)
From the ledgers (per second): 45994686575 lane 4 had 5 cone entries and the run's only stared-at flag (enderman 2698 at 42540, pitch 90, during a clutch drop from a 43-high pillar); lane 6 3; lane 0 2; lanes 2 and 3 one each; 45988967677 lane 1 5. Only 2 of the 17 were ever "look kept" by the filter (both at pitch 90: lane 2 at 1800, an enderman 13.4 under the look straight down; lane 4 at 23740, 19.4 off during a clutch fall): the filter judged the others safe.

Per tick, by the new `gaze` replay (`cargo run -p replay --bin gaze -- <rsr> --force`, a70f601), probe 46040113311 over all 16 lanes of both races: 17 looks into the race's cone, the same 17 the ledgers count; **every one a look down** (pitch 90, one 86.4) at an enderman 10 to 32 off and 11 to 29 blocks lower; in 15 of them the bot's own ground or pillar stopped the line (the server's test fails there too), 2 had the line clear (real stares by the jar), and the one stared-at flag (lane 4, 42540) came between ticks of a fall at 2 a tick, which no tick's look shows. `look::stares` said `None` at every one: the filter saw no stare.

## Why the filter let them through (structural)
The first cause by count is 3 (15 of 17: a look down whose line the bot's own ground stopped), then 1 (the flag and the clutch stare: a fall), then 2.
1. **The eye it judged from was not the eye the server judged.** `keep_off` tested the feet before the tick's move; the look goes out with the feet after it. On a clutch drop the feet fall 2 a tick, and an enderman's eyes 20 off move 6 degrees in the look's frame each tick: the cone is 3.
2. **No margin.** The enderman's place is the last one sent (a walk of up to 0.3 a tick, a teleport of up to 32), and the pose (crouching or not) changes a tick late, but the filter let a look go that was a hair outside the jar's cone.
3. **A line of sight trusted in the open.** A seen full block on the line excused a look; in the End the only such blocks are the bot's own pillar and the island's edges, and the race counts a stare with no line test.
4. **The bend was coarse.** A kept look jumped to 90, 60, 30 ... degrees: a look down over an enderman at a pillar's foot went to 60, a clutch's bucket away from its landing.

## The fix (E7, this branch)
- `look::GUARD` = 4 tolerances (the race's own near miss, twice the cone's angle): a look is kept outside it, not just outside the cone.
- `look::eyes`: judged from six eyes, standing and crouching, at the feet now, after this tick's move and after the next (`pos + vel`, `pos + 2 vel`).
- In the End no line of sight excuses a look (`look::watchers`); elsewhere a seen full block still does (a cave wall), tested once per enderman and eye, not per pitch.
- `look::safe_pitch`: the pitch nearest the one asked for that clears the guard, every degree, the lower first on a tie; the yaw (a walk's heading) never changes.
- `keep_off_why` says why a look was kept (`Kept::Stare`: inside the jar's cone; `Kept::Guard`: only inside the guard), and `keep_look`'s note says it: `look kept off enderman N (pitch P would stare at | come within the guard of it D off; sent Q; n looks kept in the run, m by the guard)`.
- **A clutch's pour is never bent** (`look::pouring`, `look::keep_off_act`): the tick a water bucket is used while falling, the look goes out as asked, and a pour whose look comes near an enderman is noted `clutch pour sent unbent near enderman N` (the water must land under the feet; the fall is the bigger danger). The aim ticks before it bend only to the nearest pitch clear of every enderman from every eye, which stays steeply down. Race 46038776204 (117843e, before this fix; E9's read): lane 5 at 34,040 the old bend took a clutch look from 90 to 60 and that stared at enderman 2504 18 off, the race's one stared-at flag in 7 ledgers; lanes 4 (10,600) and 6 (1,260) bent clutch looks to 60 too.
- Every look still leaves the planner through `Plan::step` → `end_gaze` (E3) → `keep_look` (E7); every act's look (`walk::look_at` in spikes.rs, the dragon's `pitch: Some(90.0)`, the bucket's `Act::UseItem`, which carries the controls' look) passes through it. No call site in another agent's file changed.

## Per act (probe 46040113311)

| Act (whose look) | Stares | Where (race lane tick) | Line clear |
|---|---|---|---|
| Dig with no look asked: the stock and surface pits (E3's `end_look(None)`, straight down) | 12 | 45988967677 l1 9820, 10340, 10560, 15080, 15120; 45994686575 l4 24140, 24200, 24440 (two); l6 8200, 8220, 8520 | none |
| On a pillar: dig down, the wait while falling, up high (E4's spikes, straight down) | 3 | 45994686575 l0 11080, 21460; l3 7960 | none |
| Clutch drop, the bucket at the landing (E4, pitch 86 to 90 while falling) | 1, and the run's one stared-at flag | 45994686575 l4 23740 (a stare); 42540 (the flag, between ticks of a fall from 67.8) | yes |
| Walk on the island, jumping (look down by default) | 1 | 45994686575 l2 1800 (rising 1.1 a tick) | yes |
| Strike, dragon look, dodge heading, eat | 0 | | |

So the stares are the discipline's own default: "look down" is not safe on high ground over endermen lower down, and a look down passes through the cone of every enderman below the feet. The fix keeps the look down but bends it (to the nearest pitch outside the guard); in the open End no line of sight excuses it.

## Tests
`crates/plan/tests/gaze.rs`: a strike at a crystal with an enderman behind it, a dragon look past an enderman's eyes, a dodge heading at one, a look down from a pillar over one, a clutch look while falling 2 a tick (outside the cone from the feet now, inside after the move), an eat with the held look on one, a wall outside the End and in it, a sweep of 24 yaws by 19 pitches round six endermen at four ranges, the whole planner in the End over an enderman at a pillar's foot, noted; and lane 5's clutch: the aim bent a little off both endermen, the pour sent straight down and noted.
