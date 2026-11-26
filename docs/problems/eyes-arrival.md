# Issue 19: the overworld arrival, from the portal to fed on the surface and the first throw (agent Q7)

The orchestrator's index is [`eyes.md`](eyes.md); this file is agent Q7's. The way home itself is Q2's ([`eyes-home.md`](eyes-home.md)); the climb's fixes here are in `crates/plan/src/arrival.rs`, called from fenced blocks (`// >>> Q7`) in `home.rs` and `lib.rs`.

## Needs
A real bot out of its own overworld portal (cells x 125 to 126, y -52 to -50, z -108; out by the south face, z -107), by the lava sea and under the cave band (y -57 to 33 round the portal, Q1's probe 45688112814), with food about 10 and nothing to eat (no healing), a table, sticks, warped stems and planks, a furnace, stone and wooden pickaxes and 14 eyes, climbs to the open sky, eats to food 18 or more, and makes its first eye throw.

## Done (world terms)
From the dev start `dev-overworld-arrival` (the frontier bot put at the portal's south face; development only, never evidence), 2 of 2 bots reach the surface, eat to 18 or more and throw their first eye, with deaths and damage counted.

## Risks
- The cave band: creepers, zombies, skeletons (overworld-leg wall 1: 7 of 16 dead).
- No healing until fed: every hit is kept.
- Pickaxe uses for a 120-block climb; the night on the surface; animals far from where the shaft comes out.
- One world: one portal, one column of rock, one surface.

## Walls

### 1. The climb by stairs is too slow, and its pickaxe run walks back to the portal (race 45727358847, 4fe8848, 2 lanes x 1 bot, cap 24,000)
- **What happened** (lane 0's ledger; lane 1 is the same world and did the same): from (125, -53, -107) at tick 60, "the climb begun"; by tick 12,000 both bots stood at (126, -19, -84), 34 blocks up, food 11 to 8, 19.8 health, no mob near. Each leg of six stairs took 620 to 840 ticks in deepslate (about 120 ticks a block of height: three digs a stair at about 23 ticks each with a stone pickaxe). At 4,620 "home: pickaxes for the climb (1)": "table skipped (a table near and 9 sticks held)", then "craft stone_pickaxe at the table (124, -53, -106)", the portal room's table 32 below: the bot walked back down its stairs, crafted, and from 5,380 to 6,440 its stairs from the portal room failed ten times (NoPath, floors of air) and once boxed by a cave; it was back at y -29 by 7,180. The second run (10,480) began the same way and placed its own table only after the far craft failed Unopened; that table was left behind.
- **Cause**: a stair costs three digs a block of height; the base's table rule counts any table within `TABLE_SEARCH` (64) as near.
- **Fix** (`arrival.rs`): a one-wide shaft straight up in rock: the cell over the head dug, a jump, a block put into the cell the feet left at the top of the jump (the pearls hunt's proven pillar). One dig a block, sealed below by its own blocks and walled by rock. Refused at a fluid in or beside the cell, a falling block in or over it, a cave beside or over it (unless the sky is over it), or a block too slow to break; then the stairs take a leg and the shaft is tried again where it ended. And the climb's pickaxe run refuses tables past `TABLE_REACH` (5), so the base places the carried one where the bot stands, and digs it back once the run is over. Ledger: "arrival: shaft up from", "arrival: shaft over at … (why; risen N in T ticks)", "arrival: no shaft from … (why)", "arrival: the table at … is far", "arrival: the table at … taken along".
- **Unit tests**: `crates/plan/tests/arrival.rs`.
