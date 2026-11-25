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

## Walls
(none yet)
