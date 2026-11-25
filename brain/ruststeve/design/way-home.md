---
title: The way home
description: How the bot goes from 12 pearls in the Nether's warped forest back to the overworld's surface, fed, holding its eyes (plan::home, issue 19d) - the pearl top-up, the eyes and the kit made in the forest, the far portal lookup over memory, the trip west by the heading, the approach and the walk into the portal, the climb by stairs in rock, the food.
type: reference
tags: [plan, home, portal, nether, overworld, eyes]
aliases: [EnderPhase::Home, home_task, home_race_task, home_done, portal_far, HomeRules, HomeState, HOME_STEPS, CLIMB_STEPS, way home, the way home, "[home]"]
status: draft
lastUpdated: 2026-11-25
verifiedAgainst: ruststeve@25a0729
sourceRefs:
  - "ruststeve:crates/plan/src/home.rs#pub fn portal_far(w: &World, dim: &str, at: [f64; 3], chunks: i32) -> Option<Cell> {"
  - "ruststeve:crates/plan/src/home.rs#pub(crate) fn home_race_task(&mut self, w: &World, tick: u64) -> Option<Task> {"
  - "ruststeve:crates/plan/src/home.rs#pub(crate) fn home_top_up_end(&mut self, w: &World, tick: u64, wanted: i32) -> bool {"
  - "ruststeve:crates/plan/src/base.rs#pub const HOME_STEPS: [Step; 6] = ["
  - "ruststeve:config/goals.toml#top_up_ticks = 6000"
  - "ruststeve:crates/plan/tests/home.rs#fn the_far_lookup_finds_a_portal_past_the_loaded_chunks() {"
related:
  - "[[ruststeve/design/_moc|Design]]"
  - "[[pearl-hunt]]"
  - "[[forest-base]]"
  - "[[planner]]"
---

# The way home

Issue 19d (`docs/problems/eyes-home.md`). Eyes do not fly in the Nether and no portal can be cast there (water evaporates; obsidian needs a diamond pickaxe), so the bot goes back through its own entry portal: about 475 blocks west of the warped forest through the basalt deltas, on a ledge over the lava sea, its overworld side deep by the overworld's lava sea.

## Key files
- `crates/plan/src/home.rs`: `HomeRules` (`[home]`), `HomeState` (`EnderState::home`), `portal_far`, `portal_cells`, `Plan::home_race_task` (the planner's entry, first in `nether_task`), `home_done`.
- `crates/plan/src/base.rs`: `HOME_STEPS` and `CLIMB_STEPS`, run by the forest base's machinery (`BaseState::steps`).
- `crates/plan/src/pearls.rs`: `pearl_rules` reads `[home] pearls_wanted` in Home; `hunting` stands down once the top-up is over.
- `crates/plan/tests/home.rs`; `Enter`'s done check's test in `home.rs`.

## How it works
- **Begun** (`home_begin`) under `Aim::Race`: in the Nether with 12 pearls and eyes held (the `[pearls]` count), from any earlier phase (a resumed bot has none); or in the overworld at stage `pearls_12` or past it (the top-up, the kit and the trip behind it). Note "home: 12 pearls held, on to the way home" or "home: resumed in the overworld".
- **The top-up**: the hunt's rows run on with `pearls_wanted` 14 until 14 are held or `top_up_ticks` 6,000 from Home's start; "home: pearl top-up over" says which. The pearls-done row never runs in Home (it would end the race).
- **The kit** (`HOME_STEPS`, each a base step with its notes): every pearl an eye (Q3's `eyes` row when the table has it, else `home-eyes`, one eye a run, the powder from rods by the needs planner), stems to `home_logs` 16, a table placed with sticks, wooden pickaxes to `home_picks` 2 with 40 uses, sticks to 8, a table to carry. "home: the kit over" (or paused: on without it).
- **The portal**: `portal_far` reads memory's `NetherPortal` index in `portal_chunks` 48 columns round the feet (768 blocks), lowered to its column's bottom portal cell; read again every `portal_every`. With none, `[home] portal` (22, 34, -20: geography, Bridger's ruling; memory holds it at the frontier, probe 45686594347). Note "home: the portal at … (memory|geography)".
- **The recovery** (as `endermen_task`'s): at or under the danger line with nothing to heal with, a sealed pocket ("home: hold in a pocket"); under `travel_health` with a ghast or blaze in sight, cover first.
- **The trip**: the heading (`heading_with`) runs legs to `[home] way`'s points then the portal's column, with the travel's rules (`EnderPhase::travels`: shunned piglins and cubes, the fortress turn, the tunnel's spare). The forest at the feet does not end it. Turned out, a far walk to within `approach`. "home: on the way" every `note_every` with health, food, pickaxe uses, filler, eyes.
- **The approach** (within `approach` 24 on the plane): a plain route into a lit portal cell is `Task::Enter` ("home: enter the portal at …"); else a far walk to beside it (bridging lava), else a tunnel to it, each "home: approach N by …". A portal seen out (its cell loaded and not a portal) is lit with flint and steel on the bottom obsidian when held.
- **Lava at the portal**: lava beside or under its lowest cells, and beside or under the open standing cells in front of its two lowest rows, is sealed first with filler from where the bot stands, clicked on a full block beside it ("home: lava sealed at"), at most `SEALS` 24. The entry portal's south face (z -19) has one dry standing cell, (22, 35, -19), with lava at (21, 35, -19), (23, 34, -19) and (24, 35, -19) (probe 45688112814); the north face has no floor over the lava sea. **The front** (`portal_front`): the face with an open standing cell over a full block, the nearer when both have one; the walk in starts only on its side, there is no far walk once a front is known (a bridge can cross to the floorless face), and the tunnel goes to the cell behind the standing cell, (22, 35, -18) at the entry portal, through z -18's netherrack. Lava behind the plane is never sealed.
- **`Task::Enter`** is done in any dimension other than the one it began in (`Enter::from`); a task with none reads as the overworld's (the old way in).
- **The overworld**: stairs up in rock (`stairs_to_surface`) to the open sky, never the caves (overworld-leg wall 1): the climb's stairs are dry (water beside a cut cell refused) and in rock (`Stairs::rock`, the climb's first cell: past `ROCK_FROM` 4 on the plane from it, a cell to cut that is already open is a cave and refused, "cave at", the heading turned; the stairs' end note counts "turned from a cave ahead"); stone pickaxes from the stairs' cobblestone when the stone uses fall under `climb_pick_uses` (`CLIMB_STEPS`: a table placed, then pickaxes to the base's 100 uses), at most `pick_runs`. On the surface: a hut each night (`hut_step`), and in it the night's cook (`home_cook`: with raw meat held and the fuel for all of it, `fuel_ticks_held` counting overworld planks, logs as four planks and coal, the clean flow's cook phase runs through `decide` with `home_cook` set: table and furnace placed, the meat smelted; "home: cook in the hut", "home: the cook over", or "home: no cook, the fuel short"), eat any safe food under `fed` 18, else hunt (`choose_prey`), else look for animals (`animal_walks`); fed, a rest toward `heal_to` for at most `heal_ticks`; then "home: over, fed on the surface" and `home_done()`; the planner then idles in the overworld (never back into the portal) until the eyes' phase takes it.

## Gotchas
- Warped planks do not burn: overworld cooking needs overworld wood or coal.
- Not built yet: the old stairs as the way up; the tunnel's own lava seal at the ledge (the tunnel's stairs refuse lava beside a cell instead).

## See also
- [[pearl-hunt]] - the hunt the top-up runs on.
- [[forest-base]] - the base machinery the kit and the climb's pickaxes run on.
