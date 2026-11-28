# PR draft: issue 20, the End and the dragon (issue-20-end into main)

Title: `feat: the End and the dragon, from the portal room to the credits (20)`

Fill the `<…>` fields at the win, then delete this header block and paste from "Closes #25" down.

---

Closes #25.

One real bot, resumed from the frontier, killed the dragon and received the credits: stage `win`, its snapshot published, the frontier moved (`brain/ruststeve/decisions/one-bot-first.md`). The index of the issue's walls is `docs/problems/end.md`; each part has its own wall file.

## Evidence
- **Race:** `<run id>` on `<commit>` (`race.yml -f snapshot=frontier`, `revive=false`, `<lanes>` lanes, cap `<ticks>`), resumed from `<frontier stage>` (`<frontier tag / asset>`).
- **Bot:** `<rs-lNN-b00>`, lane `<N>`: `stage reached: win` at server tick `<tick>`.
- **Snapshot:** `<snapshot-win-…-lane-N.tar.zst>` on release `race-<run id>`; `config/frontier.toml` moved to it in `<commit>`.
- **Its ledger, in order:** `spikes: crystal gone` ×`<n>` (or `bow: crystal N gone by arrow` ×`<n>`), `stage crystals` at `<tick>`, `dragon fight`, `dragon strike head` ×`<n>` with the dragon's health falling, `dragon dead` at `<tick>`, `dragon: exit …`, `credits`, `respawn after the credits (1)`, `back from the End in minecraft:overworld`.
- **Coverage row:** `<no !!>`; gaze: `gaze near <n> (stares <n>)`.
- **Rates (reported, not the bar):** `<k>/<n>` lanes at `win`; `<k>/<n>` at `crystals`; deaths by cause `<…>`.
- **CI:** `<ci run id>` green on `<commit>`.

## The ladder (each a real snapshot, each the frontier in turn)
`portal_room` (45800967549 lane 4) → `armed` (45882199358 lane 0) → `end` (45887747475 lane 1) → `island` (45888713362 lane 0) → `crystals_5` (46063334187 lane 8) → `<crystals_1: run, lane>` → `<crystals: run, lane>` → `<win: run, lane>`. The bow line: `armed` → `island` (46072716511 lane 1), `config/frontier-bow.toml`.

## What changed, by subsystem
- **Stages** (`crates/world/src/stage.rs`, accreted so `Ord` stays the ladder): `armed`, `bow_armed`, `end` (existing), `island`, `crystals_5`, `crystals_1`, `crystals`, `win`. `crystals_5` and `crystals_1` hold only on the ground; a crystal counts gone only when destroyed (its removal with its blast), or seen absent for 400 ticks.
- **Measures and config** (`config/end.toml`): the platform, the gap, the lip, the ten spikes (centre, radius, bedrock top, caged), the fountain and the jar's numbers, each with its source. Steering by the seed-fixed layout is Bridger's ruling; no bot code reads RCON.
- **World, memory, protocol** (fenced additions): the entity yaw from move and teleport packets; `dragon_dead`, `won`, attacks sent; crystal tracking (`crystals_destroyed`, `crystals_absent`); end stone, iron bars and bedrock in memory, `memory::frame_has_eye`; a look sent with no column loaded (the arrival stare); the credits answered with the existing `Act::Respawn`, the bow released with the existing `Act::ReleaseUse`.
- **Armed and the portal** (`plan::end_portal`, the `end-kit` goal row): up the bot's own stairs, wood by hand, 3 stone pickaxes, an iron sword and boots, the meat cooked, back down; the spawner broken, 11 eyes into the frames, the step in by `Entry::deeper` and a push from the frame's top; a dimension change ends the task in hand.
- **Arrival and the island** (`plan::end`): pitch 90 for the first 100 End ticks, the pillar on the platform and the guarded bridge onto the lip, surface mining in the dragon's sight (no bunker or tunnels before `crystals`), the pickaxe budget and wooden pickaxes from carried wood, one `blocks_wanted` shared with the crystals, the drowning guard, one calm rule (`Plan::calm`: holding, 40 or more off).
- **The crystals by pillar and strike** (`plan::spikes`): the perch's heal box first, nearest first; a pillar of any carried full block with end stone at the top, 3 off the crystal's column; the two-block wall at 2 off (or the cage's bars) re-read before every strike; the strike by reach; climbs only in calm, stopped by any other phase, a fireball or the knock rule; down by landing water or the water clutch (predicted landing, the pour by height), the pillar left as a stub to climb back onto; water taken back or refilled from what was left; off a spike's top or out of its cage onto the clutch.
- **The bow line** (`plan::bow`, the kit's `Bow` phase, `bow_armed`): a bow from the stronghold's cobwebs and arrows from feathers and redug flint; the open crystals shot from the ground (the jar's flight, the low arc by bisection, the hit on the box grown by the jar's margin), the caged ones left to the pillar (`bow_keep`).
- **The dragon and the win** (`plan::dragon`): the pure model (phase, perch, head point, breath end reach); breath, fireball and charge answered by dodges sideways in the open (`fire_imminent`, `CHARGE_SPAN`), pacing in sight through a strafe; endermen answered by water at the feet, the sword the fallback; `recover_step` under 10 health; the fight only with no crystal standing, from the head stand out of the blow boxes, the crossing strike from under the wings; the exit portal, `win_game`, the respawn.
- **Gaze** (`plan::look`, `plan::endermen`): every look guarded by 4 tolerances from six eyes after the move, no line of sight trusted in the End, the nearest safe pitch; a clutch pour never bent; an angry enderman's flag not counted as a stare.
- **Defend:** creepers stepped away from (`away_step`), never fought; the dragon never a defend target.
- **Observe, results, race:** the End's facts (`CrystalSeen`, `CrystalGone`, `Strike`, `DragonPhase`, `DragonHit`, `Breath`, `GazeNearMiss`, `DragonDead`, `Credits`, …), End senses, the race summary's End row (crystals left from strikes and blasts) and dragon health trail, `Gain::End` progress, the `gaze` replay binary.
- **Model and parity** (`crates/sim/src/end.rs`, `config/moments.toml` e01 to e07, e09): a synthetic End (spikes, blasts with exposure, a scripted perched dragon, the exit portal and credits) for the closed-loop tests in `crates/bot/tests/end_loop.rs`; real-server rows for the frame fill, the entry, the strike through cover, the cage, the gaze cone, the head and its blow, the water clutch's window and the bow.
- **Brain:** new design notes `end-portal`, `end-island`, `end-crystals`, `end-bow`, and the dragon in `planner`; every note whose cited sources changed is re-verified at `<pin>`.

## Unverified
- Everything is this one seed's End: one platform, one gap, one set of spikes; no fix is judged on another world.
- The win is one bot (`<k>/<n>`); the stage rates past `crystals_5` are small samples.
- The dragon's flight, breath, wings, charges and heal are not modelled: the fight is judged real-only. The fight's tests in the sim are a scripted perched dragon.
- The bow line has not reached `bow_armed` in a race (`<fill if it has>`); the carved pumpkin is not built, so the gaze is discipline only.
- `config/goals.toml [end_kit] bow = true` is merged into the pillar line too; its comment says the pillar line's merge would set it false. It matters only for a race from `armed` or before (the kit phase); decide before merging. (The same comment block says 16 arrows count as armed; `world::stage::BOW_ARROWS` is 8.)
- Pillar bodies of non-immune blocks can be broken by the dragon; the bot comes down by water, not by the pillar.
- Shallow water is not standable in the walker (dragon wall 33): dodges press straight on from water; left for after the win.
- A crystal destroyed out of the bot's hearing is not counted (safe, can hold a stage back).
- Sprint is not built.
