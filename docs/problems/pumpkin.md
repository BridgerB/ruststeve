# Issue 80: carved pumpkin, and endermen angry at whom

**Status: open.** Lead lead-80. Branch `issue-80-pumpkin` (from `stack` 201dfcfe). Plan: https://github.com/BridgerB/ruststeve/issues/80#issuecomment-7241447566. Gaze rules: [`end-gaze.md`](end-gaze.md).

## Needs
No enderman angered by the bot's look on the stronghold and End legs, and no water poured or cover taken against an enderman that is not after the bot, with no rise in End deaths and the win rate no lower than on 9e1cfa0.

## Done (world terms)
- A parity row: a bot wearing a carved pumpkin looks an enderman in the eye and it stays neutral, on the real server and in the model.
- A real bot from `armed` finds a pumpkin (seen with an exposed face), carves it with shears and wears it from the way down through the End.
- From several crystals snapshots (wall 3 of `lava-portal.md`: lanes of one snapshot are correlated), pours and covers against endermen near zero where the enderman is not the bot's, End deaths and win rate no worse than 9e1cfa0's pooled crystals races, with the spread per snapshot.

## Risks
- A pumpkin patch is rare (to be measured: `patch_pumpkin` rarity); the kit may find none in its clock.
- The `armed` pack holds 7 raw iron (sword 2, boots 4): the shears' 2 ingots come ahead of the boots, a data choice.
- The pumpkin takes the helmet's head slot (armour 2).
- Skipping cover from an enderman judged not the bot's could cost a life if the judgement is wrong.

## Wall 1: in the End, the dragon angers the endermen, not the bot's look
- Race 46108868213 (9e1cfa0, 15 lanes from the crystals frontier, race 46101740935 lane 1, typecraft): win 6/15; "end: water poured at" 238 times in 14 runs, "end: covered from an angry" 142 in 13, "end: the hold from enderman" 60 in 13; `gaze near 0 (stares 0)` on 14 lanes (one near miss on lane 11).
- Every one of the 546 first "enderman#N angry at" lines (41 more are the same enderman again) in the 15 ledgers comes with "enderman#N hit: health" in the same tick or within 40 ticks, none without. They are hit 30 to 50 blocks from the bot, two at once, 5 then 3 (lane 0, ticks 2100 to 2120: endermen 197 and 198 at 50 blocks, 40 to 35 to 32): the dragon's breath or its wings, not the bot.
- So a pumpkin cannot drop these pours: it covers only the stare, and the End has none by discipline already.
- Cause, measured (probes 46160185304, 46160754145; `brain/game/mc-pumpkin.md`): `EnderMan`'s target goals are the stare (`EndermanLookForPlayerGoal`), `HurtByTargetGoal` (no alert), endermites and the anger reset. Any hurt whose entity is a living one makes the hurter the target, and all the dragon's damage is the dragon's: the wings' knock-back 5 (not while it sits), head and neck 10, the breath cloud 6 a dose every 20 ticks. `EnderMan.setTarget` raises `DATA_CREEPY` (17) for any target, so the bot's "angry" means "has a target", never whom. An enderman on the dragon turns on the bot only by a stare or a new hurt from the bot; the target drops once the dragon is unseen 60 ticks or beyond 64. The 3 of "5 then 3" is not explained (a re-hit inside the 10-tick immunity deals only the excess; unverified).

## Wall 2: a pumpkin is about 100 blocks off in the one world measured
- `patch_pumpkin`: one chunk in 300, 96 tries within 7 across and 3 up, only air over grass, plain pumpkins (jar, 80-measure).
- `replay --bin seen` over all 16 lanes of the five bow-line races (one `armed` snapshot, typecraft): a 16-pumpkin patch at x 1671 to 1680, z -13 to -20, seen exposed at tick 220, 101 to 108 blocks from the start; a second 241 to 250 off, seen only by lanes that wandered east. None within 96. Other worlds will vary a lot; the kit's radius is 150 (`[end_kit] pumpkin_radius`) and it gives up cleanly.
- Fix (planned, approved by the director): an enderman counts as after the bot only by the jar's targeting rule and what the bot sees (hurt by the bot, a stare, coming at the bot, teleporting toward it); pours and covers skipped for the others, each skip noted with its reason.
