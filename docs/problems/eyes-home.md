# Issue 19d: the way home, from the warped forest to the overworld's surface (agent Q2)

The orchestrator's index is [`eyes.md`](eyes.md); this file is agent Q2's (the pearl top-up, the kit, the trip back, the approach and the enter, the climb and the food).

## Needs
A real bot resumed from the `pearls_12` frontier (the main warped forest, about (495 to 518, 44, 13 to 38): 12 pearls, 7 rods, a shield, iron armour, stone and wooden tools, food 10 to 11 and nothing to eat, so no healing) tops its pearls up to 14 (Bridger, 2026-11-25; bounded at 6,000 ticks), makes its eyes, carries wood, sticks, spare pickaxes and a table, goes back through its own entry portal (obsidian x 21 to 24, y 34 to 37, z -20, about 475 blocks west through the basalt deltas, on a ledge over the lava sea with lava in front of it) and comes up to the overworld's surface from the portal's side near the lava sea (about y -50), then eats to food 18.

## Done (world terms)
A case file from a race resumed at the frontier shows `stage reached: overworld` (Q1's stage: back in the overworld holding the eyes); then, for Home's own end, the ledger's "home: over, fed on the surface".

## Risks
- No healing on the whole trip: every hit is kept; ghasts, magma cubes and piglins in the deltas.
- The lava ledge: the row in front of the portal turned to lava (`nether-tunnel.md`, `nether-walk.md`).
- The cave band over the overworld portal killed 7 of 16 bots in issue 16 (`overworld-leg.md` wall 1).
- Pickaxe uses for a 100-block climb: about 300 digs.
- One world: one portal, one route, one cave band.

## What is built (branch `issue-19-q2-home`)
`EnderPhase::Home` and `crates/plan/src/home.rs` (brain note `way-home`):
1. **Begun** under the race with 12 pearls (and eyes) held in the Nether, from any phase (a resumed bot has none); or in the overworld at `pearls_12` or past it. "home: 12 pearls held, on to the way home".
2. **The top-up**: the hunt runs on to `[home] pearls_wanted` 14, or `top_up_ticks` 6,000 from Home's start. "home: pearl top-up over" (the count or the clock). The pearls-done row never runs in Home.
3. **The kit** (`base::HOME_STEPS`): eyes (Q3's `eyes` row when present, else `home-eyes`), stems to 16, a table placed with sticks, 2 wooden pickaxes with 40 uses, sticks to 8, a table carried.
4. **The portal**: `home::portal_far`, memory's `NetherPortal` index in 48 chunk columns round the feet, lowered to the bottom cell; else `[home] portal` (22, 34, -20), geography. Q1's probe 45686594347: the frontier snapshot's memory holds the portal, lit, at x 22 to 23, y 34 to 36, z -20, and the whole way from it to the forest (385 columns); the overworld portal at x 125 to 126, y -52 to -50, z -108.
5. **The trip**: the heading's legs to `[home] way` ((490, 150), (226, 194), (8, 81): the chain's route east reversed), then the portal's column, with the travel's rules (`EnderPhase::travels`). With a point the turns and fresh choices go toward it (`turn_toward`, `choose_toward`); the keep is 16 in Home and a leg refused only for filler restocks from the walls first (netherrack, basalt, blackstone).
6. **The approach** within 24: lava beside or under the portal's feet cells and the standing cells in front of its two lowest rows sealed with filler first (`home_seal`, at most 24); the plain walk into a lit cell (`Task::Enter`), else (only with no front known) a far walk beside it (bridging the lava), else a tunnel; a portal seen out is lit with flint and steel when held. The front (`portal_front`) is the face with a floored standing cell: the walk in starts only on its side, no far walk once a front is known, the tunnel goes to the cell behind the standing cell ((22, 35, -18) at the entry portal); the north face (no floor over the lava sea) is never chosen.
7. **`Task::Enter`** is done in any dimension but the one it began in.
8. **The overworld**: stairs up in rock to the open sky (`stairs_to_surface`), dry and in rock: past 4 from the climb's start a stair that would cut an open cell (a cave) turns its heading (`Stairs::rock`, "cave at"), stone pickaxes from the stairs' cobblestone (`base::CLIMB_STEPS`) when the stone uses fall under 30; on the surface a hut each night, the cook (`home_cook`, in the hut by night, before eating by day) when raw meat and the fuel for it are held, sticks from the warped stems counting, a log seen within 32 fetched by day when short, eat to 18, hunt, look for animals; then a rest toward 18 health for at most 2,400 ticks; "home: over, fed on the surface".

Unit tests: `crates/plan/tests/home.rs` (Home begun with 12 and not 11, both top-up ends, the eyes first, the far lookup past the loaded chunks, the trip aimed at the remembered portal, a lit portal walked into, the climb from under rock, fed on the surface is over and never back to the portal, meat eaten first) and `home::tests` (`Enter`'s done check both ways).

## Not built yet
- The bot's old surface stairs as the way up (memory holds no air; the climb is dry stairs only).
- A seal of lava faces met inside the approach's tunnel (its stairs refuse lava beside a cell instead); only the portal's own feet cells are sealed.
- Fuel past logs seen within 32: the cook burns sticks from the kit's warped stems, overworld planks, logs or coal; with none, the meat is eaten raw.

## Walls
1. **The trip stalls in the deltas** (race 45698618366 at 8624263, 4 lanes from the frontier). The top-up, the kit (12 or 13 eyes, 8 to 14 logs, 9 sticks, 4 wooden pickaxes, a table) and the portal lookup ((23, 34, -20) from memory, 493 off) all held, at 19.8 health. From tick 9,000 lanes 1 to 3 sat at x 448 to 465 in the basalt deltas, and lane 0 walked east to x 535. The legs straight at the portal found no walk, no tunnel route (one search: "6446 stairs down refused in the fortress footprint") and far walks of 44 to 50 bridges with 11 to 13 filler over the keep of 32; the exploring turns ran (-1, 0) and (0, 1), never (0, -1), and every fresh choice was (1, 0), east, away from the seen columns' centroid, which is the route back. Fix: way points on the chain's route east reversed, turns and choices toward the point, the keep 16 in Home, a restock for a leg's bridges, basalt and blackstone as filler.
