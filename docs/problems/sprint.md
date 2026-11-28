# Problem: every move is a walk (issue 56, sprint)

**Status: measured; the machine is in progress** (S1 measure wrote this file; S2 to S6 append their own sections). The winning lineage of issue 20 took 466,498 ticks from spawn to the credits, almost all of it travel at the walking pace (4.317 b/s). Sprint is 5.612 b/s, and the deaths from moving too slowly (creepers keeping pace with a retreat, wither skeletons, the End's dodges) are the same gap.

## What the bot needs
- A body that sprints exactly as a 26.1.2 client does: the speed, the jump, the rules that start and stop it, and the packet that tells the server, in the right place in the tick.
- A hunger budget that counts what sprinting costs, so sprint never takes the food under the line where it stops working (6) or where the bot must eat.

## Risks
- A sprint the client simulates and the server does not believe: the server never checks the speed (below), so a wrong model shows as no correction and goes unseen. The parity rows are the only check of the body.
- A sprint over an edge or into lava: the policy (S5) refuses it there.
- Food: one point every 40 metres of flat sprint, one every 20 sprint-jumps.

## Done, in world terms
A real bot sprints along a flat floor at 5.612 b/s, sprint-jumps, stops sprinting on a wall it walks into and on food 6, and is never corrected; the same rows in the model give the same outcome classes.

## Measured (S1, probe 46115253148, `javap -c -p -constants` of the 26.1.2 server and client jars)
Every number is in `config/sprint.toml` with its class and method; the prose is `brain/ruststeve/design/sprint.md`.

| What | Value | Source |
|---|---|---|
| Sprint modifier | `minecraft:sprinting`, 0.30000001192092896, `add_multiplied_total` (id 2), transient, on `movement_speed` | `LivingEntity.<clinit>`, `LivingEntity.setSprinting` |
| Sprinting speed | 0.1 × 1.3 = 0.13; steady 0.2806 a tick, 5.612 b/s (walk 0.2159, 4.317) | `Player.createAttributes`, `LivingEntity.getFrictionInfluencedSpeed` |
| Air acceleration | 0.025999999 sprinting, 0.02 walking | `Player.getFlyingSpeed` |
| Sprint-jump | `+(-sin(yaw) × 0.2, 0, cos(yaw) × 0.2)`, `Mth`'s sine table, yaw × 0.017453292f | `LivingEntity.jumpFromGround` |
| Start | sprint key, forward impulse > 1e-5, possible, not using an item that forbids it, not crouching (unless under water), not fall flying (unless under water) | `LocalPlayer.canStartSprinting` |
| Possible | no blindness; food > 6 (or `mayfly`); not in shallow water unless flying | `LocalPlayer.isSprintingPossible`, `Player.isMobilityRestricted`, `FoodData.hasEnoughFood` |
| Stop | not possible, no forward impulse, or a horizontal collision that is not minor (input against collided move under 8°, 0.13962634 rad) | `LocalPlayer.shouldStopRunSprinting`, `isHorizontalCollisionMinor`, `Entity.move` |
| Not a stop | sneaking, using an item, releasing the sprint key | `LocalPlayer.aiStep` |
| Sprint hit | knockback hit: velocity × (0.6, 1, 0.6), sprint off; a sprinting hit is never a critical | `Player.causeExtraKnockback`, `Player.attack`, `canCriticalAttack` |
| When decided | each tick before the body moves (`AbstractClientPlayer.aiStep` comes after), from the last tick's collision | `LocalPlayer.aiStep` |
| Packet | `player_command` id 42: entity varint, action varint, data varint; START 1, STOP 2, data 0 | `ServerboundPlayerCommandPacket.write`, `$Action.$values` |
| Order a tick | `player_input` (on change), `player_command` (on change), the move | `LocalPlayer.tick`, `sendPosition`, `sendIsSprintingIfNeeded` |
| Input bit | sprint 64, kept by the server, never read (only shift is) | `ServerGamePacketListenerImpl.handlePlayerInput` |
| Server truth | shared flag 3, bit 0x08 of entity data index 0 | `Entity.setSprinting` |
| Server checks | none: the command is taken as sent; moves refused only over 100 squared beyond expected per packet, or 0.0625 squared off the server's sweep | `handlePlayerCommand`, `handleMovePlayer` |
| Exhaustion by distance | on the ground horizontally: sprint 0.1 a metre, walk 0, crouch 0; swim, eye in water, in water 0.01 a metre; in the air 0; whole centimetres per move | `ServerPlayer.checkMovementStatistics` |
| Exhaustion by jump | 0.05, sprinting 0.2 (when the client leaves the ground going up) | `ServerPlayer.jumpFromGround`, `handleMovePlayer` |
| Exhaustion to food | over 4.0: one saturation point, else one food point (not on peaceful); capped at 40 | `FoodData.tick`, `FoodData.addExhaustion` |

What this changes in the plan:
- **Hunger** is one point per 40 metres of flat sprint, not "0.1 food per 40 metres". A 1,000-block sprint is 25 points: 12.5 shanks, which saturation pays first.
- **Sneaking is not a stop rule** in the game. The agreed `physics::sprint_allowed` refuses a sneak anyway; that is stricter than the game and safe (the bot never asks for both), but a parity row must not expect a sneak to stop a real sprint.
- **Using an item is not a stop rule** either: eating while sprinting keeps the flag (the input is slowed by the item's use speed). Only the start is refused.
- **A sprint hit ends the sprint** and costs 40% of the speed: a retreat that swings must start the sprint again (S5).
- **The server never corrects a wrong sprint**, so the parity rows (S3) and the jar read are the only evidence the body is right.

Not read: the client's side of a sprint hit (`MultiPlayerGameMode.attack` running `Player.attack` locally), swimming's speed (`Player.travel` in water), and `AttributeInstance.calculateValue` (the standard `value × (1 + amount)` is assumed for `add_multiplied_total`).

## S3: physics and parity

### Inertia before the move (fixed)
The sprint-jump test measured 8.756 b/s against vanilla's 7.127 (CI 46118649784). The cause was older than the sprint: a tick's horizontal inertia was taken from where the body ended the move (clojurecraft's `landed`), not from where it stood before it (`LivingEntity.travelInAir`). Fixed in 653ac072; see [[inertia-before-the-move]]. The jump and ledge rows now also compare the final position within 0.25 blocks (05a491d3), so an error of this kind can no longer hide inside a block cell.

### m00-3: a hurt's motion lands a client tick early or late (open, not physics)
**The last recorded failure:** parity 46121838127 on 8643d925 passed 219 of 239. m00-3 differs:
- real ended at (-34.065, 69.420, 0.5): the first tick of a jump from y 69;
- sim ended at (-34.879, 70.249, 0.5): the top of a jump, about 5 ticks further on.

On main (parity 46123165102) the row matched by cell, with real at -37.167 and sim at -37.531: a 0.36 gap that the cell comparison hid.

**What the real recording shows.** These come from `replay export` of `m00-3.real.rsr` (probe 46124185859). Ticks are script ticks, events are the recording's.
- **Tick 34, the first fall.** The bot lands at y 78.0 after falling 8 blocks. The damage event (ev 2378) and the server's `set_entity_motion` for the bot (ev 2389) both arrive before tick 35 (ev 2404). The motion zeroes the horizontal velocity, and tick 35 jumps from it whole (y 78.42).
- **Tick 65, a second fall.** The new inertia made this fall 4.25 blocks, from 78.25 to 74.0; on main the fall here was 3.25 and did no damage. The damage (ev 3519) arrives before tick 66, but the motion (ev 3533) arrives after it. By then tick 66 has jumped, to (-16.639, 74.42).
  - The motion sets the velocity to (0, -0.078, 0), so tick 67 moves 0.0196 in x (the air input alone) and falls to 74.342.
  - The jump is lost. The body lands at tick 69 and waits out the 10-tick jump cooldown. That is the 5-tick lag at the end.
- **Main's run did the opposite at its first fall.** It landed at tick 35, and the motion (ev 2377) came after tick 36 (ev 2361).
- No column was missing, no position was sent twice, and no block changed near the path. Real reported clean: no teleport.

**The cause.** The client takes the server's `set_entity_motion` for its own body as vanilla does: it sets the velocity, so a jump the server has not yet seen is lost.
- A real server sends the motion at the end of the server tick that handled the landing move. Whether it reaches the client before the client's next tick or the one after depends on where the two ticks' phases fall. Here that was once early, twice late, and once split, with the damage early and the motion late.
- The model (`bot::lockstep::run_acting`) always delivers the motion late. It sets `motion_due` while it handles the move, sends the motion from the next `In::Tick`, and the bot reads it only after its next tick.
- So any row that takes fall damage and jumps on the next tick has a timing coin-flip in real that the model cannot match. The inertia fix moved m00-3's path onto a second hurt fall, which exposed it.

**Proposed, for the lead to choose:**
1. **Write m00-3 down as the table's timing row**, alongside y04's silverfish, and keep TOLERANCE at the count that holds. This needs no code.
2. **A start with fall damage off for the `jump:` m-rows**, so no hurt motion meets a jump. This changes their health class, so it is a parity-table decision (moments.toml and the real side's gamerule, in moments.rs).
3. **Not proposed:** delivering the motion early in the model. That would trade m00-3 for main's case, and the real server does both.

Physics needs no change: the real body moved exactly as the client physics predicts, tick by tick, between the motion packets.

## Lead: walls found in the races

### Wall L1: a retreat that circles the same creeper never ends (progress-invariant gap)
- **Seen:** no-sprint arm race 46121815604 (8643d925), bot rs-l06-b00. A creeper stood by the bot's furnace. For about 2,200 ticks (31080 to 33300) the bot went round a loop: `smelt: collect 7 IronIngot` (a batch was left cooking), then `defend Creeper 7897 (Retreat)`, then collect again, six times over. Each retreat ended with 0 swings and the creeper still in reach of the furnace. On the sixth return the fuse lit at 3 blocks and the explosion killed it.
- **Structural cause:** the retreat and the work it cuts have no shared progress invariant. Each retreat counts as a success ("defended", back to the goal), and each return to the same furnace counts as fresh work. Nothing counts the returns to one threat, so the loop has no end. This is the class of iron-kit walls 21, 28 and 29: `flee_clear` holds work off for a while, but the work comes back to the same spot while the creeper is still there.
- **Needs:** a retreat from a `never_fight` kind, repeated around the same entity and the same work site, must end the loop. After N returns, the work site is given up (a new furnace elsewhere), or the bot waits out of sight until the creeper is gone. N comes from data, and every firing writes a ledger note.
- **Decision:** fixed in #56 only if the sprint arm does not clear it. Sprinting widens the gap on each retreat, but it does not end the loop.
- **Done, in world terms:** in the races, no bot dies to a creeper after more than two retreats from the same creeper.
||||||| 8744ed3b

### The cost of the vanilla inertia rule, by ledger lines (probe 46129430056)
Every creeper retreat (`defend Creeper … (Retreat)`) in four 8 × 14 iron-kit races from `start=stone`:

| arm | races | creeper retreats | fuse lit during the retreat | died in the retreat |
|---|---|---|---|---|
| baseline, old inertia (f2408e5) | 46115072696, 46115074754 | 94 | 14 (14.9%) | 1 |
| no-sprint, vanilla inertia (8643d925) | 46121815604, 46125605993 | 152 | 21 (13.8%) | 10 |

A creeper reaches fuse range about as often in both arms. What changed is getting away once the fuse is lit: 13 of 14 escaped under the old rule, 11 of 21 under vanilla. The old rule's longer hops outran the 30-tick fuse, but that was an unearned edge the server never checked ([[inertia-before-the-move]]). Sprinting is how vanilla wins the distance back. The sprint arm is judged against this table, by the same probe.
||||||| 8744ed3b

## The pace policy (S5, `plan::sprint`, `config/sprint.toml` `[policy]`)
Which moves sprint, as a closed `sprint::Pace` read from the task in hand; the gate (`Plan::pace_gate`) runs last in `Plan::step`.

- **Ask**: walks (every task's walk), explores, retreats and the escape's move hold the sprint key. A retreat from a `never_fight` kind always asks.
- **Food**: a sprint needs a point to spend over `hungry_below` (saturation first), one point per 40 metres (`metres_per_point`). A `never_fight` retreat skips this; the body's floor of 6 still holds.
- **Ground**: no sprint in water (no swim model) or lava, nor toward or along a lava edge, a missing floor, the void's edge, water, awkward footing or an unloaded cell, `look_ahead` (2) blocks along the facing.
- **Path**: bridges, sneaks at an edge and stairs or travel tunnels walk; a fight's close-in walks.
- **Counted**: "sprint begun, <pace>" once a task, "sprint refused, <reason>" once a task for each reason and "sprint restarted, <n>" on each later start (a stop holds `hold_ticks`, doubled per stop up to `max_doublings`, and until `clear_blocks` from where it stopped), in the race summary's notes by kind; `Plan.pace` keeps the run's sprint and refused ticks.

Unverified until a race: the sprint's effect on waypoint following at corners (0.28 a tick against `WAYPOINT_REACH` 0.35), and the first CI and race counts of each note.

### S5 walls found by the sprint (issue 56)
- **A reach walk's route ended where it could not arrive** (probe 46126047959): `Goal::Near` counts height in its range, the reach arrival allows the block at most 2 over the feet; the planner test's walk stood under a log 3 up and turned on the spot until NoProgress. Fix: `Goal::Reach`, one rule (`path::reach_rise`) for the route's goal and the arrival.
- **Open: explores go back and forth** (CI 46126753035; old main behaviour): an explore's `Goal::Afar` ends anywhere far enough from its start, so two targets can alternate. A turn away from ground explored (cfc2d366, `explore_walk`'s check in `choose`) was tried and reverted: with every heading spent it waited, and `bot::wood` ran out of its 6,000 ticks (CI 46130367510). It needs a fallback that explores farther, not a wait.
- **Open: a log only a pillar reaches.** A tree's log 3 or more over the only ground beside it has no `Goal::Reach` route (the arrival needs it at most 2 over the feet), and `stairs_to_log` cuts stairs only for a log roofed below. Such a log is refused and the planner looks for another; a pillar or stairs-up finish is a feature of its own, not issue 56's.
- **A log too high to reach hid its tree** (CI 46128712142 against main's 8744ed3b, probe 46129792904): with `Goal::Reach` the log at (189, 75, 24) has no route, and `choose` refused it with a 6-block area, which hid the ground-level log (191, 72, 25) main reached at tick 1293. Fix: a log with a `Goal::Near` route and no `Goal::Reach` one is too high only; it is refused alone, noted "log too high to reach". Perception was not the cause: logs are read from the memory's exposed sightings, not the gaze.
