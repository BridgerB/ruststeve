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
- So a pumpkin cannot drop these pours: it covers only the stare, and the End has none by discipline already. Hypothesis until the jar answers: an enderman hurt by the dragon targets the dragon (`HurtByTargetGoal`), its creepy flag (metadata 17) goes up for any target, and the bot reads every creepy enderman as after it. Measuring: `EnderMan` target goals, `DATA_CREEPY`'s rule, the breath and wing damage (80-measure).
- Fix (planned, approved by the director): an enderman counts as after the bot only by the jar's targeting rule and what the bot sees (hurt by the bot, a stare, coming at the bot, teleporting toward it); pours and covers skipped for the others, each skip noted with its reason.
