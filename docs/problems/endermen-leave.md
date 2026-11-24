# Issue 17c and 17e: leave the rods, recover, and stand near endermen safely

The orchestrator's index is `docs/problems/endermen.md`; this file is agent E3's (the leave, the recovery, the endermen task, the stare filter, provoked endermen).

## Needs
A bot resumed from the frontier `rods_7` snapshot (at the blaze spawner (129, 64, 159) with blazes round it, 7 rods, iron armour and sword, 2 stone pickaxes of unknown wear, about 620 filler, one raw porkchop at most, no table or wood) leaves the spawner alive, keeps every rod, travels on, and when a ticking enderman is within 32 blocks holds there safely: sealed into a 2-high pocket, eyes down, never having stared at or struck an enderman.

## Done (world terms)
- No frontier run ends `met` at the rods: every ledger has "rods held: on to the endermen" and one "provisions" note.
- A run that reaches `endermen` shows "endermen reached", its "pocket block" notes and "endermen met" (or "endermen met unsealed" with its reason).
- In races near endermen: "look kept off enderman N" counted, "near miss" 0, and no provoked enderman that did not hit first.

## Risks
- The snapshot starts in the fight: the frontier run that made it walked away at 9.2 health. Food is one raw porkchop: little healing is possible (spawner wall 7).
- A stare by accident: walking straight at an enderman 32 off at pitch 0 is 1.7 degrees under its eye, inside the jar's cone (2.3 degrees at 32). Every look must be filtered.
- A provoked enderman in the open: 7 a hit (about 4 through iron), anger for ever, teleports in from beyond 16. A pocket takes about 10 blocks at one per 10 ticks.
- Pickaxe uses are the scarce resource (at most 262, no table, no wood).
- Thin fortress floors over the lava sea (clojurecraft #715).
- One world: every frontier race is the same seed, fortress and heading.

## What was built (branch `issue-17-e3-endermen`)
- **17c.1** `spawner_task`'s rods branch keeps the leave walk (`Goal::Away` 50 by the route, `LEAVE_TRIES` 12) and then, instead of `met`, notes "rods held: on to the endermen" and hands over to `Plan::endermen_task` (`crates/plan/src/endermen.rs`); `nether_task` goes straight there once `ender.left`. `met` is set only at the stage: a ticking enderman within 32 ("endermen reached"), then the pocket (`cover::pocket_block`: the hut, the walls nearest a threat first, the roof last), then "endermen met"; the pitch is held at 90 after.
- **17c.2** Recovery: health under `travel_health` 14 with a ticking blaze or ghast seeing the body: `take_cover` ("recover before travel"); with no rest to take, "travel hurt". At or under `danger_health` with nothing to heal with: "hold in cover: nothing to heal with" and a pocket. The eat row already eats raw porkchop to heal (not in `unsafe_foods`; `eat_to_heal`).
- **17c.3** One "provisions" note before the first leg: pickaxe uses left (`endermen::pickaxe_uses_left`) against `pickaxe_reserve` 40, filler against 32, rods, food points, the food bar, health. Rods are never fuel (`smelt::fuels_held` burns coal, planks and spare sticks only) and no recipe in the goal table uses them. The search-tunnel fallback is not taken at or under the reserve ("no tunnel: the pickaxe reserve").
- **17c.4** Inside the fortress's brick footprint (bounds of the bricks on record within 8 chunk columns, padded 4) a plain walk out first ("out of the fortress footprint"): a walk never cuts stairs. The tunnel's own "stairs down" inside the footprint is E2's (`tunnel.rs`); `endermen::fortress_bounds` and `in_footprint` are there for it.
- **17e.1** Approach: a ticking enderman within 64, else a frozen one seen within 128, walked to within 26 by the route's measure (plain route, else the far walk; no tunnel), "approach enderman N"; given up after 3 walks or 6,000 ticks ("enderman N given up").
- **17e.2** Never stare: `look::stares` is the 26.1.2 jar's `isBeingStaredBy` (probe 45472254333: the enderman's eye point only, dot over `1 - 0.025/len`, a collision-shape clip, no carved pumpkin, range 64). `Plan::step` passes every tick's controls through `look::keep_off`: a staring look keeps its yaw and takes the first pitch of 90, 60, 30, 0, -30, -60, -90 that stares at nothing. Only full blocks seen exposed stop the line here (the safe side). Notes: "look kept off enderman N" (every 200 ticks at most, with the run's count) and "near miss: enderman N stared at" when its stared-at flag (index 18) is seen set.
- **17e.3** Endermen are never a foe (`defend::NEVER_FOE`, in `foe` and `pressing`). A provoked one (creepy flag, index 17, or an enderman hit within 400 ticks) cuts in after the always-rows: "provoked enderman N: into a sealed pocket", the pocket where the bot stands, eyes down, "held from enderman N" every 600 ticks, "enderman N calm or gone" after.
- **17e.4** Waiting: standing on warped nylium (a block seen; E1's feet biome joins it) with none near, "wait in the warped forest" up to `forest_wait_ticks` 1,800, then "forest wait over". Simplified: the bot waits where it stands, and a provoked enderman gets the pocket there, not a pocket kept a step away.
- **The order after recovery** (the orchestrator's decision of 2026-11-24, `docs/problems/endermen.md`: the warped forest is 454 blocks east behind basalt deltas, a soul sand valley 32 west of the spawner): the approach; a warped forest known from blocks seen (E1's `Memory::forest_known`, stubbed `None`), walked to ("to the warped forest at …"); the wait on warped nylium; a soul sand valley known from soul sand and soul soil seen (E1's `Memory::soul_valley_known`, stubbed `None`) walked to ("to the soul sand valley at …": a plain route, the far walk, the tunnel above the reserve; 3 tries, then "… given up"); stood in (on soul sand or soil, or in the known column), a **nook** built of held filler (the pocket with the side toward the most open floor left open, `cover::nook_block`, `open_runs`; never open toward a ticking ghast or skeleton while another side is open, `endermen::nook_side`): "nook at … open to (dx, dz)", then "waiting in the soul sand valley" every 1,200 ticks, up to `valley_wait_ticks` 12,000 ("valley wait over"); the fortress footprint; the heading. Filler, not digs: 600 blocks against 144 pickaxe uses.
- **17e.5** The heading: `endermen_task` calls issue 16's `explore_tunnel` in a fenced block until E2's `heading_task` lands.

Stubs, one place each, to swap when E1 lands: `endermen::ticking` (= `world::ticks`), `endermen::near_ticking` (= `Stage::Endermen`), `endermen::in_forest` (adds `feet_biome`).

## Tests
- `crates/plan/tests/endermen.rs`: the stare (a look at its eye, a dig whose look crosses its head, beyond 64, a pumpkin), ticking at the chunk edge, the stage at 32, provoked by flag or hit, never the foe, provisions counts, the footprint.
- `crates/bot/tests/endermen.rs` (closed loop, a Nether cavern of the model): seven rods go on (no "rods met", a provisions note, the fallback heading); a ticking enderman 20 off is met in a sealed pocket; one 44 off is walked to; a provoked one gets the pocket and no swing; on soul sand the bot builds a nook open away from a ghast and waits in it.

## Walls
(none yet: the first frontier race is below once the hand-over lands)
