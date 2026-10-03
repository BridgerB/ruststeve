# Nether phase — design note (2026-09-28)

Status: measurement + design only (per the playbook). Harness: `scripts/nether-harness.sh` — a kitted
`rust-nether-NNN` bot on a 7×7 netherrack pad 120 blocks WEST of the real fortress on Server B
(`locate` → fortress at nether **48, ~, 176**), `RACE_GOAL=blaze`, telemetry in `data/race.db`.
Server B: `view-distance=6` (96 blocks), `simulation-distance=4`, `difficulty=peaceful` in
server.properties (the harness sets `normal` at runtime; the overworld portal work so far ran on
peaceful — the measurement race must state which).

## Runs

| run | bot | start | outcome | where it ended / why |
|---|---|---|---|---|
| 1 | rust-nether-001 | 19:24 pad (-72,70,176) | stopped 19:28, 0 progress | 3 sweeps, moved 3 blocks total; probe (DIM=the_nether): solid netherrack at (-67..-64, 69..71, 176) — the sprint walks into a wall and the footing check only handles GAPS, never solid ahead. Telemetry archived in `data/nether-run1.db`. |
| 2 | rust-nether-001 | 19:29 pad (-72,40,176) | stopped 19:30 — **7 deaths in 90 s**, 0 progress | every sweep sprinted east off the pad and fell into the lava sea (deaths at y 15–30, x −54..−61); after the first death the `lava underfoot` bail fires but the next sweep repeats it. The footing check looks one block ahead at foot level — useless for a drop. Telemetry `data/nether-run2.db`. |
| 3 | rust-nether-001 | 19:31 pad (-72,88,176) | died at (0.2, 64, 176.7) within ~60 s of arrival | the pad WAS built (probe: netherrack at -72,87,176) but the bot's client world had not streamed when the step loop started (`feet-block=air solid_in_11x7x11=0`, NETHER_START's 20 s wait timed out with `feet=None`) — it sprinted 72 blocks east through a void local world and fell. **Arrival = wait for real chunk data, not a timer.** |

(2 more runs to add; death sites snapshotted with `scripts/snapshot.sh`.)

## What `find_fortress` does today (port of steve)
Scan for nether-brick blocks within 64 (direct block scan, no line-of-sight); if none, sprint along
±X with a footing check (bridge one cobble over a gap; bail if lava underfoot), 45 s per call, flip
direction after 180 blocks. `kill_blaze` then walks to the spawner and melees. Combat proved in an
arena (blaze killed, rod dropped) — the open question is TRAVERSAL.

## Idea A — tunnel at a fixed Y instead of surface sprinting
The bot's best skill is dig-down-and-tunnel (`strip_tunnel`, `descend_step`, lava-ahead checks).
Nether fortresses generate with their corridors mostly in y≈48–80 and the lava sea surface at y=31.
A 1×2 tunnel at a fixed Y (proposal: **y=64**, above the sea, in the band where fortress bridges
and corridors intersect terrain) along ±X:
- immune to ghasts (enclosed), cliffs (no falls), and open lava (the ahead check sees it before
  stepping; a lava cell ahead is capped with cobble and the tunnel continues);
- turns traversal into the same "terraform to a template, run a fixed script" shape as the portal;
- cost: netherrack breaks in ~0.4 s with an iron pick → ~3 blocks/s with the 2 blocks per column →
  100 blocks ≈ 70 s. A fortress is typically within 200–300 blocks of a portal.
Direction: keep ±X (fortress bounding boxes are long along their own axis but the `locate`
distance is what matters; ±X first because that's what the code already assumes), flip after
~250 blocks, then ±Z. Better: use the CHUNK SCAN below to choose the direction.

## Idea B — find the fortress from loaded chunk data, not line of sight
The client already holds every block in the loaded chunks. With `view-distance=6` the bot sees
±96 blocks in x/z at all heights. Scanning loaded chunks for `nether_bricks` / `nether_brick_fence`
/ `nether_brick_stairs` (and `spawner`, the real target) detects a fortress up to ~96 blocks to
either side of the tunnel line without ever surfacing — effectively a 192-block-wide search swath.
Cost: a full-column scan of 13×13 chunks is ~2.7M cells; do it per NEW chunk as chunks load
(`BotEvent::ChunkLoad` → scan that chunk's sections for the brick palette entries — a palette check
per section is O(1) when the section has no brick at all). Implementation sketch: in the chunk
ingest, keep a `HashSet<ChunkPos>` "has nether brick" flag from the section palettes; `find_fortress`
reads it, picks the nearest flagged chunk, and tunnels toward it. This also gives `kill_blaze` the
spawner position directly (`spawner` in a palette) instead of the 32-block scan.

## Arrival through a real portal (untested)
The natural bot that entered (rust-race-002) was killed by the driver 4 s later. Needs: wait for
chunks (the `NETHER_START` path already polls for a solid block under the feet — reuse), portal
cooldown (don't step back in), immediate ghast exposure (tunnel down/in from the portal at once).

## Pearl decision (arithmetic)
12 eyes of ender is the safe budget (a stronghold portal has 12 frames, on average ~1.4 pre-filled;
triangulation throws ~4–8 eyes with ~20% break chance). Eyes need 12 blaze powder (6 rods) + 12 pearls.

**Piglin bartering (this version, post-1.16.2 table):** ender pearl weight 10 / 459 = 2.18 % per
ingot, count 2–4 (mean 3) → 0.065 pearls per gold ingot → **12 pearls ≈ 184 ingots** expected,
i.e. ~420 nether gold ore at ~0.44 ingot/ore (2–6 nuggets), plus smelting. Deterministic but far too
expensive for a bot that mines ore by ore. Rejected.

**Endermen:** drop 0–1 pearl (mean 0.5) → ~24 kills. Two viable venues:
1. **Warped forest (nether)** — endermen spawn in numbers there; the bot digs a 2-high, 1-wide
   alcove, endermen (3 tall) can't enter, the bot hits at the opening. Needs a warped forest within
   reach (biome check via the chunk data — same scan machinery as Idea B) and the sword.
2. **Overworld at night** from the same alcove near the portal exit. Always available; slower
   (spawn rate), and the bot must survive other mobs — the alcove handles that too.
**Decision: endermen, warped forest first, overworld alcove as the fallback.** Same mechanic, same
code; the chunk scan decides which. Skeleton step `get_pearls` (steps.rs) gates on 7 rods.

## Skeleton after kill_blaze (in steps.rs now)
`gather_blaze_rods` (7) → `get_pearls` (12) → `craft_eyes` (12) → `find_stronghold` → `enter_end`
→ `kill_dragon` → `exit_end`. Bodies return "not implemented"; the last three gate on world flags
(`stronghold_found`, `in_end`, `dragon_dead`) that arrive with their tasks.

## Harness 2026-09-29 08:52 — measured (3 bots, 25 min, normal difficulty, kit: iron armor/sword/shield/pick, 16 food, 128 cobble)
- **Finding the fortress is solved by the loaded-chunk scan (Idea B):** 3/3 bots located fortress brick from their pads at 80–108 blocks, no line of sight, and pathed onto the fortress within ~1 minute. The tunnel sweep (Idea A) was never needed for *finding* from 120 blocks.
- **Getting there safely is NOT solved.** One bot fell from the open route into the lava sea at the same spot three times — (−11.5, 28, 182), right after `reached fortress near (8,61,151)` — `!! in lava — escaping` ×3, dead each time. Consistent with ghast knockback over the sea. **Decision: approach the fortress ENCLOSED** — even when the brick is in view, tunnel (Idea A mechanics: 1×2, cap lava, fill floor) toward it at the pad's height instead of a pathfinder walk over open terrain; switch to walking only on fortress brick.
- **Finding the spawner:** works by scan when it is within view (one bot found (129,64,159) from 74–75 blocks); two bots idled at the fortress edge with the spawner ~125 blocks away → `kill_blaze` now explores toward the farthest fortress brick 30–60 blocks off (built 09:00).
- **Combat at the spawner:** both arrivals at (129,64,159) died beside it (blazes, normal difficulty). Next harness run adds `KIT_FIRE_RES=1` to separate fire damage from melee reach/positioning.
- **SDK bugs found by this harness (both fixed):** login did not set the dimension height and a dimension change did not clear loaded columns → nether blocks read 64 too low for bots that log in already in the nether.

### Rate table — harness 08:52 (race.db, `scripts/rate-report.sh`, stopped at 16 min)
| bot | find_fortress done | kill_blaze reached | min in kill_blaze | deaths | rods |
|---|---|---|---|---|---|
| rust-nether-001 | 0.0 min | 0.6 min | 15 | 0 | 0 |
| rust-nether-002 | 0.0 min | 1.1 min | 15 | 0 | 0 |
| rust-nether-003 | 0.0 min | 1.7 min | 3 (back in find_fortress) | 9 | 0 |
3/3 reached the fortress and the blaze step within 2 minutes; 0/3 got a rod. Blockers in order: spawner out of view (2 bots idle), open-route falls into the lava sea (bot 3: 9 deaths, same spot), deaths beside the spawner.

### Harness run 2 (09:09, KIT_FIRE_RES=1) — combat notes
- First blaze killed on real terrain: explore → spawner by scan (129,64,159) → 75-block walk → engaged at 2.4 → **killed in 25 swings** → no rod (≈50% drop).
- **25 swings for a 20-hp blaze** (iron sword ≈ 6 per full-charge hit → ~4 landed hits) means ~80% of swings missed or hit i-frames: the bot fights ~16 s under fire per kill. Next: swing only when the blaze is within 3.0 of the EYE and on the crosshair ray, and back off/shield between.
- The same bot then **died beside the spawner** with fire resistance active — so not fire; fireball impact / wither skeletons (normal difficulty). The log has no hp trace; next: log hp + nearest hostile per engagement.

### Phase 2 harness summary (5 runs, 08:48–10:35; run 4 cut by the Mac sleeping)
| stage | result |
|---|---|
| find the fortress (loaded-chunk scan) | **solved** — every bot, every run, from 80–108 blocks, <2 min |
| reach it alive | open route: 9+ falls into the lava sea at one spot → enclosed tunnel approach built (all branches since 09:11); one later fall just west of a pad |
| find the spawner | scan finds it within ~96 blocks; fortress exploration added for when it is out of view; path to it often fails → tunnel toward it (built 10:20), a cave gap still blocks (`nether step … blocked` log added) |
| kill a blaze | **2 kills on real terrain** (25 and 8 swings), 0 rods (≈50% drop) |
| survive the fight | died at 0 hp in one kill → mid-fight break-off at <10 hp works (survived at 10 hp in run 4) |
Open for the next session: bridge-edge falls, spawner pathing across cave gaps, rod pickup after a kill, combat hit rate.
