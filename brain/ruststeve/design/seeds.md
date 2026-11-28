---
title: Seeds
description: The fixed list of worlds the bot is measured on (config/seeds.toml), the xtask commands that list them and start a server on one, how each was chosen and measured, and what in the repository is fitted to the typecraft world.
type: reference
tags: [seeds, worlds, race, relay, measurement, generalising]
aliases: [seeds.toml, xtask seeds, xtask seed, server --seed, level-seed, race seed, typecraft, seed coupling, known geography]
status: verified
lastUpdated: 2026-11-27
verifiedAgainst: ruststeve@316c1bb9
sourceRefs:
  - "ruststeve:crates/results/src/config.rs#pub struct Seeds {"
  - "ruststeve:crates/results/src/config.rs#pub enum SeedValue {"
  - "ruststeve:crates/xtask/src/main.rs#fn seed_value(seeds: &results::config::Seeds, name: &str) -> Result<String, String> {"
  - "ruststeve:crates/xtask/src/main.rs#struct ServerArgs {"
  - "ruststeve:config/race.toml#seed = \"typecraft\""
  - "ruststeve:crates/plan/src/arrival.rs#pub const FOOD_POINT: [f64; 2] = [1648.0, -16.0];"
  - "ruststeve:crates/bot/src/lib.rs#p.spikes.geography = end_spikes(\"config/end.toml\");"
  - "ruststeve:config/goals.toml#travel_way = [[440, 60], [488, -111]]"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[race-and-results]]"
  - "[[one-bot-first]]"
  - "[[development-starts]]"
---

# Seeds

Issue 20 won on one world, `typecraft`, the race seed in `config/race.toml`. Issue 69 measures the same bot on a fixed list of other worlds. A fix that only works in one cave is then visible as such (AGENTS.md section 4.10).

## Key files
- `config/seeds.toml`: one `[[seed]]` per world, with a `name` (`[a-z0-9-]+`, used as a lane's `SEED` and a file name under `config/frontier/`), a `value` (server.properties' `level-seed`: a string, which vanilla hashes, or a 64-bit number, taken as it is) and `tests` (what the world tests). Measured facts sit in comments, and each cites its probe run.
- `results::config::Seeds`: the reader. It refuses a bad name, a name used twice, an empty value or an empty list. A unit test holds the first seed as `typecraft`, equal to the race seed.
- `xtask`:
  - `xtask seeds` prints the names, one a line, in file order (the relay's matrix);
  - `xtask seed <name>` prints the value, or exits 1 when the name is unknown;
  - `xtask server --seed <value>` overrides the race config's seed. Without the flag the server keeps the race seed, so older workflows and local servers are unchanged.

  The workflows use these commands, never a `sed` over the toml.

## How a seed was chosen
Candidates were measured on a probe server, one runner, one vanilla 26.1.2 server per seed in turn, with RCON on. The console's command source stands at the world spawn, so each measurement starts from there:
- the spawn's chunk, from `forceload add ~ ~`'s reply. A marker summoned at `~ ~ ~` was never found by `data get entity`, so its Pos gave nothing;
- the spawn's biome (`execute if biome ~ ~ ~ <biome>` over every overworld biome);
- `locate` for the stronghold, a village, a ruined portal and the nearest ocean. The distances fit a source at the spawn: typecraft's stronghold at (1648, -16) is "1648 blocks away" from chunk (0, 0);
- `locate` for the fortress and bastion in the Nether, from the spawn's chunk position divided by 8.

The first probe was 46118069221, with 15 candidates. Seven were kept, to cover different worlds: the far stronghold, the far fortress, the one dry (badlands) spawn, a cold spawn by a frozen ocean, a dense jungle and a pale garden. None of the 15 spawned in a desert or on an ocean: vanilla's spawn search avoids water. These facts are development knowledge: they explain a stall and are **never a bot input**. Under strict no X-ray, the bot finds every structure the way it would on any world.

## Seed coupling
The win's lineage put the world's own geography into the bot, under Bridger's 2026-11-24 ruling that known geography may steer the bot to win. That geography is right only on `typecraft`. The full audit is in `docs/problems/seeds.md`. The walls, the values the bot reads, are:
- `plan::arrival::FOOD_POINT`, the `typecraft` stronghold;
- `[pearls] travel_way` and `hunt_point`, that world's warped forest;
- `[home] way`, `portal` and `bridges`, that lineage's Nether route;
- `[end_kit] way_up` and `way`, that lineage's stairs up from its portal room;
- `config/end.toml`'s `[[spike]]` rows, read as geography by `bot::end_spikes`.

The spike centres are the jar's ring, the same on every seed. Vanilla shuffles each spike's height, radius and cage per world seed, so on another seed every row matches a centre but carries the wrong height and the wrong cage. Everything else fitted to `typecraft` is a fixture: development starts' places, landings, the capture's default label and the frontier files.

## Gotchas
- **A wrong geography value is a slow stage, not a crash.** Read the ledger lines that name it (`toward [1648.0, -16.0]`, `known geography (travel_way ...)`) before blaming behaviour.
- **Never change a seed's value.** Its frontier and every number measured on it would then describe another world. Add a new seed instead.
- **Numbers or text.** `value = 42` and `value = "42"` are the same world in vanilla, since a string that parses as a number is used as the number. `-1234567890` and `"-1234567890"` are the same too. Keep each seed as it was first written.

## See also
- [[race-and-results]] - the race and its starts; a lane's seed.
- [[one-bot-first]] - generalising comes after the win; this is that measure.
- [[development-starts]] - why `dev-*` places are fixtures, never evidence.
