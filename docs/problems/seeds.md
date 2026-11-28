# Issue 69: seeds (one world's win, measured on many)

The seeds are data in [`config/seeds.toml`](../../config/seeds.toml). The `xtask` commands are `xtask seeds`, `xtask seed <name>` and `xtask server --seed <value>`. The brain note is [`brain/ruststeve/design/seeds.md`](../../brain/ruststeve/design/seeds.md). This file holds the seed-coupling audit (agent 69-config). Each seed's stall stage is appended below it as a wall (lead-69).

## Needs
Issue 20 won on one lineage of one world, `typecraft`. Every frontier, every fix and every measured number in `docs/problems/end*.md`, `eyes*.md` and `pearls*.md` comes from that world's caves, Nether, stronghold and End. Before a seed other than `typecraft` can be raced from spawn and its stall read as a bot problem, not a fixture problem, everything fitted to `typecraft` has to be known. Each item must be marked either as something the bot reads (on another seed it is wrong, so it is a wall) or as a development or test fixture only (harmless on another seed, or simply not used there).

## Done (world terms)
- A bot on any seed in `config/seeds.toml` plays from spawn using only what it sees. No coordinate, bearing or block layout measured on `typecraft` steers it.
- Every fixture fitted to `typecraft` is either named for that world (a start or landing whose comment says "the race seed") or refused off it. For example, 69-relay's race.yml refuses any start besides `spawn` on a seed other than the race seed without a snapshot.
- The relay's table compares seeds stage by stage, and each seed's furthest stage is a bot wall, not a fixture.

## Risks
- **Development knowledge in the bot.** Bridger's rulings allowed "steering by the world's known geography to win" (2026-11-24) and treating "the End's layout as fixed by the seed" (issue 20). Both are true only on `typecraft`. On another seed the same values steer the bot toward places that hold nothing: no forest, no portal, no bridge, no stronghold, a spike of another height. That costs more than steering by nothing, because the bot trusts the value over what it sees until a fallback fires.
- **Silent wrong data, not a crash.** None of these values is checked against the world, so the race reads them as a slow or stuck stage and puts the blame on behaviour. An end reason will not show them. The ledger lines that name them will (`toward [1648.0, -16.0]`, `known geography (travel_way ...)`).
- **The fix is not this issue's.** These sit in crates/plan, config/goals.toml and config/end.toml. Plan code is fenced (issue 56) or not 69-config's to change. This file names them so the lead can queue them, one per stall.

## Seed-coupled

### Walls: the bot reads them, and they are wrong on another seed
Line numbers are at `main` f2408e5d. Each wall gives:
- **Where:** the file and line.
- **Read by:** the planner path and the stage that reads it (the step from one `world::Stage` to the next).
- **Off typecraft:** what the bot does on a seed where the value does not fit, read from the code.
- **Shows as:** the ledger note (task, then reason) that would show it happening, so a seed's stall stage and reason (`results seeds`) can be matched to a wall.
- **Needs:** what the bot would need to find the same thing itself, by sight (a block seen with an exposed face) or by the eyes of ender, with no X-ray.

None is fixed here. They are the generality backlog for after #69.

1. **`plan::arrival::FOOD_POINT` = (1648, -16)**, the `typecraft` stronghold's `locate` point.
   - **Where:** `crates/plan/src/arrival.rs:130`; the bearing is taken at `:637`.
   - **Read by:** `Plan::arrival_food_walk`, at stage `pearls_12` → `overworld`. That is the way home's arrival on the overworld surface, by day, hungry (food under `HAND_OVER_FOOD` 6), with no prey in view.
   - **Off typecraft:** each call walks one leg of `FOOD_LEG` 48 on the bearing from the feet to (1648, -16), or that bearing turned by up to 60 degrees. It does this up to `FOOD_LEGS` 24 legs, about 1,150 blocks. From an overworld portal near spawn, that bearing is always east. The walk does not depend on the world: it goes east into ocean, desert, mountains or anything else. A hunt still fires on any animal it passes, so the cost is a walk in a fixed direction, not a certain stall. After 24 legs it gives up.
   - **Shows as:** `arrival: a walk for animals to (x, y, z)` / `leg n of 24, bearing B (turned T) toward [1648.0, -16.0]; food …`, then `arrival: no walk for animals` / `leg n of 24: no leg on any bearing` at a shore or wall. The stall would be at `overworld`, hungry.
   - **Needs:** legs out from the portal on bearings chosen from what it has seen. That means grass, leaves or animals on record, or failing those, new ground in turn round the portal (a spiral of legs, with a progress invariant).
2. **`[pearls] travel_way` = [[440, 60], [488, -111]]**, travel points to the `typecraft` Nether's nearest warped forest (`locate biome`, probe 45529738504).
   - **Where:** `config/goals.toml:483`; `Survival::travel_way` at `crates/plan/src/make.rs:365`.
   - **Read by:** the heading, `Signs::way` in `crates/plan/src/heading.rs`, at stage `endermen` → `forest` (the travel from the endermen pocket to a warped forest).
   - **Off typecraft:** until a warped forest is known from warped blocks seen, the heading's legs aim at (440, 60), then (488, -111), with detours and the usual leg turns. A forest seen on the way replaces them at once. Soul sand valleys come only after the travel points. Past both points the heading falls back to its compass axis. On a Nether where those points are lava sea or deltas, the legs bridge and detour toward them until the progress invariant or `no-progress` ends the run.
   - **Shows as:** `heading to the travel point (440, 60) on (dx, dz)` / `known geography (travel_way; Bridger 2026-11-24); 0 of 2 reached; health …`, then `travel point reached: (440, 60)`. The stall would be at `endermen`, with these notes and no `heading to the warped forest at column …`.
   - **Needs:** a Nether search of legs out from the pocket or portal on bearings not yet walked, with a progress invariant, until warped nylium or warped stems are seen exposed. The "forest known from blocks seen" path already exists; only the search to replace the points is missing.
3. **`[pearls] hunt_point` = [490, 5]**, "where the hunt goes first", a spot inside the same `typecraft` forest.
   - **Where:** `config/goals.toml:605`; `PearlRules::hunt_point` at `crates/plan/src/pearls.rs:82`, used at `:3055`.
   - **Read by:** the pearl hunt, before the first burrow, at stage `forest` → `pearls_6`.
   - **Off typecraft:** before any burrow, the bot walks far toward (490, ~, 5), up to `HUNT_POINT_WALKS` 24 walks or `HUNT_POINT_TRIES` 3 walks with no route, until it is within `HUNT_POINT_NEAR` 16. Only then does it hunt where it stands. On another seed that point is in whatever Nether lies there, often hundreds of blocks from the forest just found, and the walks lead out of the forest.
   - **Shows as:** `to the hunt point` / `(490, 5), D off; walk n`, `no way to the hunt point` / `… try n of 3`, then `hunt point: here` / `D from (490, 5); n walks, m with no route`. A large D at `hunt point: here` means the hunt began far from the point, possibly outside the forest. The stall would be at `forest`, with no pearls.
   - **Needs:** the spot picked from memory, for example the warped nylium on record with the most nylium round it, away from basalt and lava. With `hunt_point` unset (`None`) the bot already hunts where it stands, which is a fallback today.
4. **`[home] way`, `portal`, `bridges`**, the frontier lineage's own Nether route home: 31 way points across the deltas, the entry portal at (22, 34, -20), and its two bridges at z 194.
   - **Where:** `config/goals.toml:865`, `:869`, `:874`; `HomeRules` at `crates/plan/src/home.rs:46`, `:48`, `:52`.
   - **Read by:** `Plan::home_trip` and `home_bridge` (`crates/plan/src/home.rs`), at stage `pearls_12` → `overworld`, from the forest back to the entry portal.
   - **Off typecraft, `way`:** the trip's aim is the first way point not yet passed, then the next, and only after all 31 its portal. A bot resumed with the eyes made skips to the nearest. So a bot on another seed walks the `typecraft` route's 31 points, from (477, 152) to (24, -11), through that seed's Nether, with the heading's legs and bridges, before it turns to its own portal.
   - **Off typecraft, `portal`:** `home_portal` reads memory first (`portal_far` within `portal_chunks` 48 chunks). The geography value (22, 34, -20) is used only when memory holds no portal, so a bot that lit its own portal is not misled by it.
   - **Off typecraft, `bridges`:** near one of the old bridge lines (within `BRIDGE_NEAR` 8 of z 194, x 201 to 400, y within 10), the bot restocks guard filler and walks "onto the old bridge" as if its blocks stood. On another seed that is void, lava or rock.
   - **Shows as:** `home: the trip begun toward the portal at (…) (memory|geography)` / `… 31 way points`, then the heading's `heading to the travel point (477, 152) …` / `known geography (travel_way; …)`. The home trip shares the heading's `Signs::way`, so the note names `travel_way` although the point is `[home] way`. Then `home: on the way` / `at … , D from the portal`, with D not falling. The bridges show as `home: onto the old bridge at …` and `home: up onto the old bridge at …`. The stall would be at `pearls_12`, `no-progress` or a death on the way.
   - **Needs:** a trail recorded in this run: the bot's own positions every so many blocks on the way out, and the bridges it laid, kept in memory and walked back in reverse. The portal is already in memory as a `nether_portal` sighting.
5. **`[end_kit] way_up` = [1608, 63, -32] and `way`**, eight points: the frontier bot's own stairs and tunnel from the `typecraft` portal room at (1620, 17, -36) up to the surface, from race 45800967549 lane 4's ledger. `EndKitRules::default` (`crates/plan/src/end_portal.rs:107`) holds the same values.
   - **Where:** `config/goals.toml:1081` and `:1085`.
   - **Read by:** `Plan::end_kit_climb`, `end_way_down` and `end_kit_down` (`crates/plan/src/end_portal.rs:850`, `:1950`, `:1860`), at stage `portal_room` → `armed`. That is up from the room to make the End kit on the surface, then back down.
   - **Off typecraft, the climb up:** it starts at the way point nearest the feet. On another seed every point is far off, so each walk fails or routes partially. After `up_walks` 3 walks a point is skipped, all nine are skipped in turn, and only then do the dry stairs up (`stairs_to_surface`) take over.
   - **Off typecraft, on the surface:** a bot more than 48 off the stairs' top with no frame within the search retargets the overland's legs to (1608, -32) (`:1869`). On another seed that is a walk of hundreds to thousands of blocks away from its own stronghold.
   - **Off typecraft, the y tests:** the reclimb rule (`feet.1 < 63 - 4` with no open sky, `:1037`) and the hut rule (`:1506`) use the `typecraft` surface's y 63. On another seed's surface at another height they fire wrongly, or never.
   - **Shows as:** `end kit: up its own way from point i of 9` / `feet …, the nearest point (1608, …)` with a nearest point far from the feet, `end kit: way point i at (…) skipped` / `3 walks to it from …` nine times, and `end kit: legs back to the stairs' top` / `D off it and no frame in the search`. The stall would be at `portal_room`.
   - **Needs:** the stair cells the bot dug on its way down to the room, recorded in the run and walked back up, or fresh dry stairs up from the room (already the fallback). The surface's y should come from the open sky the bot reaches, not from a constant.
6. **`config/end.toml` `[[spike]]` rows** (centre, `top_y`, radius, caged), measured on `typecraft`.
   - **Where:** `config/end.toml:145` on; `bot::end_spikes("config/end.toml")` at `crates/bot/src/lib.rs:355`, put into `plan.spikes.geography` at `:587`, then matched by centre in `plan::spikes::spike_for` at `crates/plan/src/spikes.rs:483`.
   - **Read by:**
     - the pillar line (`Plan::spikes_task`, and `next_pillar_need` at `crates/plan/src/end.rs:1982`);
     - the bow line (`crates/plan/src/bow.rs:573`, the caged test);

     at stages `island` → `crystals_5` → `crystals_1` → `crystals`.
   - **What varies by seed:** vanilla's spike centres are fixed: the ring of ten at radius 42 (`world::stage::SPIKES`). A spike's height (`76 + 3j`), radius (`2 + j/3`) and cage (`j` 1 or 2) are shuffled per world seed (`SpikeFeature`'s spike cache, keyed on the seed). Every crystal therefore matches a geography row by centre, and the fallback that reads the spike from its blocks (bedrock under the crystal, obsidian out along +x, bars two out) never runs.
   - **Off typecraft:**
     - the pillar is built to `typecraft`'s height for that centre, too low (the crystal is out of `STRIKE_REACH` 3, the strikes miss, and `REFUSE_TICKS` refuses the spike) or too high (blocks wasted from the stock);
     - the stock asked for follows the wrong heights;
     - the bow shoots at a caged crystal, wasting arrows on iron bars, and leaves an open one to the pillar.
   - **Shows as:** `spikes: pillar up from (…)` / `to feet T for the spike at [cx, cz] along …`. Compare T with that seed's measured `top_y` (the table below). Then `spikes: pillar up to (…)` followed by no `spikes: crystal gone N`. The bow line shows `bow: crystal N caged, left to the pillar` for the wrong spikes. The stall would be at `island` or `crystals_5`.
   - **Needs:** `spike_for` reading the blocks first, and the geography never. The spike's bedrock top, its obsidian and its bars are open to the sky, so they are seen with exposed faces once loaded. That code exists. Only the order is wrong for any seed but `typecraft`.
   - **Measured** (probe 46118698150, RCON `execute if block` for bedrock at each centre at y 76 + 3j, and iron bars two out on +x; typecraft matches `config/end.toml` exactly). The table gives the bedrock top y by centre, caged ones starred:

     | seed | [12,39] | [-34,24] | [42,0] | [-34,-25] | [-42,-1] | [33,-25] | [33,24] | [-13,-40] | [-13,39] | [12,-40] |
     |---|---|---|---|---|---|---|---|---|---|---|
     | typecraft | 76 | 79* | 82* | 85 | 88 | 91 | 94 | 97 | 100 | 103 |
     | far-stronghold | 91 | 82* | 79* | 85 | 88 | 100 | 76 | 103 | 94 | 97 |
     | far-fortress | 82* | 88 | 91 | 94 | 79* | 97 | 103 | 100 | 76 | 85 |
     | badlands | 76 | 91 | 100 | 82* | 88 | 97 | 103 | 79* | 85 | 94 |
     | cold | 79* | 76 | 94 | 85 | 91 | 103 | 97 | 82* | 100 | 88 |
     | jungle | 79* | 91 | 103 | 76 | 88 | 97 | 100 | 82* | 94 | 85 |
     | pale-garden | 82* | 88 | 85 | 97 | 91 | 103 | 100 | 94 | 79* | 76 |

     On every other seed most spikes differ from `typecraft`, by up to 27 blocks, and the caged two are different spikes.
7. **`plan::end::BRIDGE_FLOOR` = 52**, the height of the main island's lip at x 68 to 72, z 0, measured on `typecraft` (E1, docs/problems/end-observe.md). It is seed-dependent, as measured.
   - **Where:** `crates/plan/src/end.rs:47`, used at `:999` to `:1008`.
   - **Read by:** the End's bridge, at stage `end` → `island`. The bot pillars up from the arrival platform (floor 48) to floor 52 and bridges west along z 0 at that height.
   - **Measured** (probe 46118698150, RCON `execute if block … end_stone` over x 60 to 76, y 40 to 70, z 0, in each kept seed's End, forceloaded; the top y per x, and "-" where there is none from y 40 to 70):

     | seed | x 60 | x 65 | x 70 | x 72 | x 76 | edge |
     |---|---|---|---|---|---|---|
     | typecraft | 56 | 54 | 52 | 52 | - | x 72 (the lip, y 52) |
     | far-stronghold | 55 | 56 | 57 | 57 | 57 | past x 76 |
     | far-fortress | 44 | 44 | - | - | - | x 65 |
     | badlands | 57 | 54 | 50 | 44 | 42 | past x 76, sloping down |
     | cold | 56 | 46 | 49 | 50 | 48 | past x 76 |
     | jungle | 58 | 58 | 59 | 59 | 59 | past x 76 |
     | pale-garden | 43 | - | - | - | - | x 63 |

     The island's edge comes from `EndIslandDensityFunction`, but the End's final density adds the seeded `base_3d_noise`, and that moves the edge by tens of blocks and its height by 15.
   - **Off typecraft:**
     - Where the end stone reaches toward the platform higher than floor 52 (far-stronghold, jungle: y 57 to 59 at x 76), the bridge at floor 52 runs into the island's side, and the bot's feet are 5 to 7 under its top.
     - Where the edge is farther (far-fortress x 65, pale-garden x 63) or lower (y 40 to 44, under the platform), the bridge runs past the 25 `gap` cells over the void. It meets ground at another x and height, or none within `BRIDGE_MAX` 96.
   - **Shows as:** `end: pillar up to the bridge's height at (…)` / `… floor 52`, `end: bridge west from …`, then `end: bridge failed at (…)` or `end: bridge block at (…) not laid`, and no `island` stage. The stall would be at `end`.
   - **Needs:** the bridge's height and length read from the end stone seen exposed along z 0 from the platform: the nearest island top in view, within the view distance. The arrival platform (100, 48, 0) and the spike centres are the jar's constants, the same on every seed. The fountain is at (0, 0) on every seed, but its ground is not: the end stone top at (0, 0) is 58 to 64 across the kept seeds (typecraft 62).

### Fixtures: development or test only, harmless or unused on another seed
- **`config/starts.toml` places.** These are `lava`, `kitlava` and `sealava` `places` (`typecraft`'s lava sea), `nether` (0.5, 64, 0.5) with its fortress note, and every `dev-*` start: dev-forest, dev-overworld-eyes, dev-stronghold, dev-nether-home, dev-overworld-arrival, dev-nether-fortress, dev-portal-room-armed and the dev-end-* starts' crystal tags at the `typecraft` spikes. On another seed they put a bot in the wrong place. They are development starts, never evidence (`brain/ruststeve/decisions/development-starts.md`), and a relay from spawn never uses them. `spawn`, `fresh`, `table`, `stone`, `kit`, `portal` and `snapshot` are generic. Guard: race.yml refuses a non-`spawn` start on another seed without a snapshot (69-relay).
- **`config/landings.toml`, `config/landings-grid.toml`.** These are column positions on the race seed for captured worlds (`capture`), the parity table's moments (`config/moments.toml`) and issue 09's 200-landing measure. They are test fixtures: the sim is judged on `typecraft`'s captured ground, which is a fair test of physics on any seed's ground.
- **`crates/bot/src/bin/capture.rs` `seed: "typecraft"`.** This is the default `--seed` label written into a captured world's metadata. It is a test fixture, and `--seed` overrides it.
- **`config/frontier.toml`, `config/frontier-bow.toml`.** These are `typecraft`'s lineage snapshots. Per seed they become `config/frontier/<seed>.toml` (69-relay), and `typecraft` keeps `config/frontier.toml`.
- **`config/end.toml` `[room]`, `[bridge]`, `[island]`, `[fountain]` and the spikes' `ground_y`.** These are documentation of the `typecraft` End and portal room. The bot does not read them: only `[[spike]]` reaches it (wall 6). `world::stage` mirrors `[kit]`, `[island] near` and `[crystals]` as constants, but those are rules (counts, distances), not places. `world::stage::SPIKES` holds the spike centres, which are the same on every seed.
- **`plan::dragon::PERCH_POINT` = (0.5, 67, 0.5)** (`crates/plan/src/dragon.rs:767`). Its y comes from `typecraft`'s fountain ground (62 + 5), while the kept seeds' ground at (0, 0) is 58 to 64 (probe 46118698150). Only `heals_from` reads the y, with a window from y - 32 to y + 40. For any perch from 63 to 69 that window holds every crystal top (77 to 104), so the answer is the same on every kept seed.
- **`world::stage::ROOM_HALF`, `ROOM_BELOW`, `ROOM_ABOVE`.** These come from the portal room's template (`StrongholdPieces$PortalRoom`), checked on the `typecraft` room. The template is the same on every seed, and the box is symmetric on its short axis, so it holds whichever way a room faces.
- **`config/race.toml` `seed = "typecraft"`.** This is the race seed, the default every server uses without `--seed`. `config/seeds.toml` starts with it, and a unit test holds the two equal.
- **Comments and fitted thresholds.** Many plan constants cite a `typecraft` race for their size: `[fortress_way] reach` 250, `[portal_way] reach` 120, `home.portal_chunks` 48 ("past the forest's 475 from the portal"). They are distances, not places. On another seed they may be too short or too long, which shows up as a stall, not a wrong heading. They are listed here so a stall at the way home is read with them in mind.

## General walls: on every seed, not a value fitted to `typecraft`
These come from the full relay on the seeds baseline: issue-69-seeds at 4a1bda89 (integration CI 46122448225). It plays the bot as it was before #56 (sprint). The branch carries only #56's results reporting (S6) and its measurement docs (S1). The relay runs `race.yml` with `seed=all relay=true`: 1 lane a seed, 14 bots, `start=spawn`, `goal=race`, cap 36,000, legs 8. Its first leg is race 46124595679. None of these walls is fixed in #69.

### G1. The opening hunt: 18,000 ticks looking for animals at food under 48
- **Where:**
  - `crates/plan/src/lib.rs:4270`: `let look = rules.provision_ticks * if first { 3 } else { 1 };` and the explore reason at `:4273`;
  - `config/goals.toml:343` `food_reserve = 48`;
  - `config/goals.toml:370` `provision_ticks = 6000`.
- **Read by:** the first stocking, at stage `log` → `table`. With food points under `food_reserve`, it explores for animals for up to `provision_ticks * 3` = 18,000 ticks before it gives up and goes on to the table.
- **On every seed:** the bot spends 15 game minutes of every opening hunting. On the kept seeds it finds little or nothing. The table then comes at the same tick on every world:

  | seed | median run ticks to `table` (k/n) | first snapshot's `table`, game ticks |
  |---|---|---|
  | typecraft | 18,680 (12/14) | 6,343 |
  | badlands | 18,340 (6/14) | 18,446 |
  | cold | 18,520 (10/14) | 18,477 |
  | far-fortress | 18,500 (14/14) | 18,510 |
  | far-stronghold | 18,580 (8/14) | 18,640 |
  | jungle | 18,860 (14/14) | 18,249 |
  | pale-garden | 18,840 (14/14) | 18,465 |

- **Shows as (the ledger of rs-l04-b00, cold, race 46124595679 lane 4):**
  ```text
      1860 start #31 explore: explore toward (54, 69, 41) (provisions 0 of 48: look for animals)
     18000 start #105 explore: explore toward (-62, 64, -109) (provisions 0 of 48: look for animals)
     18220 note stocked (12 logs and 0 food points held after 18184 ticks)
     18520 stage table
  ```
- **Needs:** a hunt that ends on its own progress (animals seen, food gained per tick spent) instead of a fixed clock. Or the table and the stone tools first, with food gathered while the bot goes on. This is backlog for #78 (food).

### G2. shore: no log reached from a stony shore, drowned
- **Seed:** `shore` (value 11, stony shore 64 from a warm ocean).
- **Where it stalled:** stage `in_world`, never `log`. All 14 counted runs ended there.
- **What the bots did:**
  - Every run's last task was `walk-to-log: walk to log (157, 104, -26) (the nearest log in view at least 0 blocks from the start, with a route)`. That is a tree high up the shore's hills.
  - 9 runs ended at the cap still on that walk (`cap | walk to log … | the nearest log in view …, with a route` x9).
  - 5 were killed by drowned with tridents on the way (`death death.attack.trident drowned | walk to log …` x5).
- **Needs:**
  - a walk to the first log that leaves the water's edge;
  - a route that does not run along the shore at night;
  - a progress invariant on the walk-to-log, so the bot gives up a target it gets no closer to and picks another log.

  This is backlog for #71 (swim) and the water walk.
- **The relay:** shore left the relay after leg 1 (`relay-shore.toml`: stage `spawn`, `stalled = true`).

### G3. The iron for gear: most bots never see enough exposed iron
- **Stage:** `stone_pickaxe` → `gear`. Gear is an iron helmet, chestplate, leggings and sword (`world::Stage::Gear`, `crates/world/src/stage.rs:538`), about 22 iron plus the pickaxe's.
- **The relay's 36,000-tick legs called it a stall too soon.** In leg 2 (race 46127598383), 6 seeds went 0 of 14 to gear and were marked stalled at `stone_pickaxe`. far-fortress alone geared (1/14).
- **The check:** one race per stalled seed from its own `stone_pickaxe` snapshot of leg 1 (race 46124595679), 1 lane, 14 bots, cap 72,000 (a game hour). Run ticks are session ticks from the snapshot.

  | seed | check run | gear k/n | median run ticks to gear (p25 to p75) | how the rest ended |
  |---|---|---|---|---|
  | typecraft | 46130982111 | 2/14 | 34,500 (34,500 to 36,660) | tunnels with no exposed iron in sight |
  | far-stronghold | 46130984192 | 1/14 | 29,740 | the geared bot was killed by a creeper it never struck |
  | cold | 46130988348 | 3/14 | 44,280 (39,820 to 57,260) | a cave spider, an escape from water |
  | pale-garden | 46130992689 | 3/14 | 42,080 (37,800 to 62,080) | escape from water, a failed stairs step, a tunnel |
  | badlands | 46130986262 | 0/14 | - | no-progress walk-to-log x7 (no log to walk to), boxed stairs (falling blocks, no floor) x4 |
  | jungle | 46130990489 | 0/14 | - | creeper deaths x4, tunnels with no exposed iron x5 |

  far-fortress, from the relay: gear 1/14 in leg 2, then `nether` in leg 3 (race 46130812202).
- **Shows as (typecraft's best bot in leg 2, rs-l00-b00, race 46127598383):** it held 3 raw iron by tick 5,720 and 7 ingots by 19,460. It then ran 81 `stairs: tunnel (no exposed Some(IronOre) in sight, at its depth)` tasks to the cap:
  ```text
      5720 start #63 stairs: stairs down (no exposed Some(IronOre) in sight with a route)
     35980 start #359 stairs: tunnel (no exposed Some(IronOre) in sight, at its depth)
     36000 end #359 stairs after 20 ticks: the run ended (Cap)
  ```
- **Reading:** with strict no X-ray, a bot mines only the iron it has seen exposed. Its tunnels at one depth find little, so fewer than a quarter of bots gear in a game hour on any seed, and none on badlands or jungle. Gear's median is 29,740 to 44,280 run ticks where it happens. The relay is right that the bot is slow here, but the leg was shorter than the stage.
- **Needs:** iron found faster without X-ray: caves and ravines sought by sight (exposed stone faces, air pockets), and branch-mining at the depth where iron is densest, with a progress invariant on ore seen per block dug. On badlands, logs carried from the start for the later tools. This is backlog for #84 (structures and caves) or a mining issue.
- **The relay's leg cap:** the next relay runs with `cap` 72,000 (minutes 90), longer than gear's p75. A leg shorter than the slowest stage calls a slow stage a stall.
