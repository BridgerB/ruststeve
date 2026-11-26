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
- The bunker: four down, the top two cells laid back as the roof, the door a three-step stair; sealed on return, opened to leave.
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

### 2. Killed in the stock tunnels by a dragon fireball's cloud (race 45872058898, c4ae22f, dev-end-arrival, 2 lanes x 1 bot, cap 10,000)
- **What happened**: both bots did the pillar, the bridge, the island and the bunker as in wall 1; the covered rule fired ("end: covered from an angry enderman, no pocket": 5 and 8 notes, lane 1 4,126 ticks covered) and the stock went on: lane 0 to 43 end stone in 9 runs, lane 1 to 112 in 11. Then a dragon fireball splashed 3 off each bot in its tunnel ("particles_dragon_fireball_splash at 3", "dragon_fireball passed within 5.8 / 6.8") and the cloud took 20 to 0 in 60 ticks: lane 0 at (19, 59, -1), tick 6,200; lane 1 at (8, 59, -6), tick 6,840, "death.attack.indirectMagic ender_dragon". Lane 1's last pocket block went into (8, 61, -6), a cell over the tunnel's head: the tunnel ran one under the surface (feet 59, head 60, roof 61, the ground's top), and where the ground dips a block the head cell is open to the sky. 37 and 8 "provoked enderman" pockets were sealed where the tunnel was open so.
- **Cause**: one block of roof over a tunnel cut along the surface's top layer.
- **Fix**: the bunker is dug four down and the top two cells laid back: the stock tunnels run with two of end stone over the head, so a dip of one opens none. The door is a three-step stair (`bunker_cells`). The covered rule, the seal and the open are unchanged.

### 3. Two blocks of roof were not enough either (race 45876313549, 6fe2dc4, dev-end-arrival, 2 lanes x 1 bot, cap 10,000)
- **What happened**: lane 1 stocked 134 end stone in 12 runs (one stone pickaxe broken, 13 uses left on the next) and was killed at (19, 58, -12) at tick 7,900, again by a fireball's cloud ("particles_dragon_fireball_splash at 1", 20 to 0 in 60 ticks); lane 0 sat to the cap in issue 17 pockets (194 "provoked enderman" in the race against 17 "covered"): pockets mean the tunnel was not covered where they were sealed. The ground's top within 30 of the fountain runs 57 to 64 (E1): a level tunnel at a fixed depth under one site comes out under the sky where the ground falls away.
- **Fix**: the stock goes down `STOCK_DEPTH` 6 under the bunker's feet by stairs first ("stairs down under the bunker"), then level ("a level tunnel deep under the ground"); stairs are dry. At the bunker's y 58 to 59 the level tunnels run at about 52.
- Also built since wall 2 (c6d4006): the walk back to the bunker is not begun while an area cloud lies within its radius and 1.5 of a route cell (`end::cloud_on`, "end: the way to the bunker waits for a cloud at"), for E4's race 45875049490 lane 1.

### 4. Stocked, then no way back up (race 45878860222, c1cf39d, dev-end-arrival, 2 lanes x 1 bot, cap 10,000)
- **What happened**: lane 0 went down six and stocked **164 end stone** at y 52 in 13 runs with **no damage** (20 health at the cap; 15 "covered" notes, 8 pockets, all before the depth), "end: stock over" at tick 7,460. The hand-off to the crystals then wanted the bunker's stair: "end: no walk to the bunker" 23 times from (8, 52, 0) and "spikes: no way to (39, 59, 0)" to the cap. Lane 1 was killed at tick 1,380 digging the bunker's shaft at (14, 59 to 62, 0): a fireball aimed at the bot splashed 2 off in the open shaft before the roof went in.
- **Fix**: with no walk back to the bunker from under the ground, stairs up from where the bot stands toward the spiral's next heading (`Stairs::up`, dry, at most `OUT_RUNS` 8: "end: stairs up out of the tunnels from"), then the hand-off from the surface.
- **Open**: the shaft is open to the sky for about 150 ticks while it is dug; E5's global breath rule is to cover the cloud, not the fireball's aim.
- **Fix (the coordinator, wall 4's open item)**: the shaft is never open to the sky. A post of two blocks goes on the ground beside the shaft's top cell first; then four times a dig under the feet, the fall, and the cell over the new head laid at once (`Op::Roof`, on the post above the ground, on the shaft's side below; `end::shaft_ops`, `EndState::roofs`). Begun only with the dragon over `DRAGON_DIG_CLEAR` 64 off or perched ("end: the bunker waits for the dragon at", `dig_waits`). And every stock run begins and ends at the bunker: the stairs down first (their foot kept), then three level arms from the foot (on along the stairs' heading and to both sides), each run walked out to its arm's tip from the bunker, cut, and walked back ("end: stock run N over at", "end: stock back to the bunker", "end: back at the bunker after stock run N", `stock_back`); a run with no walk back ends the stock there ("end: no way back to the bunker from the stock at"). Tests `the_bunker_shaft_is_roofed_after_every_dig`, `a_flying_dragon_near_holds_the_bunker_shaft`, `every_stock_run_begins_and_ends_at_the_bunker`.

### 5. Pockets in the stairs shut the way home (race 45884609352, 451e50e, dev-end-arrival, 2 lanes x 1 bot, cap 12,000)
- **What happened**: both bots dug the roofed shaft (no damage in it) and cut the stairs down; then 499 "provoked enderman" in the race against 7 "covered": a stair's cells are three high, so the 2-high test failed in them, and issue 17's pockets laid 40 blocks in the stairs and tunnels. Both stocks ended after 2 runs "end: no way back to the bunker from the stock at", and the stairs up out of the tunnels fired 8 times. Lane 1 went on to E4's climb and a crystal gone; lane 0 was hurt on the surface, waited for a cloud on the way to the bunker ("end: the way to the bunker waits for a cloud") and was killed by the dragon's breath where it waited (E5's global rule is to step out of a cloud).
- **Fix**: `covered` is "under the end stone": a full block over the feet's column and over every open side column within the sky check, so the bunker, the stairs and the arms all count, and no pocket is sealed in them. A walk home or to an arm now needs a whole route (`found`: `route_now` takes a partial route for a goal walk, which wandered toward a shut cell).
