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

### 7. Every fireball is stood under (races 45337168104 and 45338756013, six bots)
- **What happened**: 13 fireballs hit three armoured bots in race 45338756013 and lit 42 fire ticks: 80 of the health lost there, against one blaze's touch, one wither skeleton's blow and two arrows. In a fight the bot stands still facing its foe whenever the foe is in reach, and walks straight at it when not.
- **Read**: each hit follows "small_fireball passed within 2 to 5" lines at 20-tick steps; a blaze's three shots leave 6 ticks apart and arrive 6 to 15 ticks after they appear. The world keeps a fireball's place and, when the server sends it, its velocity (`world::Entity::velocity`). Nothing in the planner reads either.
- **Cause**: no dodge. clojurecraft has one (`dodge.clj`: of 212 small fireballs 24 hit, every hit passed within 1.4 blocks of the body and none of 76 passing 1.5 or more away did).
- **Fix** (`defend.rs`, a shooter's Fight only): each small fireball within 30 blocks whose line passes within `DODGE_CLEAR` 1.5 of the body's middle, and is not flying away, starts one dodge: `DODGE_TICKS` 8 of walking across its line, to the side the line leaves the body on (`dodge_side`; the other side when no safe step lies that way, `safe_step`), the strikes going on. A fireball with no velocity sent yet is taken to fly at the body. Counted in the fight's note: "dodged N fireballs".
- **Not known**: whether the server sends a small fireball's velocity before it lands (`add_entity`'s own velocity is not decoded). The count of dodges against hits in the next race says whether the guess of "at the body" is enough.
- **Unit test**: `a_fireball_s_line_is_stepped_out_of` (plan/tests/foe.rs).

### Walls 6 and 7 raced (847d33c: the real spawner snapshot, race 45343420833, 2 x 1; and `dev-spawner-fed`, 32 cooked beef on the healthy development snapshot, race 45343424571, 1 x 1; cap 12,000)
- **Real snapshot**: 2 of 2 dead (600 ticks, fire; 3,000 ticks, a fireball), 1 rod (lane 1), 2 blazes gone. **Lane 0 never swung**: "defend switched to Blaze 2 from Blaze 1 … Blaze 1 from Blaze 2" six times a tick. My bug: `pressing` took "a kind that just hit the bot" to include the foe's own kind, so two blazes were turned to in turn and the swing's facing tick never came. Fixed below (a pressing foe is never a shooter).
- **Fed** (development only): 2 rods, 3 blazes gone, 25 swings and 22 landed, 7 cooked beef eaten, dead at 4,080 ticks by a fall. Hits: 8 fireballs, 39 fire ticks, 3 ticks standing in fire, 1 blaze's touch: about 65 health for 3 kills. So with food taken away as the limit the fight still does not win: each time out it lands 2 to 4 blows, takes one fireball, and goes back to cover.
- **Firings**: "defend switched to" 103 and 220 (the flip above, almost all of them); "no way to" 11 in the fed lane (blazes 3 to 4.6 under the eye, off the side of the bridge, chosen again at once because a shooter within 4 whose kind has hit is answered though ignored: fixed below); dodges 3, 5 in the real lane and 45 in the fed one, 15 of them with a velocity sent (so the server does send a small fireball's velocity, about one time in three before the dodge is chosen).
- **The fall**: the fed bot's cover cell was (127, 62, 165), the top of the bridge's rail (it was "in cover" there four times); at tick 4,040 a fireball hit it there and it fell. Fixed below (`defend::edge`: no cover cell with a fall beside it).
- **Healing is slow even fed**: ticks 3,400 to 3,600, 25 cooked beef held, health 9.0 to 11.2: the bot eats to heal only under food 18, and at food 18 to 20 with no saturation health comes back 1 in 80 ticks. Wall 9.

### 8. Four blows a blaze (all races)
- **What happened**: a kill takes 4 landed blows or more (9 kills for 32 landed in race 45337168104; 3 for 22 in the fed lane, the blows spread over several blazes), each 14 ticks apart: 56 ticks a blaze at best, as long as its 60-tick wind-up.
- **Read**: an iron sword's blow is 6 and a blaze has 20. A blow struck while falling is a critical one, 9: three blows.
- **Fix** (`defend.rs`, a shooter's Fight only): the bot jumps `CRIT_LEAD` 7 ticks before its swing is ready and swings on the way down; a swing ready while still rising waits for the fall at most `CRIT_WAIT` 8 ticks; no jump on a cell with a fall beside it. Counted in the fight's note: "N swings falling". Judged by landed blows a kill in the next race.

### 9. Health comes back slowly with food to spare (race 45343424571)
- **What happened** and **read**: above ("Healing is slow even fed"). `eat::eat_to_heal` fires under food 18 only. Cooked beef gives 12.8 saturation, and with saturation a point of health comes back every 10 ticks.
- **Fix** (`eat.rs`, one rule): hurt under the engage line and holding a food whose saturation is at least its hunger points, the bot eats at any food bar under 20. Raw meat (3 and 1.8) keeps the old line of 18, where eating it wastes nothing.

### Fixes to walls 3 and 6
- A pressing foe is never a shooter and never a `never_fight` kind (`defend::pressing`): the switch is for a melee mob arriving under a ranged fight.
- An ignored shooter within `defend_radius` waits like any other ignored one, whatever its kind did (`defend::foe`): a blaze under the bridge was fought, given up "no way" and chosen again every 80 ticks.
- A cover cell with a fall beside it is not walked to (`defend::edge`). Any fight's close-in stays out of fire (`fire_ahead`; the model's fire arena, probe 45343436986: a defend on a wither skeleton and the fire escape cut each other every tick, 20 to 2.3 health with no swing).
- **Model arenas** (another agent's `crates/bot/tests/nether_fight.rs`, probe 45343436986 at 071c346): `the_melee_foe_in_reach_is_fought_under_a_blaze_out_of_reach`, `a_fight_on_a_one_wide_bridge_never_ends_in_lava` and `a_fight_on_an_open_bridge_never_ends_in_lava` pass and are no longer ignored; `a_fight_never_stands_in_fire` failed (the foe lived) and stays ignored until the next probe.
- **Unit test**: `a_cell_beside_a_fall_is_an_edge` (bot/tests/blaze_fight.rs).

### 8bd7212 raced (real spawner snapshot of race 45343411709, race 45344605924; lane 0 read)
- Lane 0: 4 sorties in 4,340 ticks, 11 swings and 11 landed (10 of them falling), **0 kills, 0 rods**, 9 fireballs and 20 fire ticks, dead to a fireball at (127, 61, 188) while taking cover. "defend switched to": 0 (the flip is gone). Each sortie: 160 to 240 ticks, 2 or 3 blows, 6 to 9 dodges, 8 to 12 health, then 600 to 1,100 ticks healing.

### 10. Sorties from the bridge do not win, fed or not: the fight has no post
- **What happened**: across six races the fight is a sortie. The bot idles on the bridge 7 blocks south of the spawner and 3 below its deck, walks 5 to 10 blocks at the nearest blaze with 5 to 7 others in sight, lands 2 to 4 blows in 160 to 240 ticks (the rest is closing in, dodging and stepping off), takes a fireball (6 to 9 health with its fire), and leaves for 600 ticks or more, during which the blazes it wounded are joined by new ones. Fed with 32 cooked beef: 65 health for 3 kills (race 45343424571). On the real kit: 0 kills in 4 sorties (above).
- **Read** (probe 45344729907: `javap -c` of `net.minecraft.world.entity.monster.Blaze$BlazeAttackGoal.tick` in the 26.1.2 server jar): with its target seen and within 2 blocks (`distanceToSqr < 4.0`) a blaze touches every 20 ticks and moves at it; seen within its follow range it only charges (60 ticks), fires (6, 6) and rests (100), with no move; unseen it moves at the target only while `lastSeen < 5`. `Blaze.customServerAiStep` lifts it toward the target's eye height and no more. So a blaze stays where it spawned, a spawner spawns within 4 blocks of itself, and nothing comes to a bot in cover (race 45338756013 lane 1: 3,000 ticks out of sight with 7 to 12 blazes holding at 8 to 14 blocks).
- **Cause**: the fight has no place. The spawner's deck (x 127 to 131, z 157 to 161 at y 64, fenced) is within a strike of every cell a blaze can spawn in; a blaze reached inside its 60-tick wind-up dies before it fires (three falling blows are 28 ticks); the fence stops falls. The bot fights everywhere but there.
- **Fix**: the fight post. `defend::post_cells` (pure, seen blocks): the deck's standable cells, no fire, no fall beside them. A fight with a blaze, a spawner seen within 24: off the deck the fight walks a found route to the deck cell nearest its foe (note "fight post at … from …"); on the deck no step leaves it. Between fights the spawner task stands the bot on the deck (one fenced block in `spawner_task`, note "fight post at …"). A fight that lands nothing in `shooter_patience`, or has no way to its foe, ends with the bot's health whole and the next blaze is taken (note "shooter left for the next"); the foe waits 600 ticks as any foe never struck does.
- **The leave rule**: unchanged and never under the danger line: at or under `shooter_retreat_health` (13: the danger line of 8 and one fireball's fire) the fight is a retreat to cover on a found route, and the spawner task's leave-and-heal takes it from there. The walk on and off the deck is always a route (two full steps at z 162 and 163).
- **Fire on the deck**: a post cell is never a burning one, no step of the fight enters fire (`safe_step`), and the fire escape, an always-row ahead of the defend, moves a body that touches fire at once.
- **Rods**: a rod on the deck is within a step of every deck cell; the spawner task's pickup (8 blocks, a found route) runs before the post walk whenever no fight is on. Counted from "item blaze_rod" lines.
- **Judged by**: kills and rods per 1,000 ticks against the sorties (0 to 1 kill per 1,000 ticks tonight), health lost per kill.
- **Unit test**: `the_fight_post_is_the_spawners_deck` (bot/tests/blaze_fight.rs).

### The whole-stage model (another agent's `crates/bot/tests/nether_rods.rs`; probe 45347050872 at ffc97e2, the post in)
- Raw kit: no death in 12,000 ticks, 3 kills, 0 rods, 46 health lost, 8 meals. 30 cooked beef: dead at 6,160, 8 kills (1.30 per 1,000 ticks), 78 fireballs fired and 19 hit, 150 health lost (18.8 a kill), 15 meals. With a shield: dead at 8,300, 11 kills, 196 fired, 24 hit, 26 blocked. No rod held in any (the model's blazes hold their columns, most off the deck).
- The model's health a kill (18.6 to 18.8) matches the real fed race (19). So the model is fit to judge a change in minutes.

### 11. A defend walks back into the fire the escape left (race 45343411709, another agent's; the model run above, ticks 6,100 to 6,160)
- **What happened**: "escape-fire (trapped in fire; cut defend)" and "defend Blaze 86 (Retreat) (… cut escape)" begun in turn several times a tick stamp, health 5.7 to 0 in 60 ticks in the model; 14 escapes and 18.5 to 0 in 220 ticks in the real race.
- **Read**: the fire trap is in force only while the body's box touches a fire block (`escape::touching_fire`), and it is not held. One tick the escape steps the body off; the next the trap is gone, the defend begins again and its walk (a route to cover, a flee, a close-in) takes the body back through the burning cell.
- **Fix** (`defend.rs`): no step a defend sends goes into fire: after every stance's choice, a forward step with fire in the cell 0.9 ahead is held (`fire_ahead`; counted in the fight's note, "N steps held from fire"). A route through fire then stalls and the retreat looks for another cell.

### The post raced (334644e: real snapshot of 45343411709, race 45346896371, 2 x 1; fed development, race 45346899542, 1 x 1)
- **Real**: lane 1 alive at the cap, **3 rods**, 4 blazes gone, 18 swings / 14 landed, at 13.4 health with nothing left to eat (1 rod and 1.1 health at 8bd7212); lane 0 dead at 1,780 to a blaze's touch with 0 swings. **Fed**: dead at 3,220 to a blaze's touch, 1 rod (6 rods alive at 8bd7212).
- **Firings**: "fight post at" 2, 5 and 8; "shooter left for the next" 1, 0, 1.
- **Hits**: touches 3, 5 and 9 (1 or 2 a lane before): on the deck the blazes are within 2 blocks. Fireballs 3, 2 and 10.
- **Model** (probes 45348139250 to 45348337350 at ffb8459 / 80b34f1, 30 cooked beef, one run each, so a direction and no more): all parts on, dead at 3,960 with 5 kills; the dodge off, 7 kills in 2,340 ticks at 6.9 health a kill, then dead on the deck; the keep-off off, alive to 12,000 with 11 kills; the retreat line at 9, dead at 2,680. Deaths come fast on the deck with little health lost before: touches.

### 12. A blaze in touch is walked away from, not struck (race 45346899542, ticks 3,120 to 3,220; 45346896371 lane 0)
- **What happened**: "fight post at (128, 64, 161) from (128, 61, 165)" with blaze 1 "facing at 1": "hit mob_attack by blaze" at 3,120, 3,140, 3,160, 3,180 and 3,200, 17.8 to 1.7, through the walk to the post and then a retreat to cover three blocks away, with one swing in the 100 ticks. Lane 0 of the real race: 3 touches and 3 fireballs, 0 swings in the whole run.
- **Read**: the jar (wall 10): a blaze within 2 blocks moves at its target and touches every 20 ticks. The defend swings only in its Fight stance with its own foe in reach and the body standing: not while it walks to the post, not in a retreat.
- **Fix** (`defend.rs`): whatever a shooter's defend is doing, a mob of the foe's kind in reach (by its box, in sight) is swung at when a full swing is ready and the weapon is in hand (`strike_in_passing`; no neutral mob in the sweep). Counted in the fight's note: "N in passing".

### 13. The retreat line with nothing to heal with (spawner wall 7's other half)
- **Read**: since spawner wall 7 the spawner task takes no rest that cannot heal (no safe food held, the food bar under 18). The fight still turned to a retreat to cover at 13: a bot at 9 to 13 with nothing to eat would be handed from the fight's retreat (cover) to the spawner task (no rest: to the post) and back, with nothing coming back.
- **Fix** (`defend::retreat_line`): a shooter's retreat line is `shooter_retreat_health` while a rest can heal (`plan::can_heal`), else the danger line of 8: the bot fights on with what it has and never under the danger line. Counted in "shooter fight given up: health … (line N)" and in fights begun under 13. Unit test `with_nothing_to_heal_with_the_blaze_line_is_the_danger_line` (`crates/plan/tests/foe.rs`).

### The whole-stage model at 0ee129a (probe 45350488562; the post, strike in passing, spawner walls 6 and 7)
- Raw kit: dead at 4,680, 9 kills (1.92 per 1,000 ticks), 5.5 health a kill. Shield: dead at 8,080, 18 kills (2.23), 11.8 a kill. 30 cooked beef: dead at 11,480, 19 kills (1.66), 15.2 a kill. Sorties were 0.5 to 1.3 kills per 1,000 ticks at about 19 health a kill.
- **Rods 0 in every run**: not the bot's. The model's blaze drops nothing: `data::generated::loot::MOB_DROPS` has no blaze row (its rod pool is conditioned on a player's kill, and the datagen keeps unconditioned drops only), so `sim::life::drop_loot` spawns no rod. For the model's owner. Kills are the model's measure until then.
- Two of three deaths at (128 to 129, 61, 176): spawner wall 8 (the leave walk met at once), fixed in 4323494.

### 14. Hurt, no cover found, and back to the post (the model above, raw kit, ticks 4,440 to 4,620)
- **What happened**: a fight turned to a retreat at 12.7; "fight post at (129, 64, 161)" 20 ticks later; at 4,620 health 2.9 and "leave the spawner … to heal"; dead.
- **Read**: `take_cover` returns nothing when it finds no cover cell, no leave route and no shelter to build; `spawner_task` then goes on to the post walk, which asks nothing of health.
- **Fix** (`spawner_task`): hurt under `engage_health` with a way to heal and a spawner within 24, the bot does not walk to the post (or to the spawner) but waits where it is, the eat row healing it (note "no fight post: hurt, no cover found", once per 600 ticks).
