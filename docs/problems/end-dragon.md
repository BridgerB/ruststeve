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

### Race 45886819862 (e5d912a, 3 lanes, cap 24,000), read mid-run (lane 2's evidence at tick 11,980)
Lane 2 had 4 perches and 1 strike; the dragon was at 199. Each flame cycle runs scanning (about 20 to 40 ticks, until its head faces the bot), then attacking (40), then flaming (200).

**Wall 9: the strike window was lost.**
- The bot chased the head stand as the scanning turn swept the head round. With no stand open it went for the bunker in the middle of the perch ("go … to the bunker" while sitting-scanning).
- It reached the stand as attacking began, then left at 15 ticks for the flame's way before its first strike, every turn.
- No damage was taken (0 hits in perch 1), but there were no strikes either.
- **Fix:**
  - Scanning, the stand is where the head will come: 9.0 out from the body on the line to the bot (`scan_stand`). The turn brings the head there.
  - Attacking, with its yaw held, the head stand as it points.
  - The flame leave moves from 15 to 25 ticks into the attacking turn.
  - Sat with no stand open, the bot holds near and out of the bite, never the bunker; the bunker is for landing and before the dragon has sat.

### Race 45886819862 (e5d912a, 3 lanes, cap 24,000): 0/3, all three dead
- **Lane 0:** the flame-clear cell was taken 3.6 from the fountain, beside the body and under the neck. The next scanning turn swept the head onto it: 10, a throw, death by the fall.
  - **Fix:** a flame-clear cell is also at least `BITE_OUT` + 1 (9) from the body.
- **Lane 2:** 7 strikes; the dragon reached 168. It died by a fall after a walk failed NoPath.
- **Lane 1:** killed by an enderman early ("near miss: enderman … stared at"), outside the fight.

### Race 45895541443 (9657b79, dev-end-no-crystals, 3 lanes, cap 24,000), lane 0 read from its evidence
- **Perches and strikes.** 10 perches, with 7, 3, 0, 1, 7, 2, 0, 0, 0, 0 strikes. The dragon went 200 → 94 by tick 11,360, then took no more damage for 11,000 ticks. Hits taken were 1, 0, 1, 0, 0, 0, 0, 0, 1, 0: the fight itself is safe now.
- **Wall 10: the takeoff count never started again.** The blow that sent the count over 50 was seen with the phase already takeoff, so it was never counted while sitting. "dragon takeoff count over" never fired. The count sat over 44, every strike was the crossing one, and each wanted a low stand ("hold no head stand open"; one dig begun at tick 19,900 was cut by the flame's leave).
- **Fix:** a drop counts if the phase noted before it was sitting. A takeoff within 20 ticks of a drop, with the count within a strike of 50, starts the count again, as the server does.

- **The rest of race 45895541443.** No win.
  - Lane 0 reached the cap with the dragon at 94: 20 strikes, then none for 11,000 ticks (wall 10).
  - Lane 2 brought the dragon to 96 with 19 strikes ("dragon takeoff count over" fired once, at 16,520), then died. Retreating "out of reach after a blow" to 19 from the fountain during a scanning turn, it drew a charge (sitting-scanning → charging at 22,480).
  - Lane 2's `charge_step` crouched it still (no bunker within 12). The charge still struck for 10, and two fly-by blows of 5 followed while it held an enderman pocket. Crouching in the open does not stop a charge aimed at the bot.
  - Lane 1 died in a fall during E4's clutch.
- **Wall 11, fixed in the next push:**
  - **Scanning, never far enough to draw a charge.** While the dragon scans, every way out is held within `SCAN_KEEP` (12) of the fountain: the blow's step out, the perch line's ring, and "into the scan's reach" from a hold. The dragon charges a bot it cannot find within 20 for 100 ticks.
  - **At a charge with no bunker within 12, a notch two deep.** The bot digs the end stone under its feet twice at once, crouched, and drops in. Never where the block under that is air (the void at the edge). Once the charge is over it digs a step out beside it, one under the surface, and climbs out a jump at a time. Notes: "charge: dig a notch at …", "charge: the step out dug at …".
  - Roofed: a block on a side first (on the wall's top), then the roof against it, so a fireball's cloud lies on the roof. The roof is dug back before the step out (the jump needs the head clear), and the step is never under the side block. Note: "charge: the notch roofed at …".
- **Wall 11 as first found:** a bot far from the perch during scanning draws a charge, and crouching does not answer one. To try next:
  - keep near the scanning stand (9 from the body) during scanning, which draws the attacking turn instead;
  - at a charge with no bunker near, a dug notch two deep, under the head's and wings' boxes.

### Race 45901376114 (622f39a, dev, 3 lanes, cap 24,000), read mid-run (lane 0's evidence at tick 4,740)
**Wall 12: a low stand was out of reach.**
- With the feet at 62 under a dragon at 67, a stand 2.0 off the head's box is 2.38 under it. The eye was 3.75 from the box, so "no head stand open" came every turn, and the dragon was at 193 by tick 4,740.
- **Fix:** `clear_of_blow` counts a body under the blow boxes as clear of them. The boxes reach down to 2 under the dragon's feet, the same floor as the wings' sweep. Such a stand may be right under the head, 2.4 from its box.

**Wall 13: sealed in a pocket, the fight walked for the bunker** (race 45901376114 lanes 0 and 1). An enderman's pocket sealed the bot in (21 blocks). The fight's wait while the dragon flew walked for the bunker: NoPath, "the fight plans again", 656 times over 13,000 ticks. The dragon stayed at 193.
- **Fix:** in a roofed 2-high cell (`end::covered`), the wait holds there ("in cover"). A walk that fails NoPath in cover plans again only after 200 ticks.
- Still open: a sealed pocket is not opened by the fight when the dragon sits.

**Race 45901376114 in full** (622f39a, dev, 3 lanes, cap 24,000): no win.
- **Lowest dragon health:** 157 (lane 2), 175 (lane 1), 193 (lane 0).
- **Strikes:** 8, 5 and 2. In lanes 0 and 1 every later perch was lost to wall 13 (2,091 "dragon walk to … failed" in all).
- **Lane 2's death, wall 14: walled in a breath.** An enderman's pocket sealed it in, and a flame's cloud then burned through the walls. 140 walks out failed NoPath, 20 health to dead.
  - **Fix:** a walk out of a breath that fails NoPath digs the wall toward the cell, at the feet's height and then the head's (`wall_toward`; never bedrock or obsidian). It then walks.

**Wall 15: no pocket in the End, ever** (the top killer across the End's races). Pockets held bots in the dragon's reach, shut E3's bunker stair, stranded the fight, and let a breath through their walls.
- **Fix:** in the End, `ender_threat` is never called (the lib.rs call site, fenced E5); `Plan::end_enderman_step` answers an angry enderman instead:
  - in a roofed 2-high cell (`end::covered`), the work goes on, since it cannot come in;
  - on open ground, into the bunker within 24;
  - otherwise, struck when in reach while health is over the danger line (40 health, the iron sword's 6);
  - under the line, the bunker however far, else the task goes on.
- Note: "end: no pocket for enderman N (…)".
- E3's covered guard and E4's up-high guard inside `ender_threat` stay as they are.

**Wall 16: breath near the bunker, answered sideways** (E4's race 45912100302 lane 0; frontier 45910659261 lane 1 on the stair). The bot was at y 59 in the bunker's hole, and a fireball's cloud lay in the hole with it. Every cell out was up on the surface (y 61): nine walks out failed NoPath, and the wall dig went sideways through end stone. 20 to dead in 3 s.
- **Fix:** breath is answered down.
  - Within 6 of the bunker, into it ("breath: into the bunker at …").
  - In it with the breath still on, a notch down from there.
  - A walk out that fails NoPath anywhere digs a notch two deep where the feet are and roofs it ("breath: dig a notch at …"). The notch is held while a cloud lies over it on the plane, then left by the roof and the step ("breath: out of the notch").
  - The sideways wall dig is no longer begun.
- The notch is the charge's code (`notch_dig_step`, `notch_leave`, `notch_exit_step`), shared.

### Race 45913296602 (e67f786, dev, 3 lanes, cap 24,000): the first fight with no pockets, 0/3
- **Dragon's lowest health:** 145 (lane 0, 10 strikes) and 156 (lane 2, 9 strikes). One "dragon takeoff count over" fired (wall 10's fix, counted).
- **Lane 0:** died of breath in the fight, on e67f786 before wall 16's fix.
- **Lane 2:** died in a fall during E4's clutch.
- **Wall 17: an enderman fought in the open** (lane 1, ticks 140 to 260). Angry from a stare at the arrival ("near miss"), with no bunker yet, it was struck 6 times for 26. It hit back for 7, 6.6 and 6.9 (20 → 2.3), and the bot died with the enderman at 13.6.
  - **Fix:** with no bunker near, a notch two deep where the feet are, roofed ("end: no pocket for enderman N (a notch down, no bunker near)"). An enderman can neither enter nor see into it. Out by the roof and the step once it is calm.

### Race 45918669138 (d7f7a01, dev, 3 lanes, cap 24,000), read mid-run (lane 1's evidence)
**Wall 18: no way out of a notch under the bunker.**
- Wall 16's answer worked: "breath: into the bunker", then "a notch down" at tick 880, and "out of the notch" at 1,500.
- But the notch was 2 under the bunker's floor, walled all round. No step out was found, and E3's retreat said "no walk to the bunker" every tick from 1,700 on.
- **Fix:** with no step to dig, the notch is risen out of, a block put under the feet at each jump (`rise_step`), to the height it was begun from. Notes: "notch: risen out at …".

### Race 45918669138 in full (d7f7a01): went backwards
- **Wall 19: the way into the bunker flapped.**
  - Lane 2 stood at the bunker's stair top (13, 58..59, -1) under a flame's cloud. The body's height put it in and out of the cloud from tick to tick: "into the bunker" and "out of the breath" 40 times each. Between them the fight's own walk climbed the stair back into the cloud, 3.0 a time, 20 to dead.
  - Lane 0 died the same way after 7 strikes (dragon at 168).
  - **Fix:**
    - The way into the bunker is kept to its end while a breath lies within its reach plus 2 on the plane.
    - In the bunker the bot stays, crouched, while one does.
    - A bunker not clear of every breath (on the plane and in height) is not gone to; the notch where the bot stands is dug instead.
- **Wall 20: held in cover while the dragon sat.**
  - Lane 1 was in its notch under the bunker (wall 18), and "end: covered" from an angry enderman ran the whole race: 0 strikes.
  - **Fix:** a fight walk that fails NoPath in cover while the dragon is grounded climbs out by a shaft straight up: the block over the head dug, a block put under the feet at each jump, until the sky is over the head. Never while a breath lies over the shaft on the plane: it stays in cover until the cloud clears. Notes: "out by a shaft up", "dragon: out of cover". Unit-tested.

**Wall 21: near the head stand, out of reach** (dev race 45924968115 lane 1). The bot stood 0.7 off the stand's middle, inside "at the stand" (1.0) but 3.2 from the head's box: "no head stand open" every turn.
- **Fix:** within 1.0 of the stand but over 0.25 off its middle, the bot steps onto the middle (`Want::Centre`).

**Wall 22: the bunker's breath hold through the strike window** (the same race, lane 2). The clouds the dragon kept sending held the bot in the bunker for 7,320 ticks, and the dragon stayed at 200.
- **Fix:** the hold yields while the dragon is scanning or attacking on its perch.

**Wall 23: every breath death in the real chain began in a strafe.** In the last three frontier races (45910659261, 45913263506, 45924970608) six bots died of breath, and all six followed the same sequence:
- a dragon phase change 1 → 0 (strafe over, the fireball out);
- its splash 20 ticks later;
- the first fatal hit 20 ticks after that.

| race | lane | strafe ended | splash | first fatal hit |
|---|---|---|---|---|
| 45910659261 | 1 | 12,100 | 12,140 | 12,160 |
| 45913263506 | 0 | 19,580 | 19,600 | 19,620 |
| 45913263506 | 1 | 12,420 | 12,460 | 12,480 |
| 45913263506 | 2 | 18,960 | 18,980 | 19,000 |
| 45924970608 | 1 | 9,760 | 9,780 | 9,800 |
| 45924970608 | 2 | 15,040 | 15,060 | 15,080 |

A strafe lasts about 180 ticks, time for a notch. Answering after the cloud lands is too late.

**Fix:** `charge_step` takes the strafe as it takes the charge (`strafing_at`: phase 1 within 64).
- On open ground (not in the bunker, a roofed notch, or up a pillar or spike, where E4's holds answer), the bot shelters before the fireball: the bunker within 12, else a roofed notch dug now.
- It stays until the phase has left the strafe and no cloud lies within reach.
- Notes: "strafe: shelter before the fireball (the bunker at … | a notch)".

**Wall 24: the strike that makes it take off never came** (dev race 45931228063 lane 2). The count stood at 44, so every strike was the crossing one and wanted a stand under the wings. One dig in the scanning and attacking window (about 60 ticks) was too slow: "no head stand open" every turn, and the dragon sat at 156 for 9,000 ticks.
- **Fix:** the low stand is dug through the flame. It is a notch 9 out from the body, on a bearing at least a right angle off the head, out of the flame's reach (`low_prep_cell`). The scanning turn then brings the head to it.

### Frontier race 45931225605 (8ef101e): the strafe shelter on the island
- **Firings:** "strafe: shelter before the fireball" fired 4 times in lane 0 and 11 in lane 1. Both died of breath all the same.
- **Lane 0:** the strafe began 69 off and ended 56 off. The fireball flew 40 ticks with the phase already holding, so the shelter never fired for it.
  - **Fix:** a dragon fireball in flight within 64 counts as the strafe (`fireball_at`).
- **Lane 1:** sheltered in the bunker at [14, 58, 0], and the fireball came into the bunker (the cloud at 0.0 off, the bot at 14.7, 58.2, 0.7). The bunker was open: E3's stair and door. **Fix (E5):** the strafe shelter and every breath answer skip a bunker that is not shut (roof and the door's first cell solid: `Plan::bunker_shut`, E3's own test), and dig the roofed notch instead. E3 seals the bunker on a strafe.

**Not yet answered.**
- The flame cloud costs about 3.0 each flaming phase.

### Dev race 45937087645 (0783310, dev-end-no-crystals, 3 lanes, cap 24,000), read mid-run: the strafe shelter was wrong
**Wall 25: hidden for the whole strafe, the strafe never ended.** The dragon was at 200 in all three lanes and was never struck.
- **Lane 0:** the strafe began at tick 780 and "strafe: shelter before the fireball (a notch)" fired. The bot sat in its roofed notch at (13, 57, 3). The phase was still strafe at tick 11,900, with no change in the ledger in between. The fight handed back at 1,820 with 0 strikes, E3's stairs out failed (dig `Refused`) twice and stalled, and the bot was idle in the notch from 3,840 on.
- **Lane 2:** the same, from 660. **Lane 1:** the strafe held from 900 to 4,700.
- **Why the rule was wrong** (the 26.1.2 jar, `javap` of `DragonStrafePlayerPhase.doServerTick`, probe 45938702513): a strafe ends only one of two ways.
  - It fires: `fireballCharge` goes up 1 a tick only while the player is within 64 (`distanceToSqr < 4096`) and `dragon.hasLineOfSight(target)`, and goes down 1 a tick otherwise. At 5 or more, with the target within 10 degrees of the dragon's facing, it fires and sets `HOLDING_PATTERN`.
  - It has no target (`attackTarget == null`), and sets `HOLDING_PATTERN`.
  - Out of sight, the charge never builds, so the dragon circles in the strafe and never goes back to the holding pattern it perches from. Sheltering for the whole strafe (walls 23 and the 3f0d5be rule) turned the fireball's danger into a fight that never starts.
- **Fix:** the strafe is met in the dragon's sight (`Plan::strafe_seen_step`, called from `charge_step` when there is no charge).
  - With no threat, the bot digs a notch two deep in the cell beside the feet and stands by it, sneaking. Notes: "strafe: a notch dug beside at …, stood by in sight" and "strafe: stood by the notch at … in sight".
  - With a threat, it steps into the notch (not sneaking) and roofs it ("strafe: into the notch at …"). A threat is a fireball in flight within 64 (`fireball_at`) or a cloud within reach and `OUT_SLACK`.
  - Hidden in its notch through a strafe with no threat, it stays `STRAFE_HIDE_TICKS` (200), then digs the roof back and rises out a block at a jump to be seen ("strafe: out of the notch to be seen at …").
  - A charge is still hidden from at once, and a cloud within reach still holds it in.
  - With no notch beside ready at a threat, the shelter is as before (a shut bunker within 12, else a notch where the feet are).
- **Unproven:** whether 200 ticks is right, and whether the step in beats the fireball. The fireball spawns at the head and flies about 30 to 40 blocks; the step is one block and a 2-block drop. E3's seal-on-strafe for the bunker hides the bot the same way and will hold the strafe too unless it is capped.

### Dev race 45940964839 (dd18d00, 3 lanes, cap 24,000), read mid-run (lanes 0 and 1 at tick 4,700)
- **The fix's first race.** Lane 0 perched twice: 4 head strikes, the dragon to 181. Lane 1: 6 head strikes, the dragon to 169. No breath hits in either lane.
- **Lane 1 is the fix working:** "strafe: stood by the notch at [14, 58, 2] in sight" at 2,940, and in the same tick the strafe fired and went to holding. The fireball passed 14 off.
- **Wall 26: the old shelter still fired first.** At each strafe's first tick, on the bunker's stair, no cell beside fitted a notch, so the step fell back to the old shelter and dug a notch at the feet. Lane 0 then stood by a notch from a block down, mostly out of sight, and the strafe held 1,380 ticks (2,720 to 4,100).
  - **Fix:** with no cell beside (or in the air, or the notch's dig failed) and no threat, the bot stands where its feet are, in sight ("strafe: no notch beside, stood in sight at …"). The old shelter is for a threat only.
- **Wall 27: a silent idle (lane 1).** From tick 3,580, after "charge: over", the bot sat idle in the bunker cell (14, 58, 2), its door open. There was no note of any kind for 15,800 ticks. A strafe at 17,800 to 19,300 sent a fireball whose cloud landed on it, and it died at 19,420. `why` at 10,000 shows the bunker open and the roof solid. It cannot name the row, because the recording does not hold the planner.
  - **Backstop:** in the End, idle with no note for `IDLE_NOTE_TICKS` (600), the planner notes "end: idle N ticks at … (task …, rows held …)", drops E5's holds and chooses again now (`Plan::end_idle_watch`). `step_inner` records which rows sent controls while idle (`Plan::held`).
  - **The notch beside is never dug** in the bunker's column or its stair's (`Plan::bunker_columns`). In lanes 0 and 1 it was the bunker's own floor.
  - **A half-dug notch is taken** at a threat, with its second block dug under the feet. In lane 2 the strafe fired 40 ticks after the bot was seen, with one block dug.
- **Wall 28: cover is no answer to a fireball** (the 26.1.2 jar, `javap` of `DragonFireball.onHit`, probe 45938702513). On a hit, the fireball lists the living entities in its box grown by (4, 2, 4). The cloud (radius 3, 600 ticks, growing to 7) is put at the first one within 4 blocks (`distanceToSqr < 16`), walls or not.
  - All three deaths in this race fit: the cloud "0.0 from its edge" of the bot, on it in the bunker (lanes 0 and 1) and in a half-dug notch (lane 2), the splash 1 to 2 blocks off.
  - A notch or bunker within 4 blocks of where a fireball lands puts its cloud on the bot. Only distance shelters: the bot more than 4 from the impact, and the cloud's growing reach kept clear after.
  - The notch still answers a charge (a body blow) and the perched flame (no fireball).
  - **Fix (the dodge, `Plan::fireball_dodge_step`):** a dragon fireball in flight within 64, aimed within `DODGE_AIM` (6) of the bot (its velocity, else straight at it), is dodged on open ground.
    - **Where:** sideways to its flight, `DODGE_MIN` to `DODGE_MAX` (6 to 10) from the cell the bot stood in, the side away from the dragon first (`dodge_cell`).
    - **Which cells:** a cell stood on in the open (two sides open, the sky over it), never in the bunker's or its stair's columns, ground under every column within 3 (no void edge), and clear of every cloud.
    - **Note:** "fireball: dodge to … (flight …, ticks …, from …)".
    - **Never cover for a fireball:** the shelter rows, `breath_step` and `breath_ahead` read clouds only (`clouds_only`).
    - **The notch is kept** for a charge and the perched flame. A strafe with no fireball is met standing in sight ("strafe: stood in sight at …"); the notch beside is gone.
- **Wall 29: a hold from an angry enderman never ended** (E4's race 45942992471 lane 0: 12,000 ticks, "end: the walk from enderman … to the bunker failed" 3,071 times). An enderman with a target keeps its anger for good (its timer restarts every tick), so a hold "until it calms" never ends.
  - **Fix (`Plan::end_enderman_step`):**
    - Beyond `ENDER_NEAR` (8) the task goes on: it teleports to the bot only from over 16 and cannot come into a 2-high cell.
    - Within 8 in a roofed 2-high cell: struck from there in reach over the danger line, else held.
    - Within 8 on open ground: into the bunker (a failed walk is never tried again for that enderman), else a notch.
    - Every hold from one enderman is capped at `ENDER_HOLD_CAP` (600), then the choice is made again ("end: the hold from enderman N capped").
- **Wall 30: out of the dragon's sight under the sky, the strafe held** (frontier race 45943073264 on 1a54bdf, lane 1). The bot "stood by" at (13, 52, 0), ten under the surface, and the strafe held from tick 480 past 24,040. Lane 2: from 180 to 9,760.
  - **Fix:** in a strafe, with no line from the dragon to the bot (`cover::seen_body`) for `STRAFE_UNSEEN_TICKS` (100) under the open sky, the bot rises a block at a jump until seen, at most `STRAFE_RISES` (10). With no sky it is noted once, for the End's rows to take it out ("strafe: unseen N ticks at …, no sky").

### Frontier race 45947478869 (723e4d8, from `island`, 3 lanes, cap 36,000): 0/3, all dead of breath in the crystals stage
- The dragon was at 200 in every lane. Dodges fired 3, 2 and 2 times; breath deaths 3.
- **Wall 31: the dodge began too late.**
  - Lane 0 saw its fireball 20 ticks out. Walking, it covered about 4 blocks, inside the 4-block snap, and its dodge cell was on the fountain's rim.
  - Lane 2 saw one 6 ticks out.
  - Lane 1's dodge walk failed `NoPath` at once and the bot stood. Lane 1 was also up E4's pillar once ("no dodge cell").
  - **Fix (keep moving):** a fireball aims at where the bot is when it fires, with no lead. So in a strafe the bot paces in sight, a leg of 6 to 10 sideways to the dragon's bearing and back (`Plan::pace_step`, "strafe: paced in sight from … to …").
  - A fireball seen on a leg is dodged on the side the leg already goes (no turn back).
  - Dodge and pace cells keep `FOUNTAIN_CLEAR` (5) from the fountain's axis.
  - A failed dodge walk presses straight on toward its cell instead of standing.
  - Sprint (about ×1.3) needs physics, protocol, the bot loop and the sim; it is left to its own issue, pending these races.

### Dev race 45947481217 (723e4d8, dev-end-no-crystals, 3 lanes, cap 24,000): the dragon killed in 2 of 3 lanes
- **Lane 0:** dragon dead at tick 19,500, credits, home (47 strikes, 9 or more dodges, 2 breath hits).
- **Lane 1:** dragon dead at 19,840, credits, back in the overworld (56 strikes, 4 breath hits).
- **Lane 2:** died of breath at about 8,440, the dragon at 145.
- The first kills on a real server, from a development start (never the issue's evidence: `brain/ruststeve/decisions/development-starts.md`). They ran before wall 31's pacing.
- **Wall 32 (lane 2's death):** dodging at 8,320, the bot stood on an older cloud's edge. `charge_step` left a cloud on the feet to `breath_step`, whose walk out went the other way, and the next fireball's cloud was put on it at 8,360. **Fix:** the dodge comes ahead of the walk out of a cloud (its cell is clear of every cloud).

### Frontier race 45954274931 (21efdaf, from `island`, 6 lanes): 0/6 crystals, 5 dead of breath, the dragon at 200
- **Wall 33: the dodge chose cells it could not walk to.** The three deaths read as "holding -> landing-approach" were not the perch. That was only the last phase note before death. The bots were 38, 53 and 15 from the fountain, and in each the fireball's cloud was put on the bot.
  - **Lane 2:** the dodge to [7, 62, 40] failed `NoPath` at once. Pressed straight on, it went nowhere, then the walk out of the cloud failed `NoPath` too, in E4's pool water.
  - **Lane 3:** just down from a clutch fall at (46, 58, 0) by a spike. The fireball was seen 5 ticks out, and the dodge failed `NoPath`.
  - **Lane 5:** repeated clouds wore it from 20 to 3.5. Its dodge to [12, 62, 8] crossed an older cloud's reach.
  - **Fix:** a dodge (and pace) cell is taken only with a route found from the feet (`path::plan`, `DODGE_NODES` 800), at most `DODGE_STEPS` (16) long, none of its cells in the reach of a cloud not already on the bot. It tries sideways first, then 30 degrees either way off sideways; never along the fireball's line.
  - **The `NoPath`, read with `replan` (probe 45959575448):** lanes 2 and 3 stood in a layer of water spread over the ground (y 62 at (14, 62, 33); y 58 round (46, 58, 0), after the clutch). The planner has no start in water ("the feet cell is water"), so every walk from there fails. From water the dodge and the pace press straight on (`in_water`, `toward`), and the dodge cell's way is checked as a straight line off the clouds. The water left on the ground is E4's (the pool, the clutch).
