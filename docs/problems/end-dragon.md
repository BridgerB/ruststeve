# Issue 20g: the dragon and the win (E5)

**Status: first dev race read (2026-11-26): head strikes land 6.0 from the head stand; walls 1 to 3 fixed in the next push.**

## Needs
With every crystal gone, one bot kills the dragon and receives the credits: `w.won` (`win_game` after the exit portal), answered with the respawn, back in the overworld. Stage `win` (E1's) reads it.

## Done (world terms)
A case file in the End shows, in order: `dragon fight`, `dragon strike head` notes with the dragon's health falling, `dragon dead`, `dragon: exit ...`, `credits`, `respawn after the credits (1)`, `back from the End in minecraft:overworld`.

## What is built (`crates/plan/src/dragon.rs`)
- **The model, pure:** the phase from entity data 16 (`Phase`, ids 0 to 10 as E1 read them from the jar); perched = within 6 of the fountain (0.5, 0.5), under y 70, held 10 ticks, in a sitting phase once one is said (E1: the perch is about y 67); the head point at the body plus 6.5 along the yaw (sin, −cos) and 0.5 under the feet (the head part's 1 by 1 box at −1); breath clouds (`area_effect_cloud`, `dragon_fireball`) with their radius from data 8 plus a 1.5 margin.
- **The yaw:** nothing decoded it before. `move_entity_rot`, `move_entity_pos_rot` and `teleport_entity` now carry it into `world::Entity::yaw` (protocol and world, approved by the orchestrator). `add_entity` and `entity_position_sync` do not yet: a dragon first seen sitting still has no yaw until it turns, and the fight holds ("the dragon's yaw not yet told").
- **The choice each tick** (`decide`, clojurecraft's `toward-dragon` order): dead → into the exit portal (a seen `end_portal` cell near the fountain; straight in from the rim); a breath's reach on the feet → the nearest ring cell 4 to 12 out, out of every breath, not before the head, the way kept 2.5 from the head and 3.0 from the neck; the dragon's own blow (a hit whose cause and direct are the dragon) → out to max(8, feet + 4) for 100 ticks; flying → E3's bunker (`Plan.bunker`; with none, a ring cell 12 out), and in a lull (no breath, not grounded, no hostile in 8) eat while hurt with the bar under 20; grounded → under the perch line, out to the ring 12 out; sat → the head stand and the strike; landing or no head stand → out to the ring 12 off (wall 3).
- **The head stand** (after E6's e05, run 45866776019): a ground cell 2.0 beyond the head part's box along the facing, out of the head's blow box (its box grown by 1) and the neck's (3 by 3 grown by 1), out of every breath, the eye within 2.75 of the head's box; held while it still reaches 3.0. A way to it that would pass under the head goes round it first, 5 out to the side (`around`, `way_clear`).
- **The stand behind** (`stand_behind`, `keep_stand`; no longer chosen by `decide` after wall 3, kept for the model): a cell of the basin's floor (1.4 to 2.6 from the fountain) at least 1 behind the body along its yaw and out of every breath, the nearest the feet; the held stand kept with a slack of 1 (`keep_stand`). From outside the basin a way that crosses before the head goes round it first (`around`).
- **The strike:** `Act::Attack` on the head part, the dragon's id + 1, while sat, the feet out of the blow boxes and the eye within 3.0 of the head's box. E6's e05 landed 6.0 a strike this way, never hurt; the dragon's own id and the neck (id + 2) take only 2.5. About 9 strikes end a perch (50, E1); 34 in all.
- **The credits:** `Plan::dragon_watch` runs first every tick (before the body check, since the respawn clears it): phase-change notes, the perch count, the death (status 3, or the boss bar gone after one was shown), and on `w.won` the "credits" note and `Act::Respawn`, asked again after 200 server ticks (at most 5), every other row held until the overworld.
- **Guards elsewhere (fenced E5):** `defend::NEVER_FOE` holds the dragon (the defend's chase took clojurecraft's bot into the head); the eat always-row waits while the dragon sits within reach (`dragon_holds_meal`); the fight eats in its own lulls.

## The perch line (why one blow)
- A head or neck touch is 10 before armour (E1, `EnderDragon.hurt`). Through iron with boots (armour 15, no toughness) it is 10 × (1 − 10/25) = **6.0**; without boots (armour 13) 6.8, clojurecraft's measured number.
- Danger 8 plus two blows is 20.0: exactly full health, so a two-blow line never lets the bot strike. `perch_line` therefore takes **danger plus one blow, 14**.
- The head is struck from where its blow does not reach: E6 measured a stand 2.0 off the head's box landing full strikes, unhurt, outside the blow box. So the line of 14 is workable: under it, out to the ring; over it, the head stand.

## Risks (unproven until a race)
- The scanning dragon turns its head toward the bot and the attacking one breathes flame at the head (radius 5): the head stand is inside that cloud's reach, so each breath sends the bot out and back.
- The wings: not sitting and at hurtTime 0 they push 4 out and hurt 5 within their box inflated 4. At takeoff the bot is on its stand in the basin; the fight leaves for the ring only once the phase says takeoff.
- The call site: E3's `end_task` hands to `dragon_task` once no crystal is alive (`end::end_dragon`), out of the bunker first; the fight waits in `Plan.bunker` while the dragon flies, and hands back every 1,200 ticks so E3's order (the retreat to the bunker under 14) is asked again.

## Walls

### Race 45873410249 (issue-20-end 9067ded, dev-end-no-crystals, 2 lanes x 1 bot, cap 12,000): 0/2 win, both dead
Development start, never evidence. The setup's kill worked: lane 1 saw 3 crystals at tick 40 and all 3 were gone the same tick; lane 0 saw none. The fall from y 80 cost nothing that shows in the case files: both bots were at 20 health when the fight began.

**What worked.**
- **The head strike.** Lane 0 sent 7 strikes on the head part (`ender_dragon:head#111`) from the head stand, at reach 2.78 to 2.99. All landed: 1.2 for the first (an uncharged swing), then 6.0 each. The dragon went 200 → 163 in one perch, then took off. The bot was unhurt while striking.
- **The phases.** The phase notes match the jar: landing → scanning → attacking → flaming → scanning, and takeoff.
- **The breath escape.** It fired for the flame clouds ("out of the breath"). Each flaming phase cost 3.0 once (indirect_magic), because the flame cloud (radius 5) lands on the head stand.

**Wall 1: a provoked enderman's pocket held the bot inside the dragon's reach (both deaths).** `ender_threat` cuts in ahead of every task, so while an enderman was provoked (the dragon's fireballs anger them; our gaze near misses were 0) the fight never ran.
- Lane 0, from tick 2,680: the pocket could not be sealed ("no block held for a pocket"), so the bot stood still at (8.5, 61, -1.3). A fireball's cloud then formed on its feet at tick 2,940, and it took 6.0 a second from tick 2,960 until it died at 3,040.
- Lane 1, ticks 2,240 to 2,420: it held a pocket 6.5 from the fountain beside the sitting dragon (3.9 along, 6.0 across). The scanning turn brought the head round: 10 mob_attack, a throw to y 89, and death by the fall.
- **Fix:** `Plan::dragon_first_step` runs before `ender_threat`. With a breath's reach on the feet, or the dragon grounded and the feet within 16 of the fountain, the pocket waits and the fight runs. It writes the note "dragon first: no pocket for enderman N", every 200 ticks at most, with its count.

**Wall 2: a fireball's cloud was read as radius 0.0** (lane 0, cloud #350 "radius 0.0, 0.0 from its edge" at tick 2,940). Its reach was then 1.5, so the bot never counted it as on its feet.
- **Fix:** a cloud's radius is never taken under 3.0 + 4/600 per tick of its age, capped at 7.0. That is the fireball cloud's own growth (E1).

**Wall 3: the stand behind the body is in the landing dragon's reach** (lane 0, tick 1,620). Walking in the basin to a stand behind, at 4.4 along and 2.9 across, the bot took 10 (mob_attack) as the dragon went from landing to sitting.
- **Fix:** landing, not yet sat, or with no head stand open, the bot goes out to the ring 12 off ("out of the landing's reach") and never enters the basin. When the dragon sits, the bot goes round to the head stand from outside.

**The flip-flop, fixed (a70a168).** At about 9 from the fountain the head stand and the way round the head swapped every tick (`go [10, 62, 0] round the head` / `go [8, 62, 4] to the head stand`). The way round chosen is now held (`DragonState::way`) until it is reached, or until the head, a breath or a way past the head blocks it.

**Wall 4: breath killed bots outside the fight too** (E4's race 2, 45875049490: one bot waited in a fireball cloud at a pillar's foot; one walked E3's route back to the bunker through breath). Breath is one rule now, owned here, ahead of every row and task in the End:
- **`Plan::breath_step`** (before the always-rows): a breath whose reach takes in the body sends the bot on a walk to the nearest ground cell out of every breath. It looks within 9 columns, never before a grounded dragon's head. The cell is held while it stays clear. A breath's reach means radius plus 1.5 on the plane, with the body's height meeting the cloud's 0.5-tall box with a margin of 1 (`breath_on`). So the bunker three down and a pillar two up are out of it. Notes: "out of the breath to …", "out of the breath", "in the breath, no way out".
- **`Plan::breath_ahead`** (on every tick's controls): a walk whose point 1.5 ahead lies in a breath's reach, with the feet not in one, stops at the edge and waits. Note: "walk waits at a breath's edge", every 100 ticks at most, with its count.

### Race 45876898172 (issue-20-end with 24b0a6f, dev-end-no-crystals, 3 lanes, cap 12,000): 0/3 win
Development start. Lane 1 ran to the cap; lanes 0 and 2 died. 26 head strikes in all. The dragon went to 145 (lane 0), 180 (lane 1) and 150 (lane 2). "dragon first: no pocket" fired 44 times, and no bot died in a pocket.

**Lane 0's death** was a breath during E3's "open the bunker". The global breath rule (bb3b247, wall 4) is the answer; this race predates it.

**Wall 5: the strike that makes it take off throws the bot off the island** (lane 2, tick 3,560). The dragon's takeoff count (`sittingDamageReceived`) is kept across perches and reset only when it goes over 50: 18.4 in perch 1, then 32 in perch 2. So 1 strike in about 9 starts a takeoff. That strike was made from the head stand on the surface. The dragon flew forward over the bot: head 10, wings 5, and a throw of 120 blocks off the island, dead by the fall.
- **Fix:** `DragonState::sat_damage` counts the takeoff count as the server does ("dragon takeoff count over" when it resets).
- A strike that would take it over 50 (`crossing`) is made only from under the wings' sweep and the head's: the body's top 0.2 under the dragon's feet less 2 (`under_wings`). Where one dig of end stone under the feet makes that so (`lowerable`: a 1-deep notch, climbed out by a jump), the bot digs first (`Want::Dig`). Where nothing does, it does not strike.

**Wall 6: the sweep hits the bot on the surface outside the crossing strike** (lane 1).
- A head stand 8.7 along took 10 in the attacking turn (tick 6,600).
- The natural takeoff's wings took 5 on the way to the head stand (2,440).
- The landing's wings took 5 on the ring 11 out, twice (4,080 and 10,900).
- **Fix:** every strike prefers a low stand (one dig where that makes it so). While the dragon lands or is not yet sat, the bot waits in E3's bunker (three down, under every sweep). With no bunker, the ring is 16 out, not 12.

**The flame's 3.0, answered before it comes.** The attacking turn lasts 40 ticks (`DragonSittingAttackingPhase`) and ends in the flame's cloud, radius 5 at the head (`DragonSittingFlamingPhase`). That cloud lies over the head stand, 2.5 from the head. Now the bot strikes in the turn's first 15 ticks (`FLAME_LEAVE_TICKS`), then walks to the nearest cell 7.5 or more from the head's point (`flame_clear_cell`), not before the head, and holds there while the dragon is flaming. A walk of 5 takes about 20 ticks, so it is out before the cloud forms. Note: "dragon: go … out of the flame's way". Not yet raced.

### Race 45882663266 (issue-20-end e14dd1e, dev-end-no-crystals, 3 lanes, cap 12,000): 0/3 win
Development start. Lanes 0 and 1 ran to the cap; lane 2 died. 25 head strikes in all. The dragon went to 148 (lane 0), 171 (lane 1) and 169 (lane 2). "dragon takeoff count over" never fired: no lane dealt 50 in total. "dig down" never fired either. No bot died in a pocket ("dragon first" fired 53 times).

**Wall 7: the walk out of the breath chose a cell the feet were already in** ("out of the breath to" 3,181 times).
- Lane 2 was on the bunker's stair at y 60.25. The cell under it (y 59) read clear, because its body top is 1.2 under the cloud. That cell was walked to 211 times while the bot took 3.0 a second, 20 to dead.
- Lane 0 stood 6.4 off a reach of 6.5. The cell's middle was just out, so the walk was done each tick, and the bot held at the edge for 370 ticks with every other task stopped.
- **Fix:**
  - A cell out (`clear_of_breath`) keeps a block more than the trigger, on the plane and in height (`OUT_SLACK`).
  - The feet's own column is never the target.
  - A cell reached with the feet still in a breath is refused until out.
  - "out of the breath to" is noted only when the cell changes.

**Throughput in race 45882663266.** A perch runs from landing to sitting until takeoff, about 1,100 ticks, which is four flame cycles. Counted from the ledgers:

| lane | perches | strikes per perch | hits taken per perch |
|---|---|---|---|
| 0 | 6 | 7, 0, 0, 2, 2, 0 (the cap) | 4, 0, 7, 6, 0, 0 |
| 1 | 5 | 4, 0, 0, 2, 2 | 8, 9, 0, 6, 4 |
| 2 | 1 (died in it) | 6 | 6 |

Most perches gave 0 to 2 strikes. That is the wall, not the cap. In the perches with no strike:
- **Breath churn** (wall 7). "out of the breath to" was noted 124 to 631 times a perch, and every other task waited. Lane 1's perches 2 and 3 went that way, as did lane 0's perch 3.
- **No head stand found from the bunker** (lane 0, perch 2, 3,740 to 4,800). The fight began in the bunker three down, and `ground` looked only 3 over the feet, so the surface 4 up was never a stand. The bot held "in the bunker" through the whole perch.
  - **Fix:** `GROUND_UP` 3 → 6.
- Each flame cycle cost about 3.0 to a bot walking out of the flame. The flame fix (da32c2b) leaves before the flame comes.

**Wall 8: a charge in the open** (E4's race 4). While the bot dug down two over the ground near (12, 63, 42), a charge (phase 8) hit it for 10 and then 5 and threw it 48 high.
- **Fix:** `Plan::charge_step` runs beside `breath_step`, ahead of every task. When the dragon is charging within 40 with its facing on the line to the bot (`charging_at`) and the feet are under the open sky:
  - with the bunker within 12 on the plane, the bot walks into it;
  - otherwise it crouches still where it is, and no dig or walk begins in the open.
- A breath on the feet still goes to `breath_step`.
- Notes: "charge: hold in the bunker at …", "charge: hold crouched where the feet are", "charge: over".

**Not yet answered.**
- The flame cloud costs about 3.0 each flaming phase.
