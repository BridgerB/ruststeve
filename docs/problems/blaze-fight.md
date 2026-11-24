# The blaze fight (issue 16, the `rods_7` stage)

## Needs
A bot at a blaze spawner with the gear a real bot holds ends with 7 blaze rods (`rods_7`). No armoured, fed bot had fought a blaze before this file: the bare spawner snapshot lost 7 of 7 to fireballs in under 20 seconds from 6 to 7 health (`docs/problems/blaze.md` wall 3).

## The development start
- `dev-spawner` (`config/starts.toml`), raced with `snapshot=race-44789031743/snapshot-spawner-44789031743-race-44789031743-lane-1.tar.zst`: that snapshot's world, memory and player file put the bot at (129.5, 61, 166.4) in the Nether, 7 blocks from the spawner at (129, 64, 159), on the stairs the snapshot bot cut. The start's kit is given on top of what the bare bot held (armour, an iron sword, three more stone pickaxes, a second water bucket, 6 beef, 5 porkchops, sticks and cobblestone to the counts of the equipped Nether snapshot of race 45194253932), and its setup heals and feeds the bot (`instant_health`, `saturation`).
- Chosen over a fresh world with a teleport: the same fortress, the same spawner and the bot's own stairs are there, and a teleport into unloaded netherrack is not.
- A development start: its races are never issue-16 evidence and its snapshots never the frontier.

## The baseline (race 45335627948, 360791f, `dev-spawner`, 3 x 1, cap 9,000)
- **Outcome**: 3 of 3 dead in 620, 1,520 and 2,460 ticks (a piglin's arrow; fire from a blaze twice). 2 blazes killed (lane 0), no rod seen or held.
- **The place** (probe 45336268694, `why --blocks` on lane 2 at tick 1,000): a bridge runs south from the spawner along z, three wide (x 128 to 130), floor y 60, with a low wall each side. From z 165 to 169 it is roofed (walls y 61 to 63, roof y 64). Two full steps at z 163 and 162 lead up to the spawner's platform: floor y 63, x 127 to 131, z 157 to 161, fenced, the spawner in its middle at (129, 64, 159). South of z 170 the bridge is open to the sky.
- **The fights** (every `defended:` note):

| lane | ticks | swings / landed | health | stance at end | foe over the eye | worn |
|---|---|---|---|---|---|---|
| 1 | 40 to 260 | 3 / 3 | 20.0 to 8.8 | Retreat | 2.5 | nothing |
| 2 | 40 to 260 | 4 / 4 | 20.0 to 11.3 | Retreat | 2.0 | nothing |
| 2 | 780 to 1,080 | 0 / 0 | 18.2 to 0.4 | Retreat | 2.4 | armour |
| 0 | 40 to 120 | (cut by the fire escape) | 20.0 to 11.0 | Fight | | nothing |
| 0 | 120 to 240 | 0 / 0 | 11.0 to 12.3 | Retreat | 1.5 | nothing |
| 0 | 900 to 1,000 | 5 / 4 | 18.7 to 16.8 | Fight (foe dead) | 1.4 | armour |
| 0 | 1,000 to 1,060 | 4 / 3 | 16.8 to 9.7 | Fight (foe dead) | -0.7 | armour |
| 0 | 1,100 to 1,180 | 0 / 0 | 7.7 to 7.8 | Retreat | 0.4 | armour |

- **Health lost by cause** (hit facts and health lines, three lanes): fireballs 13 hits (5 each bare, 2.9 in armour); fire about 45 ticks of 1; blows of 5 with no hit fact 6 times, all in the bare first fights with a blaze at 1 to 3 blocks (most likely a blaze's touch of 6 landing inside the invulnerable half second of a fire tick, which sends the difference and no damage event: a blind spot, reported); a piglin's arrows 5 (lane 1); standing in fire 5 (lane 1).
- **Where they died**: all three while the planner noted "rest (health under 18; out of sight)". Lane 0 rested on the open bridge at (129, 61, 183 to 188) with blazes 18 to 24 blocks off and took 4 fireballs there; lane 2 the same at (131, 61, 181); lane 1 rested in a burning cell with a crossbow piglin 5 to 7 blocks off. The cover looks only at blazes within 16; they shoot from 48. That code is the spawner task's (another agent's wall); it is named here because it ends every run.

## Walls

### 1. The first fight is fought bare (race 45335627948, all three lanes)
- **What happened**: the kit's armour lay in slots 17 to 19 through the first fight. "defend Blaze … (Fight)" began at tick 40 in every lane; "put on the armour in slot 17" came at tick 260, slots 18 and 19 at 280 to 320, after the fight had ended. The bare fights cost 20.0 to 8.8, 20.0 to 11.3 and 20.0 to 11.0 in 80 to 220 ticks.
- **Read**: `Plan::equip` is called from `nether_task` only, which runs when a task is chosen. The `defend` always-row cuts in before any choice, so with a blaze within 16 at full health nothing puts the armour on until the fight is over. A real bot wears its armour from the overworld, so this is the start's own gap as much as the bot's; it also bites any bot that picks armour up or is given it mid-run.
- **Fix**: at the top of the always-rows, in the Nether, a held iron piece whose armour slot is empty is quick-moved on (one a second, with no click in flight, no window open and nothing on the cursor): `unworn_armour`, the note "armour on from slot N". The fight goes on meanwhile.
- **Unit test**: `held_armour_with_its_slot_empty_is_put_on` (plan).
