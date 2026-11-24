# The blaze fight (issue 16, the `rods_7` stage)

## Needs
A bot at a blaze spawner with the gear a real bot holds ends with 7 blaze rods (`rods_7`). No armoured, fed bot had fought a blaze before this file: the bare spawner snapshot lost 7 of 7 to fireballs in under 20 seconds from 6 to 7 health (`docs/problems/blaze.md` wall 3).

## The development start
- `dev-spawner` (`config/starts.toml`), raced with `snapshot=race-44789031743/snapshot-spawner-44789031743-race-44789031743-lane-1.tar.zst`: that snapshot's world, memory and player file put the bot at (129.5, 61, 166.4) in the Nether, 7 blocks from the spawner at (129, 64, 159), on the stairs the snapshot bot cut. The start's kit is given on top of what the bare bot held (armour, an iron sword, three more stone pickaxes, a second water bucket, 6 beef, 5 porkchops, sticks and cobblestone to the counts of the equipped Nether snapshot of race 45194253932), and its setup heals and feeds the bot (`instant_health`, `saturation`).
- Chosen over a fresh world with a teleport: the same fortress, the same spawner and the bot's own stairs are there, and a teleport into unloaded netherrack is not.
- A development start: its races are never issue-16 evidence and its snapshots never the frontier.
