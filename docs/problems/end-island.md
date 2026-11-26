# Issue 20e: the End's arrival and the island (agent E3)

The orchestrator's index is [`end.md`](end.md); this file is agent E3's (`crates/plan/src/end.rs`, the void edges in `walk.rs` and `home.rs`). Brain note: `brain/ruststeve/design/end-island.md`.

## Needs
A bot arriving on the End's platform at (100.5, 49, 0.5) crosses the void to the main island, digs a roofed bunker in the end stone about 12 from the fountain, eats and heals there, and stocks about 160 end stone for the crystals' pillars (20f); then hands over to the crystals (E4) and the dragon (E5).

## Done (world terms)
From the `end` frontier (or `dev-end-arrival` for development only), the ledger shows "end: on the island at", "end: bunker dug at" and "end: stock over" with 160 or more end stone; the stage `island` is reached (feet on end stone within 30 of the fountain) and snapshotted.

## Risks
- The void: a fall from the bridge is a death. Guards on both sides always, sneaking the whole way.
- The dragon: it strafes and charges at a bot on the bridge, and breaks the cobblestone it touches (only end stone, obsidian, iron bars and bedrock are dragon-immune, E1's jar read). The bridge is cut only while it is far (48) or perched.
- Endermen everywhere: every look goes through `keep_look`; unasked looks go straight down.
- One seed, one End: the gap is void at x 73 to 97 and the island's lip is at y 52 (x 68 to 72); every number here is this seed's.

## Built (branch `issue-20-e3-island`)
- The pillar on the platform up to `BRIDGE_FLOOR` 52, then the bridge west along z 0 onto the lip's top (E1: a bridge at floor 48 runs under the lip into the island's side at x 63 to 65).
- Guards (`home::guard_next`) on both sides of every bridge cell; the dragon wait; the progress invariant (1,200 ticks without a cell).
- A cliff of end stone ahead: `Stairs::up` west through it.
- The bunker: three down, the roof laid back from a side block, the door a two-step stair; sealed on return, opened to leave.
- Retreat under 14 health, held to 18; eat under food 18.
- The stock: `Stairs::level` runs in a square spiral from the bunker to 160 end stone.
- The void as an edge for walks, rails and the idle sneak, in the End only.
- Unit tests: `crates/plan/tests/end.rs`.

## Not built
- The bridge's rails and floor are of whatever bridge block is held (cobblestone on arrival): end stone comes only from the stock.
- A walk on the island is not cut when the dragon charges; only the bridge waits.
- The hand-offs are stubs until E4's `spikes_task` and E5's `dragon_task` land.

## Evidence so far
- E6's parity e04 (gaze, an enderman 12 east): a look 3 degrees aside of its eye angered it, 6 degrees aside left it calm, straight down (pitch 90) left it calm: the cone at 12 is about 3.7 degrees, as the jar says; looking down by default holds.
- E6's e02: the arrival is (100.500, 49.000, 0.500).

## Walls

### 1. Held in a pocket in its own tunnel (race 45869900923, 0900b8c, dev-end-arrival, 2 lanes x 1 bot, cap 6,000)
- **What happened** (lane 0; lane 1 the same): the pillar (4 up by tick 100), the bridge from (100, 53, 0) to the lip at (72, 53, 0) in 600 ticks: 27 floor blocks, 58 guards and rails, 0 dragon waits, 128 cobblestone down to 39 (about 3.3 blocks a cell). "end: on the island at (72, 53, 0)" at tick 720; the walk to the site (12, 62, 0) in 380 ticks; "end: bunker dug at (14, 59, 0)" at 1,240 (three down, roofed, 10 ops). Stock runs 1 to 3 cut 6 cells each in 460 ticks (12 end stone a run, about 38 ticks a block). At 2,600, in run 3, enderman 215 (hurt 40 to 35 while the dragon flew within 15) turned creepy: "provoked enderman 215: into a sealed pocket", and from 3,480 four more did the same; the pocket was held to the cap, 44 end stone. **No damage to the bot** (20 health the whole run), no near miss, no look kept off.
- **Cause**: issue 17's provoked-enderman pocket answers any angry enderman, but the bot was already in a roofed 2-high tunnel no enderman can stand in. The endermen were angered while the dragon was near and hurt them, not by a look.
- **Fix**: in the End, in a roofed 2-high cell whose open sides are 2-high and roofed too (`end::covered`), no pocket is sealed and the work goes on ("end: covered from an angry enderman, no pocket", every 600 ticks; `EndState::covered_ticks`). A fenced addition at the top of `Plan::ender_threat`. On the open island the pocket is as before. Test `in_a_roofed_tunnel_an_angry_enderman_seals_no_pocket`.
- **Open**: blocks: the bridge's 89 of 128 left 39; the real frontier holds 116 cobblestone, 64 cobbled deepslate and 150 netherrack.
