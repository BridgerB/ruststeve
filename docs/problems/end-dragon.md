# Issue 20g: the dragon and the win (E5)

**Status: built, unit-tested on hand-built worlds; no race yet (2026-11-26).** Part of [`end.md`](end.md).

## Needs
With every crystal gone, one bot kills the dragon and receives the credits: `w.won` (`win_game` after the exit portal), answered with the respawn, back in the overworld. Stage `win` (E1's) reads it.

## Done (world terms)
A case file in the End shows, in order: `dragon fight`, `dragon strike body` notes with the dragon's health falling, `dragon dead`, `dragon: exit ...`, `credits`, `respawn after the credits (1)`, `back from the End in minecraft:overworld`.

## What is built (`crates/plan/src/dragon.rs`)
- **The model, pure:** the phase from entity data 16 (`Phase`, ids 0 to 10 as E1 read them from the jar); perched = within 6 of the fountain (0.5, 0.5), under y 70, held 10 ticks, in a sitting phase once one is said (E1: the perch is about y 67); the head point at the body plus 6.5 along the yaw (sin, −cos) and 0.5 under the feet (the head part's 1 by 1 box at −1); breath clouds (`area_effect_cloud`, `dragon_fireball`) with their radius from data 8 plus a 1.5 margin.
- **The yaw:** nothing decoded it before. `move_entity_rot`, `move_entity_pos_rot` and `teleport_entity` now carry it into `world::Entity::yaw` (protocol and world, approved by the orchestrator). `add_entity` and `entity_position_sync` do not yet: a dragon first seen sitting still has no yaw until it turns, and the fight holds ("the dragon's yaw not yet told").
- **The choice each tick** (`decide`, clojurecraft's `toward-dragon` order): dead → into the exit portal (a seen `end_portal` cell near the fountain; straight in from the rim); a breath's reach on the feet → the nearest ring cell 4 to 12 out, out of every breath, not before the head, the way kept 2.5 from the head and 3.0 from the neck; the dragon's own blow (a hit whose cause and direct are the dragon) → out to max(8, feet + 4) for 100 ticks; flying → E3's bunker (not merged yet: a ring cell 12 out), and in a lull (no breath, not grounded, no hostile in 8) eat while hurt with the bar under 20; grounded → under the perch line, out to the ring; else the stand behind and the strike.
- **The stand:** a cell of the basin's floor (1.4 to 2.6 from the fountain) at least 1 behind the body along its yaw and out of every breath, the nearest the feet; the held stand kept with a slack of 1 (`keep_stand`). From outside the basin a way that crosses before the head goes round it first (`around`).
- **The strike:** `Act::Attack` on the dragon's own id while sat, the feet behind the body and the eye within 3.0 of its 16 by 8 box. It lands on the body part: 6/4 + 1 = 2.5 a sword blow, 20 blows to the 50 that end a perch (E1), 80 in all.
- **The credits:** `Plan::dragon_watch` runs first every tick (before the body check, since the respawn clears it): phase-change notes, the perch count, the death (status 3, or the boss bar gone after one was shown), and on `w.won` the "credits" note and `Act::Respawn`, asked again after 200 server ticks (at most 5), every other row held until the overworld.
- **Guards elsewhere (fenced E5):** `defend::NEVER_FOE` holds the dragon (the defend's chase took clojurecraft's bot into the head); the eat always-row waits while the dragon sits within reach (`dragon_holds_meal`); the fight eats in its own lulls.

## The perch line (why one blow, behind only)
- A head or neck touch is 10 before armour (E1, `EnderDragon.hurt`). Through iron with boots (armour 15, no toughness) it is 10 × (1 − 10/25) = **6.0**; without boots (armour 13) 6.8, clojurecraft's measured number.
- Danger 8 plus two blows is 20.0: exactly full health, so a two-blow line never lets the bot strike. `perch_line` therefore takes **danger plus one blow, 14**, and the head is never struck (`head_ok` false). The bot strikes only the body, from a stand behind it, outside the head's box (6.5 out, inflated 1) and the neck's (5.5 out, 3 wide, inflated 1).
- If a softer blow is ever measured (two blows plus danger under 20), the same code strikes the head when it is within 3.0.

## Risks (unproven until a race)
- The attack on the dragon's own id is judged by reach to its 16 by 8 box and lands on the body part (E1's jar reading); a real server may still refuse it. Parity or the first race says.
- The wings: not sitting and at hurtTime 0 they push 4 out and hurt 5 within their box inflated 4. At takeoff the bot is on its stand in the basin; the fight leaves for the ring only once the phase says takeoff.
- The interim call site: until E3's `end_task` calls `dragon_task`, `step_inner` begins the fight itself in the End with no crystal shown and nothing in hand (fenced "Interim", to be removed at the merge with end.rs). The bunker is `None` until `Plan.bunker` lands (one line, fenced "E5 merge").

## Last recorded failure
None yet: no race has run this code.
