Title: `feat: the End and the dragon, from the portal room to the credits (20)`

Closes #25.

A real bot resumed from the frontier killed the dragon and received the credits. It reached stage `win`, its snapshot was published, and the frontier moved to it (`brain/ruststeve/decisions/one-bot-first.md`). The index of the issue's walls is `docs/problems/end.md`; each part has its own wall file.

## Evidence
- **Race:** 46108868213 on issue-20-end 9e1cfa0 (`race.yml -f snapshot=frontier`, `revive=false`, 15 lanes x 1 bot, cap 48,000). It resumed from the `crystals` frontier, release `race-46101740935` lane 1, where the bow line had taken every crystal.
- **Bot:** `rs-l06-b00`, lane 6, reached stage `win` at server tick 16,980. Lanes 2 and 5 won too: lane 2's dragon died at 20,520 and its credits came at 20,740; lane 5's `snapshot-win` was published later.
- **Snapshot:** `snapshot-win-46108868213-race-46108868213-lane-6.tar.zst` on release `race-46108868213`. `config/frontier.toml` moved to it in 9b5da2e.
- **Lane 6's ledger (`rs-l06-b00.ledger.txt`, 1,272 task records, 3,612 facts, 0 dropped), in order:**
  - `stage crystals` at tick 20, the resume;
  - 28 `dragon fight` notes over 11 perches;
  - 37 `dragon strike head (IronSword)` at reach 2.85 to 2.99, the dragon from 190.0 to 2.6, the last three on its last perch at 14.6, 8.6 and 2.6;
  - `dragon dead (entity event 3)` at 16,780;
  - `dragon: exit [1, 62, -2]` at 16,980;
  - `credits (win_game: the exit portal entered with the dragon dead …)`;
  - `respawn after the credits (1)`;
  - `back from the End in minecraft:overworld` at 17,000.
- **What the fight cost lane 6:** 5 dragon blows (`mob_attack`) and no breath hit. Its lowest health was 12.5. It made 48 knock dodges, 10 fireball dodges and 19 water pours against endermen. It had no `near miss` note, and 1 `look kept off enderman`.
- **Coverage row and rates (reported, not the bar):** <from the race's summary.md once the race ends; at 10:15 PM Denver it was still running>. Interim count: 3 of 15 lanes at `win` (2, 5, 6); 7 lanes ended (3, 4, 8, 9, 10, 12, 14).
- **CI:** ci 46108854159 green on 9e1cfa0; dev race 46108869796 (dev-end-no-crystals, 2 lanes) also ran on it (development only, not evidence).

## The ladder (each a real snapshot, each the frontier in turn)
- `portal_room`: 45800967549 lane 4.
- `armed`: 45882199358 lane 0.
- `end`: 45887747475 lane 1.
- `island`: 45888713362 lane 0.
- `crystals_5`: 46063334187 lane 8.
- `crystals_1` and `crystals`, on the bow line: 46101740935 lanes 0 and 1. Lane 1 broke 7 crystals by arrow from the ground and struck 2 from a pillar, the caged [42, 0] among them; the tenth was shot from 67 off and counted by the bare-spike rule.
- `win`: 46108868213 lane 6.

The bow line: `armed` → `island` (46072716511 lane 1, `config/frontier-bow.toml`), then the crystals above.

## What changed, by subsystem
- **Stages** (`crates/world/src/stage.rs`, accreted so `Ord` stays the ladder): `armed`, `bow_armed`, `end` (existing), `island`, `crystals_5`, `crystals_1`, `crystals`, `win`. `crystals_5` and `crystals_1` hold only on the ground; a crystal counts gone only when destroyed (its removal with its blast), or seen absent for 400 ticks.
- **Measures and config** (`config/end.toml`): the platform, the gap, the lip, the ten spikes (centre, radius, bedrock top, caged), the fountain and the jar's numbers, each with its source. Steering by the seed-fixed layout is Bridger's ruling; no bot code reads RCON.
- **World, memory, protocol** (fenced additions): the entity yaw from move and teleport packets; `dragon_dead`, `won`, attacks sent; crystal tracking (`crystals_destroyed`, `crystals_absent`); end stone, iron bars and bedrock in memory, `memory::frame_has_eye`; a look sent with no column loaded (the arrival stare); the credits answered with the existing `Act::Respawn`, the bow released with the existing `Act::ReleaseUse`.
- **Armed and the portal** (`plan::end_portal`, the `end-kit` goal row): up the bot's own stairs, wood by hand, 3 stone pickaxes, an iron sword and boots, the meat cooked, back down; the spawner broken, 11 eyes into the frames, the step in by `Entry::deeper` and a push from the frame's top; a dimension change ends the task in hand.
- **Arrival and the island** (`plan::end`): pitch 90 for the first 100 End ticks, the pillar on the platform and the guarded bridge onto the lip, surface mining in the dragon's sight (no bunker or tunnels before `crystals`), the pickaxe budget and wooden pickaxes from carried wood, one `blocks_wanted` shared with the crystals, the drowning guard (under 100 air, never in a cloud), one calm rule (`Plan::calm`: holding 40 or more off, or strafing 64 or more off), the fight begun with no bunker once the crystals are done.
- **The crystals by pillar and strike** (`plan::spikes`): the perch's heal box first, nearest first; a pillar of any carried full block with end stone at the top, 3 off the crystal's column; the two-block wall at 2 off (or the cage's bars) re-read before every strike; the strike by reach; climbs only in calm, stopped by any other phase, a fireball or the knock rule; down by landing water or the water clutch (predicted landing, the pour by height), the pillar left as a stub to climb back onto; water taken back or refilled from what was left; off a spike's top or out of its cage onto the clutch.
- **The bow line** (`plan::bow`, the kit's `Bow` phase, `bow_armed`): a bow from the stronghold's cobwebs and arrows from feathers and redug flint; the open crystals shot from the ground (the jar's flight, the low arc by bisection, the hit on the box grown by the jar's margin), the caged ones left to the pillar (`bow_keep`).
- **The dragon and the win** (`plan::dragon`): the pure model (phase, perch, head point, breath end reach); breath, fireball and charge answered by dodges sideways in the open (`fire_imminent`, `CHARGE_SPAN`), pacing in sight through a strafe; endermen answered by water at the feet, the sword the fallback; `recover_step` under 10 health; the fight only with no crystal standing, from the head stand out of the blow boxes, the crossing strike from under the wings; the exit portal, `win_game`, the respawn.
- **Gaze** (`plan::look`, `plan::endermen`): every look guarded by 4 tolerances from six eyes after the move, no line of sight trusted in the End, the nearest safe pitch; a clutch pour never bent; an angry enderman's flag not counted as a stare.
- **Defend:** creepers stepped away from (`away_step`), never fought; the dragon never a defend target.
- **Observe, results, race:** the End's facts (`CrystalSeen`, `CrystalGone`, `Strike`, `DragonPhase`, `DragonHit`, `Breath`, `GazeNearMiss`, `DragonDead`, `Credits`, …), End senses, the race summary's End row (crystals left from strikes and blasts) and dragon health trail, `Gain::End` progress, the `gaze` replay binary.
- **Model and parity** (`crates/sim/src/end.rs`, `config/moments.toml` e01 to e07, e09): a synthetic End (spikes, blasts with exposure, a scripted perched dragon, the exit portal and credits) for the closed-loop tests in `crates/bot/tests/end_loop.rs`; real-server rows for the frame fill, the entry, the strike through cover, the cage, the gaze cone, the head and its blow, the water clutch's window and the bow.
- **Brain:** new design notes `end-portal`, `end-island`, `end-crystals`, `end-bow`, and the dragon in `planner`; every note whose cited sources changed is re-verified at ruststeve@9b5da2e.

## Unverified
- Everything is this one seed's End: one platform, one gap, one set of spikes; no fix is judged on another world.
- The win is a few bots from one snapshot (3 of 15 so far); the stage rates past `crystals_5` are small samples.
- The dragon's flight, breath, wings, charges and heal are not modelled: the fight is judged real-only. The fight's tests in the sim are a scripted perched dragon.
- The bow line is merged and cleared the crystals for the winning frontier (race 46101740935: 7 by arrow, 2 struck, 1 bare). Its arrows are low arcs only (`bow::HIGH_ARC` false after parity 46099772712 missed with the high arc), shot within 64 so every blast is heard. The carved pumpkin is not built, so the gaze is discipline only.
- `config/goals.toml [end_kit] bow = true` is right for the merged line, which is the bow line that won. Its comment ("the pillar line's merge would set it false") and the arrows comment (16 count as armed; `world::stage::BOW_ARROWS` is 8) are stale.
- After the credits, lane 6 ran the End kit again in the overworld. It placed and dug back a crafting table 29 times in about 4,500 ticks ("end kit: the row's table at … taken along"). Nothing after `win` is a goal yet.
- Pillar bodies of non-immune blocks can be broken by the dragon; the bot comes down by water, not by the pillar.
- Shallow water is not standable in the walker (dragon wall 33): dodges press straight on from water; left for after the win.
- A crystal destroyed out of the bot's hearing is not counted (safe, can hold a stage back).
- Sprint is not built.
