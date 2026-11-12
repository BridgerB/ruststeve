# Problem: fourteen bots, one spawn, the same trees

Issue 10's done-condition: from the spawn start, 90% of real bots hold a crafting table within 5
game minutes (`race.yml`, 4 lanes × 14 bots, cap 6,000 server ticks, `goal=table`).

## What the bot needs
Logs it can dig *and keep*: a log whose drop lands in its own touch box, not a neighbour's.
Vanilla gives a dropped item to whichever player's box touches it first.

## Risks
- Bots crowding one trunk dig each other's logs and pick up each other's drops; each waits out
  its collect for a drop that is already gone, then picks the same crowded trunk again.
- Spreading the start would hide this. A lone bot in a real game meets other players too, so
  the fix belongs in the bot.

## Done, in world terms
In a lane of 14 bots joining at one spawn, at least 90% hold a crafting table by 6,000 server
ticks, and no bot spends its time waiting at a trunk another bot is working.

## The last recorded failure
Race run 43304044744 at 8df1710 (2026-11-12): **44/56 (78.6%)**, each lane 11 of 14. The
lane's last bots to get wood failed. Every failing case ended at the cap with 3 to 5
`Uncollected` digs. In `rs-l00-b07`'s recording, `why --at 1000` shows **six other players at
0.0 blocks**, all at the trunk at (36, 75, -28), and the bot's inventory empty. Its notes show
three digs there in a row ending `Uncollected`, each with the drop last seen 0.3 blocks from its
feet: a neighbour touched it first. One "dug" came 2 ticks after the dig began, on a log another
bot had already broken. The task counted it as done because a gone block has no drop to wait
for.

## Fixes (10c)
1. Choosing a log skips one with another player within 2 blocks of its column (a trunk being
   worked).
2. Collecting fails at once (`Taken`) when the drop entity it saw is gone and its own count did
   not rise. Before, it waited out 300 ticks.
3. A dig whose block is already gone when it starts fails `Gone`; it never counts as dug.
