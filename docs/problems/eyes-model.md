# Problem: the eye of ender and the stronghold in the model and on a real server (issue 19a.5, 19g)

**Status: open** (agent Q5). The index of issue 19's walls is [`eyes.md`](eyes.md).

## What the eyes need
Q3's throw, read and triangulation (`eyes-throw.md`) and Q4's dig into the portal room (`eyes-stronghold.md`) are built on what an eye does and what the room is. Both are proven on a real server before any race (rows `y01` to `y04` in `config/moments.toml`), then in the model (the parity table agrees) before a closed-loop test or a sim number counts.

## Done, in world terms
- `y01-throw`: a thrown eye's line on the plane, read from the bot's own world, within 1 degree of the stronghold RCON locates; its end (dropped or shattered) recorded.
- `y02-fetch`: walking to the dropped eye picks it up.
- `y03-frames`: a bot about 100 blocks from the stronghold holds `end_portal_frame` cells in memory and the room's centre is computed from the ring.
- `y04-silverfish`: breaking into the portal room through a wall: silverfish woken, damage through iron.
- The model's rows match the real ones; closed-loop tests in `crates/bot/tests/eyes*.rs`.

## The jar (probe 45685968956, `javap -c -p` of the 26.1.2 server jar, on issue-19-eyes)
- **`EnderEyeItem.use`**: a look whose block hit is an end portal frame passes (a use there fills the frame: throw looking up). The target is `findNearestMapStructure(#eye_of_ender_located, player.blockPosition(), 100, false)` (the tag holds only `minecraft:stronghold`) as a lower-corner point: the same point `locate structure minecraft:stronghold` prints. None found (the Nether) consumes the use: no eye, nothing spent. Else an `EyeOfEnder` at the player's x, `getY(0.5)` (feet + 0.9), z, its delta zero; one eye spent.
- **`signalTo`**: beyond 12 on the plane the eye's own target is 12 along the line and 8 up; within 12 it is the structure's point itself (it goes down toward it). `surviveAfterDeath = nextInt(5) > 0`: 4 in 5 drop.
- **`tick`**: the position moves by the old delta with no collision (`setPos`: it flies through terrain); then `updateDeltaMovement`: horizontal speed `lerp(0.0025, speed, distance left)` along the line to the target (speed and vertical speed times 0.8 within 1 of it); vertical speed `vy + (s - vy) * 0.015`, s = +1 while below the target's y, else -1. Life counts each tick; past 80 (the 81st) it is discarded: an item entity of the eye where it was (`new ItemEntity(level, x, y, z, stack)`), or level event 2003 (the shatter).
- So **every place an eye takes lies on the one line from its spawn to the stronghold's point**: the read is exact up to the client's position quantisation, and the bar is the error of the client's read, not of the flight.
- **The stronghold** (RCON `locate`, race seed `typecraft`): `[1648, ~, -16]` from the origin, from landings 0 to 7 and 9, from the overworld portal (113, -105) (1,537 away) and from the old stairs (55, -133); `[-1088, ~, -1728]` from (-240, -240). The structure set: concentric rings, count 128, distance 32, spread 3.
- **The portal room** (`StrongholdPieces$PortalRoom`, piece-local, 11 by 8 by 16): the shell (0..10, 0..7, 0..15) of stone bricks, the feet's level y 1 on the y 0 floor; frames at y 3: (4..6, 3, 8), (4..6, 3, 12), (3, 3, 9..11), (7, 3, 9..11), each with an eye 1 in 10; a lava pool (4..6, 1, 9..11) under the ring in a platform (3..7, 1, 8..12); lava (1, 1, 1..3) and (9, 1, 1..3) in ledges (1..2, 1, 1..4) and (8..9, 1, 1..4); stairs up to the south at z 4 to 6 to y 3; the silverfish spawner at (5, 3, 6), the stairs' top; the door (grates) at (4, 1, 0); iron bars at y 3 to 4 along the side walls.

## The model (19g)
- **The eye's flight** (`crates/sim/src/eye.rs`): the rules above, the stronghold's point told to the model (`Sim::with_stronghold`; a parity row's `stronghold`, from `locate`). Its place is sent every 4 ticks (the entity type's update interval). Tests: `crates/sim/tests/eyes.rs`, `crates/bot/tests/eyes.rs`.
- **The synthetic stronghold** (`crates/sim/src/stronghold.rs`): `stronghold_world(radius, ground_y, origin, corridor)` lays the template's room (piece-local origin, door to the north) and a corridor north of the door in flat rock and returns its `Room` (frames, centre, floor_y, lava, spawner, door, infested). For Q4's tests of frames found, the dig to the floor, never over the pool, infested bricks never dug.
- **Row plumbing for y03 and y04**: a landing may carry its own capture `radius`; a row may ask for `frames` (the class: frames memory holds and the ring's centre) and wear `armor`; script `break:<finish>:<dx>:<dy>:<dz>` digs a block beside the feet.
- Not modelled: the frame fill (`useOn`), the eye's sounds and particles, the item entity's push out of a block when the eye ends inside terrain.

## The real rows (parity 45688030470, fe985fc): 173 of 178, eye rows 5 of 8 matching
- **The line is exact.** All 8 real eyes (landings 0 to 5 and 8; both strongholds) flew the line to the located point within **0.0006 degrees** (the client's read: first place to the farthest, positions told every 4 ticks); the model's within 0.0000.
- **The flight** (real and model alike): from the player's middle (feet + 0.9), **12.151 on the plane and 8.18 up** at its farthest, reached about 34 to 38 ticks after the throw, then it hovers there; it is removed 81 ticks after it was added. So a throw gives a bearing only, never a distance (more than 12 off).
- **The end**: 7 of 8 dropped, 1 shattered (y01-throw-2): the jar's 4 in 5.
- **Pickup**: y02-fetch-0 and -4 walked to the drop and picked it up, as the model did (their cells differed by a block or two: where the walk stops is the drop's random kick; the fetch now walks home after).

## The real portal room (probe 45690314001, the parity seed)
- The stronghold located at (1648, -16) has its portal room's **12 frames at y 19**, the ring's cells x 1626 to 1630, z -36 to -32 (centre cell (1628, 19, -34)); the **spawner at (1632, 19, -34)**, 4 east of the ring's centre. The located point is 20 blocks east and 18 south of the ring: the bearing leads to the stronghold, not to the room.
- From the template: floor y 16, feet level y 17; the spawner east of the ring puts the door at x 1638 (piece z 0), the side walls at z -39 and -29, the shell x 1623 to 1638.
- **Rows** (landing 10 over the ring, captured 7 columns each way: 8 is the view distance's circle and never sends its corners): `y03-frames-0` and `-1` stand still 100 and 60 west; the class is the frames memory holds and their centre (expected 12, (1628, 19, -34)). Memory watches frames only from issue-19-q1-observe (Q1 owns `crates/memory`); before that merge both sides read 0. `y04-wall` stands in a stone pocket outside the side wall at (1625, 17, -28), full iron, normal, an iron pickaxe, and breaks the wall at the head into the room: the real side's evidence of silverfish and their damage through iron; it differs in sim until silverfish are modelled.
- For y03/y04: a row's `feet_y` (new, optional) sets the start's feet height when it is not the surface (a pocket beside the room's wall).

## Walls
1. **A dropped eye can land out of reach** (y02-fetch-5, parity 45688030470). The eye ended 8 up over a tree; its item came to rest on the canopy at y 76, the bot's feet at 72: the pickup box reaches about 2.3 over the feet, so the bot stood under it for 250 ticks and never took it. The model's fell to the ground that run, by chance: the drop's sideways kick is random (up to about 5 blocks of drift), and in parity 45691653069 fetch-5's real eye missed the tree while fetch-4's landed on one 9 over the feet, real (y 73) and model (y 71) alike: the model holds items on leaves. The fetch script now gives up an eye resting on a tree (leaves or a log: parity 45694311575's real eye lay on (252, 76, 198), 5 over the feet, and the leaves-only rule did not give it up: the block under it is taken to be a log, unconfirmed) and walks home, so the row's cell is its start in both. For Q3: a throw from under or beside trees, or near a cliff edge, can lose an eye that did not shatter; throw from open ground with no block over the 12-block point on the line, and give up a drop over the feet + 2 (clojurecraft's rule: picked up only within 2 of the feet's height).
