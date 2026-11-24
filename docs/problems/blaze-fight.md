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
- **Raced** (743cf70, race 45337168104, `dev-spawner`, 3 x 1): the note "armour on from slot 17 / 18 / 19" at ticks 40, 60 and 80 in all three lanes (9 firings). In armour the first fights land: 41 swings and 32 landed in three lanes (10 and 7 before), 9 blazes killed (2 before), **3 rods held** (lane 0 two, lane 2 one; none before). All three still died: lane 1 at tick 300 to two blazes' touch, lane 2 at 360 to a zombified piglin its own sweep had struck, lane 0 at 2,680 resting in sight. Walls 4 and 5.

### 2. A fight held on a blaze no strike reaches (race 45335627948, lane 2, ticks 780 to 1,080)
- **What happened**: healed to 18.2 and in armour, the bot began "defend Blaze 17 (Fight)" and ended it 300 ticks later: "0 swings, 0 landed, Retreat … health 18.2 to 0.4, foe up to 2.4 over the eye". Five blazes (14, 15, 16, 17, 117) charged in turn; three fireballs (2.9 each) and nine fire ticks landed between ticks 900 and 1,060.
- **Read** (`why --at 1000`, probe 45336268694): the bot stood at (130.5, 61.2, 164.8), at the foot of the two steps, under the north end of the bridge's roof; blazes at 4.3, 5.7 (health 0.6, from the first fight), 6.1, 6.7 and 7.6 blocks. "2.4 over the eye" from feet at y 61 puts blaze 17's feet at y 65.0: sitting on the roof (its top is y 65) or hovering over the platform. A blaze with 0.6 health was 5.7 blocks off and was never the target.
- **Cause**: (1) the fight keeps the one foe it began with (the nearest when it began) and strikes only that one; (2) its reach is the eye to the mob's middle, 0.9 up the box, where the game measures to the box, so a blaze just over the bot's head is "out of reach" a block sooner than it is; (3) the invariant that ends a fight landing nothing waits 200 ticks, and a blaze's whole wind-up and volley is 72: the bot stands in five blazes' sight for nearly three volleys each. The same shape against a skeleton: race 45204423376 lane 0, a fight and retreat of 780 ticks, 0 swings, 18.4 to 7.1.
- **Fix** (`defend.rs`, the shooter paths only): a shooter's fight strikes whichever mob of the foe's kind is nearest in reach, by the box and in plain sight (`in_reach`, `box_distance`, `sight`: no strike through a roof), and makes it the foe (note "struck N in reach, not M"); and it is given up after `shooter_patience` (100 ticks, `config/goals.toml`) with nothing landed (note "shooter fight given up: …"), the foe then left alone 600 ticks as wall 30 has it.
- **Unit tests**: `reach_is_measured_to_the_box_not_the_middle` (bot/tests/blaze_fight.rs).

### 3. A retreat from a shooter runs away in the open (race 45335627948, lanes 0 and 1)
- **What happened**: lane 0's fights at ticks 900 to 1,060 killed two blazes and ended at 9.7; the fire took it to 7.0 by tick 1,100 and "defend Blaze 10 (Retreat)" then ran 80 ticks. Lane 1's first fight turned to Retreat at 4.0 and ran until tick 260. Lane 2's wall-2 fight turned to Retreat at 6.6 (tick 960) and burned to 0.4 by 1,080.
- **Read** (`defend.rs`): a retreat walks a route to 21 blocks from the one foe (`Goal::Away`, the reach of 16 and 5), whatever sees the bot on the way and there: on this bridge that is 21 blocks down an open deck with blazes that shoot from 48. And the line that turns a fight to a retreat is the danger line of 8, met only after the fire of the last hit has burned on: a fireball in armour is 2.9 and its fire 4 to 5 more.
- **Cause**: the retreat was built for creepers and melee mobs (distance is safety). From a shooter, safety is a full block in the line.
- **Fix** (`defend.rs`, the shooter paths only): a fight with a shooter turns to a retreat at `shooter_retreat_health` (13: the danger line and one hit's fire). A retreat from a shooter ends where no shooter has a line to the bot's body (`cover::seen_body` over `cover::shooters`, the spawner task's own cover: blazes within 48; note "in cover at …"); seen, it walks a found route to the nearest cell out of sight (`cover::cover_cells`; note "retreat to cover at …"); with no such cell or route it backs away as before. The hurt bot's rest after that is the spawner task's.
- **Unit tests**: the cover's own (bot/tests/cover.rs).

### 4. In touch with blazes, and fights begun hurt (race 45337168104, lanes 1 and 2)
- **What happened**: lane 1, in armour on the platform at (130.3, 64.8, 160.2), killed two blazes in 240 ticks and died at tick 300: "hit mob_attack by blaze" five times, 3.6 each (ticks 200, 260, 280, 280 and the last), with blazes 14 and 15 "facing at 1". Its third fight began at 9.9 health ("defend Blaze 15 (Fight) … health 9.9") and lasted 20 ticks. Lane 2 began its last fight at 12.0 and took a blaze's touch at once (12.0 to 8.4).
- **Read**: a blaze strikes by touch once a second inside 2 blocks of its target (vanilla `BlazeAttackGoal`: `distanceToSqr < 4`, then it moves at the target) and that is 6, 3.6 in iron armour, with no wind-up; its fireball in armour is 2.9. The fight closes to its reach and stands there, among the blazes; nothing steps off one that closes. And `Defend::new` begins a Fight at any health over the danger line of 8, so the wall-3 line of 13 (not in this race) would have turned a fight to a retreat and the next one begun at once.
- **Also measured** (another agent's race 45336326999): a fight's close-in is a blind step forward. Two bots walked off fortress bridges into lava closing on a blaze and a skeleton, and one stepped back into fire 13 times in 300 ticks.
- **Fix** (`defend.rs`, the shooter paths only): (1) a shooter is met with a retreat, not a fight, at or under `shooter_retreat_health`; (2) with a mob of the foe's kind within `KEEP_OFF` 2.4 of the feet the fight steps away from it, the strikes going on from the bot's own reach of 3 ("kept off N ticks" in the fight's note); (3) a shooter's fight steps only where the cells 0.9 and 1.7 ahead hold no lava or fire and have a floor right under or one down (`safe_step`).
- **Unit test**: `a_shooters_fight_steps_onto_floor_only` (bot/tests/blaze_fight.rs).

### 5. A sweep strikes a zombified piglin (race 45337168104, lane 2)
- **What happened**: at tick 320 "blow landed on zombified_piglin#14" beside "blow landed on blaze#196"; at 340 "hit mob_attack by zombified_piglin, health 2.3" (5.1 in armour); dead at 360. The piglin had stood 2 to 4 blocks off since tick 260.
- **Read**: a full-charge swing on the ground sweeps every mob within a block of the target's box. Zombified piglins are `neutral`: never chosen as a foe until one hits, for this reason (fortress wall 2). The sweep struck one anyway.
- **Fix** (`defend.rs`, the shooter paths only): no swing while a live `neutral` mob is within `SWEEP_NEAR` 2.0 of the foe ("sweeps held N ticks" in the fight's note); the fight's patience ends it if the mob stays.

### Walls 2 to 5 raced (1efbec1, race 45338756013, the healthy spawner snapshot of race 45336326999 lane 2, armour worn, 3 x 1, cap 12,000)
- **Firings**: "retreat to cover at" 33 (7, 8, 18), "in cover at" 11, "struck N in reach" 7, "shooter fight given up" 8; "kept off" 12 ticks in all; no sweep held.
- **Outcome**: 3 of 3 dead, at 980 (a wither skeleton, during a retreat), 2,900 (fire) and 6,720 ticks (a blaze's touch at 0.3 health); 6 blazes gone, **2 rods** (lane 2). Not better than the armour race in rods; one bot lived 6,720 ticks (2,680 the longest before).
- **Hits** (three lanes): fireballs 13, fire ticks 42, a blaze's touch 1 (5 in the armour race's first 300 ticks: the keep-off and the earlier line hold), a wither skeleton 1, a skeleton's arrows 2.
- **What a fireball costs**: 2.9 in armour and then 3 to 6 fire ticks of 1 that armour does not stop: 6 to 9 health a hit. Lane 2's second fight: one hit at tick 100, 20.0 to 9.9 by tick 200 with nothing else landing. Healing 13 health took 1,000 ticks and five porkchops (race 45337168104 lane 0, ticks 480 to 1,500): the kit's 11 raw meat is about 29 health. So a bot has about 45 health to spend on 14 kills, and a fireball that lands is a sixth of it.
- **The blazes multiply while the bot rests**: "none of N shooters sees the bot" went 3, 5, 7, 8, 12 in lane 1 between ticks 540 and 6,660. Its rests were at z 173 to 176, 14 to 17 blocks from the spawner at z 159: inside the 16 at which a spawner works. Kills came at 1 per 1,000 ticks; the spawner makes a batch every 200 to 800.
- **A cover cell that is no cover**: lane 2 walked to (127, 62, 165) eleven times in 220 ticks (ticks 2,640 to 2,860), each walk ending seen, and burned to death there; (131, 62, 173) and (127, 62, 165) are the tops of the bridge's low walls. Fixed here: a cell walked to and still seen from is not walked to again in that retreat (`Defend::cover_tried`).
- **Not fixed by these**: the cost of each fireball and the growing number of blazes. Next: wall 6 (below), then stepping out of a fireball's line (clojurecraft's `dodge.clj`: 24 of 212 hit; none that passed 1.5 blocks or more from the body), which `world::Entity::velocity` now allows.

### 6. A defend held on a far shooter while a melee foe strikes (race 45337951964, another agent's, lanes 0 to 2)
- **What happened**: "defend Blaze 338 (Fight)" from tick 2,840 on a blaze up to 18.9 over the eye; a wither skeleton hit at 2,880, 2,900 and 2,920 (20 to 4.6), "0 swings, 0 landed", and knocked the bot off the bridge. Lane 1 the same on blaze 341. Lane 0 held "defend Skeleton 393 (Fight)" on a skeleton 16.2 over the eye and fell off the bridge.
- **Read** (`lib.rs` `always`, `defend.rs`): `defend::foe` is asked only when no defend runs; a running one keeps the foe it began with. And the far-shooter choice takes any shooter within 16 blocks, whatever its height.
- **Fix**: (1) every tick, while its own foe is farther than `defend_radius` (or gone), a defend turns to the foe pressing the bot: the nearest hostile within `defend_radius`, else the nearest mob of a kind that hit the bot in the last 40 ticks (`defend::pressing`; note "defend switched to K N from …"), with the stance that foe gets; (2) a shooter whose feet are more than a strike's reach over the eye is not gone after from afar (`defend::foe`); (3) a shooter's fight with no step to take toward its foe (no floor, fire or lava ahead) or with the foe over the reach turns to the retreat to cover after 20 ticks (note "shooter fight given up: no way to …").
- **Unit tests**: `a_melee_foe_in_reach_presses_a_defend_on_a_far_shooter`, `a_shooter_far_overhead_is_not_gone_after` (plan/tests/foe.rs).
