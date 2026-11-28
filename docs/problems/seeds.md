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
Line numbers are at `main` f2408e5d.

1. **`plan::arrival::FOOD_POINT` = (1648, -16)** (`crates/plan/src/arrival.rs:130`; its bearing is taken at `:637`). This is the `typecraft` stronghold's `locate` point. The way home's walk out for animals heads there, in up to `FOOD_LEGS` 24 legs of 48 blocks, about 1,150 blocks. On another seed that is a fixed bearing into whatever lies east-north-east of the portal, which could be ocean, desert or mountains. It is not toward the stronghold, and not toward animals. Its ledger line is `leg n of 24, bearing … toward [1648.0, -16.0]`. Fix shape: derive the point from the bot's own state (the eyes' fix when it has one), or walk a bearing chosen from what the bot has seen.
2. **`[pearls] travel_way` = [[440, 60], [488, -111]]** (`config/goals.toml:483`, read into `plan::make` `travel_way` at `crates/plan/src/make.rs:365`). These are the travel points to the `typecraft` Nether's nearest warped forest (`locate biome`, probe 45529738504). Until a forest is seen, the heading runs to them in turn. On another seed they point at whatever Nether lies there. Its ledger line is `known geography (travel_way; Bridger 2026-11-24)`.
3. **`[pearls] hunt_point` = [490, 5]** (`config/goals.toml:605`, `plan::pearls` `hunt_point` at `crates/plan/src/pearls.rs:82`). This is "where the hunt goes first", a spot inside that same forest. On another seed it sends the hunt to a point with no forest.
4. **`[home] way`, `portal`, `bridges`** (`config/goals.toml:865`, `:869`, `:874`; `plan::home::HomeRules` at `crates/plan/src/home.rs:46`, `:48`, `:52`). These are the frontier lineage's own Nether route home:
   - 30-odd way points across the deltas;
   - the entry portal at (22, 34, -20), used only when memory holds no portal;
   - its bridges at z 194, "walked on its own blocks, no new bridging".

   This is the worst of the list. On another seed the way points cross lava seas and the "bridges" are void, and the home leg trusts them.
5. **`config/end.toml` `[[spike]]` rows**, read by the bot (`config/end.toml:145` on; `bot::end_spikes("config/end.toml")` at `crates/bot/src/lib.rs:355`, put into `plan.spikes.geography` at `:587`, then matched in `plan::spikes::spike_for` at `crates/plan/src/spikes.rs:483`). Vanilla's spike centres are fixed: the ring of ten at radius 42, which is `world::stage::SPIKES`. A spike's height (`76 + 3j`), radius (`2 + j/3`) and cage (`j` 1 or 2) are not fixed. Vanilla shuffles them per seed (`SpikeFeature`'s spike cache, keyed on the world seed). `spike_for` matches the geography by centre, so on another seed every spike matches a row that has the wrong `top_y` and the wrong `caged`. The effects:
   - the pillar is sized from the wrong height (`crates/plan/src/end.rs:1982`, `top_y + 1 - ground + PILLAR_SPARE`);
   - the bow line skips the wrong two spikes as caged (`crates/plan/src/bow.rs:573`);
   - the stand sits at the wrong y.

   The fallback that reads each spike from its blocks (bedrock under the crystal, obsidian out, bars) only runs when no row matches. Fix shape: read the blocks first, or keep the geography per seed.
6. **`[end_kit] way_up` and `way`** (`config/goals.toml:1081` and `:1085`). These are the frontier bot's own stairs and tunnel from the `typecraft` portal room, (1620, 17, -36), up to the surface at (1608, 63, -32), taken from race 45800967549 lane 4's ledger. The end kit's walk up follows them. On another seed the room is somewhere else, and these points lie in that world's rock 1,600 blocks from it.
7. **`plan::end::BRIDGE_FLOOR` = 52** (`crates/plan/src/end.rs:47`). This is the height of the main island's lip at x 68 to 72, z 0, measured on `typecraft` (E1, docs/problems/end-observe.md). The island's outline comes from `EndIslandDensityFunction` and does not depend on the seed. But the End's final density also adds the seeded `base_3d_noise`, so the lip's exact height and the edge at x 72 may differ by a few blocks per seed. To be measured (see "Measured" below). The arrival platform (100, 48, 0), the fountain at (0, 0) and the spike centres are the jar's constants, the same on every seed.

### Fixtures: development or test only, harmless or unused on another seed
- **`config/starts.toml` places.** These are `lava`, `kitlava` and `sealava` `places` (`typecraft`'s lava sea), `nether` (0.5, 64, 0.5) with its fortress note, and every `dev-*` start: dev-forest, dev-overworld-eyes, dev-stronghold, dev-nether-home, dev-overworld-arrival, dev-nether-fortress, dev-portal-room-armed and the dev-end-* starts' crystal tags at the `typecraft` spikes. On another seed they put a bot in the wrong place. They are development starts, never evidence (`brain/ruststeve/decisions/development-starts.md`), and a relay from spawn never uses them. `spawn`, `fresh`, `table`, `stone`, `kit`, `portal` and `snapshot` are generic. Guard: race.yml refuses a non-`spawn` start on another seed without a snapshot (69-relay).
- **`config/landings.toml`, `config/landings-grid.toml`.** These are column positions on the race seed for captured worlds (`capture`), the parity table's moments (`config/moments.toml`) and issue 09's 200-landing measure. They are test fixtures: the sim is judged on `typecraft`'s captured ground, which is a fair test of physics on any seed's ground.
- **`crates/bot/src/bin/capture.rs` `seed: "typecraft"`.** This is the default `--seed` label written into a captured world's metadata. It is a test fixture, and `--seed` overrides it.
- **`config/frontier.toml`, `config/frontier-bow.toml`.** These are `typecraft`'s lineage snapshots. Per seed they become `config/frontier/<seed>.toml` (69-relay), and `typecraft` keeps `config/frontier.toml`.
- **`config/end.toml` `[room]`, `[bridge]`, `[island]`, `[fountain]` and the spikes' `ground_y`.** These are documentation of the `typecraft` End and portal room. The bot does not read them: only `[[spike]]` reaches it (wall 5). `world::stage` mirrors `[kit]`, `[island] near` and `[crystals]` as constants, but those are rules (counts, distances), not places. `world::stage::SPIKES` holds the spike centres, which are the same on every seed.
- **`world::stage::ROOM_HALF`, `ROOM_BELOW`, `ROOM_ABOVE`.** These come from the portal room's template (`StrongholdPieces$PortalRoom`), checked on the `typecraft` room. The template is the same on every seed, and the box is symmetric on its short axis, so it holds whichever way a room faces.
- **`config/race.toml` `seed = "typecraft"`.** This is the race seed, the default every server uses without `--seed`. `config/seeds.toml` starts with it, and a unit test holds the two equal.
- **Comments and fitted thresholds.** Many plan constants cite a `typecraft` race for their size: `[fortress_way] reach` 250, `[portal_way] reach` 120, `home.portal_chunks` 48 ("past the forest's 475 from the portal"). They are distances, not places. On another seed they may be too short or too long, which shows up as a stall, not a wrong heading. They are listed here so a stall at the way home is read with them in mind.
