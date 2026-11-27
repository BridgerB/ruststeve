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

Per tick, by the new `gaze` replay (`cargo run -p replay --bin gaze -- <rsr> --force`), probe 46040113311: see "Per act" below once it is read.

## Why the filter let them through (structural)
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
- Every look still leaves the planner through `Plan::step` → `end_gaze` (E3) → `keep_look` (E7); every act's look (`walk::look_at` in spikes.rs, the dragon's `pitch: Some(90.0)`, the bucket's `Act::UseItem`, which carries the controls' look) passes through it. No call site in another agent's file changed.

## Per act
(filled from probe 46040113311)

## Tests
`crates/plan/tests/gaze.rs`: a strike at a crystal with an enderman behind it, a dragon look past an enderman's eyes, a dodge heading at one, a look down from a pillar over one, a clutch look while falling 2 a tick (outside the cone from the feet now, inside after the move), an eat with the held look on one, a wall outside the End and in it, a sweep of 24 yaws by 19 pitches round six endermen at four ranges, and the whole planner in the End over an enderman at a pillar's foot, noted.
