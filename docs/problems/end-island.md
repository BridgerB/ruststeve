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

## Walls
(None yet.)
