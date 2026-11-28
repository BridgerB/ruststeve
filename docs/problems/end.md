# Issue 20: from the portal room to the dead dragon and the credits (one bot first)

**Status (2026-11-27): open.** The index for issue 20 (#25). The frontier is `crystals_5` (race 46063334187 lane 8, `config/frontier.toml`); the bow line's frontier is `island` (race 46072716511 lane 1, `config/frontier-bow.toml`). No real bot has yet reached `crystals_1`, `crystals` or `win`; the dragon has been killed only from a development start (dev race 45947481217, lanes 0 and 1, never evidence).

## Needs
From the frontier, one real bot kills the dragon and receives the credits. The race snapshots it as stage `win`; the snapshot is published and becomes the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`.

Bridger's answers (2026-11-26, 2026-11-27):
- Crystals by pillar and strike, clojurecraft's design; a bow line beside it for the open crystals (part 2).
- The win is the dragon dead **and** the credits (`win_game` after the exit portal), answered with the respawn.
- Checkpoints `armed`, `end`, `island`, `crystals`, with the frontier moved at each; part 2 adds `crystals_5` and `crystals_1` (between `island` and `crystals`) and the bow line's `bow_armed` (between `armed` and `end`).

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: win`, and the race's release carries its `snapshot-win-…` asset.

## The ladder so far
| stage | race, lane | from |
|---|---|---|
| `portal_room` | 45800967549 lane 4 (`rs-l04-b00`) | issue 19 |
| `armed` | 45882199358 lane 0 | `portal_room` |
| `end` | 45887747475 lane 1 | `armed` |
| `island` | 45888713362 lane 0 | `end` |
| `crystals_5` | 46055583508 lane 2, then 46063334187 lane 8 (a stand the bot can step off) | `island` |
| `island` (bow line) | 46072716511 lane 1 | `armed` |

Best so far past `crystals_5`: race 45994686575 lane 5 took 9 of 10 crystals (E9's recount from the ledgers) before its pickaxes wore out.

## The parts
Each part has its own wall file, written by its agent. Each wall is one line here; the file has the evidence.

### [`end-observe.md`](end-observe.md) (E1: measures, stages, facts)
The portal room, the End of this seed (platform, gap, lip, ten spikes, the fountain derived) and the jar's rules (crystal blast, attack by reach with no sight test, the dragon's parts, blows, heal, phases, credits) measured; stages `armed`, `island`, `crystals`, `win`, the End's facts, senses and race-summary rows built. No walls.
- Open: the fountain's pillar top (66) and perch height (about 67) are derived from the jar, not observed.

### [`end-portal.md`](end-portal.md) (E2: armed, the frames, the entry). Done: `armed` and `end` reached and snapshotted.
1. No route up from the room: the bot's own descent dropped 4 into a corridor; fixed by the way a leg at a time and a pillar under each point.
2. The pillar ended "the pickaxes first" and its own stairs had no head room; fixed by a shaft that never ends for the pickaxes.
3. The shaft met an aquifer; fixed by climbing the old legs back up cell by cell, the shaft to the sky last.
4. On the surface with no tree in reach and no walk down; fixed by the overland's legs to logs and the way points reversed.
5. Legs toward logs underground; fixed by `log_rise` 16.
6. Lit, but standing on the frame's edge in the portal; fixed by `Entry::deeper`, the smelt's fuel crafted first, the row's table dug before the cook.
7. A creeper's retreat down its own stairs; a pillar stopped beside its point; fixed by the climb first and a point counted within 3.
8. The cook given up on the phase clock; fixed by `cook_ticks` 9,600.
9. The step in ended at the portal cell's stand; fixed by the deeper goal and a push from the frame's top.
10. A walk to the cell it stood on, 1,859 times (and a creeper death); fixed by `STAND_MARGIN` 0.4.
11. Into the End, then walked off the platform by an overworld escape; fixed by `end_dimension_watch` and `over_portal`.
12. A creeper in the room at 20 health; fixed by `away_step` within `NEVER_CLOSE` 6.
13. The night's hut 60 off the way; fixed by a plain walk to the way point first.

### [`end-island.md`](end-island.md) (E3: the bridge, the island, the stock, calm)
1. Held in a pocket in its own tunnel; fixed by `end::covered` (no pocket under cover).
2. A fireball's cloud in a one-roofed stock tunnel; fixed by the bunker four down, two roofs.
3. Two roofs not enough where the ground dips; fixed by the stock `STOCK_DEPTH` 6 under the bunker.
4. Stocked, then no way back up; fixed by stairs up out of the tunnels and a shaft roofed after every dig.
5. Pockets in the stairs shut the way home; fixed by "under the end stone" as cover and whole routes only.
6. First frontier race to `island` (3 of 3); the bunker wanted end stone underfoot; fixed by laid blocks counting.
7. Half on the stair's first step; fixed by a walk up the stair before the hand-off.
8. Two gaze holes at arrival; fixed by `ARRIVAL_DOWN` 100 ticks of pitch 90 and end stone as a shelter block.
9. No route up the stair from half off it; fixed by a steered stair climb.
10. A pocket shut the bunker's own stair; fixed by no pocket at the bunker and shut steps dug out.
11. A fireball on the bunker's stair; fixed by `STAIR_CLEAR` 8. Open: the stair's upper steps are one block under the ground's top.
12. The arrival stare again: no look packet with no column loaded; fixed in world (fenced).
13. A breath notch under a half-opened bunker; fixed by cutting the open for fire and climbing back out of the notch.
14. An open bunker under a strafe; the seal rule, then narrowed (a strafe ends only once the dragon has seen and fired).
15. The bunker is no shelter from a fireball (the jar); seal and hold for fireballs removed.
16. The stock held the strafe out of sight; fixed by the strafe calling the bot out of the stock.
17. Every path out of sight under a strafe; rule: no path into the bunker or tunnels while a strafe is near.
18. The stock never came back after a hand-back; fixed by `handed_back` and restocks.
19. Short of blocks, idle; fixed by surface mining in sight and walks home counted again.
20. The stair shut with the shaft open; fixed by `STOCK_FOOT_DEPTH` 3 and arms away from the stair.
21. The bunker kept the bot from its dodge (7 of 8 dead); decision: no bunker or tunnels before `crystals`, surface mining only.
22. Reasonless digs and pits; fixed by every surface dig noted and pits kept one deep.
23. The pickaxes wore out; fixed by a pickaxe budget (`PICK_RESERVE` 40, the most worn first).
24. Slow surface digs, pits and the perch; fixed by carried blocks counting and mining 40 or more from the fountain.
25. Drowned in a pit; fixed by `Plan::drown_guard`.
26. E3's "wanted" was not E4's; fixed by one number, `Plan::blocks_wanted`.
27. In and out of the fountain's forty; fixed by `mine_ground`.
28. A pit 8 deep, a dig on in a cloud; fixed by `PIT_LOOK` 2 and the cloud cutting a dig.
29. Few pickaxe uses with wood in the pack; fixed by wooden pickaxes at a placed table (`WOOD_RUNS` 3).
30. "Holding over 64 off" seldom held; fixed by one calm rule, `Plan::calm` (holding, `CALM_FROM` 40 or more off; the closing test dropped after race 46079924222).
- Not built: a walk on the island cut by a charge (E5's dodge answers it); a quarry open to the sky.

### [`end-crystals.md`](end-crystals.md) (E4: pillar and strike, the water ways down)
1. Held on an unguarded pillar under a strafe from 62; fixed by `CLIMB_FAR` 64 and the water clutch.
2. Struck off a tall pillar, the fall killed; fixed by the clutch and the climb begun only far or perched.
3. A breath cloud at the feet; now E5's global `breath_step`.
4. Blows land inside the box; a blow in the box ends the hold.
5. The box stranded a wide spike's bot; fixed by the two wall blocks then the strike, no box.
6. The way down left the body beside its shaft; fixed by centring and stepping into the shaft.
7. The clutch missed a fast fall; the pour predicted; a charge holds the bot in its column.
8. The clutch never ran under another row; fixed by its own call site ahead of every row.
9. A walk from a cell with no floor; fixed by stepping onto the floor beside first.
10. The cell beside a dug shaft was diagonal; fixed by axis cells only.
11. E5's breath walk on a pillar's top; fixed by digging down out of the breath.
12. A clutch with no pour; made structural: the clutch owns the hand, the bucket kept in the hotbar.
13. Shut in at the bunker's stair; fixed by a dig-out toward the nearest foot.
14. A sideways fling lands on another column; fixed in part (predicted columns). Open: a body striking the island's side.
15. The pool took both buckets and the bot walked off; fixed by "no climb: no water".
16. The escape out of water took the bot off its waterfall; fixed by the ride holding the controls.
17. A waterfall too slow to ride; replaced by landing water at the foot.
18. A 2 by 1 pool is no infinite source; fixed by a 2 by 2.
19. A dig-out that never got out; fixed by rising to the ground and a progress invariant.
20. The pour sent too late; fixed, then replaced by the measured height rule (E6's e09: `POUR_FEET` 2.7).
21. The pool's water flooded the bunker; fixed by siting the pool off the bunker and its stair.
22. The pool worked from out of reach; fixed by working only from its stand, or giving it up.
23. Landing water left by a refused approach; fixed by taking it back; refills only from seen sources.
24. The dig could not beat the fireball; fixed by the drop with the clutch.
25. The landing water flooded the pillar's foot; fixed by pouring from the first block up.
26. A bot that never started a pillar; fixed by nearest-first with refusals and `shut_in`.
27. E3's stair floor read as a pillar; fixed (a placed block is a pillar only standing free).
28. A clutch landed in its own water judged missed; fixed ("poured late").
29. The landing pour went into the feet's own cell; fixed by centring and `LANDING_AIM` 0.35.
30. A climb begun with no blocks; fixed by the whole pillar's blocks plus `PILLAR_SPARE` first.
31. A clutch with no ground to pour on (flung over the void); `landing_ground` made robust; the cure is not being knocked off.
32. The walk to a pillar's foot without its blocks; fixed by the same gate at the walk.
33. A water bucket in hand read as the clutch's water back; fixed.
34. A dig-out stair with a step's floor missing; fixed by the floor put and the way out turned.
35. Only end stone built a pillar; fixed by any carried full block (`PILLAR_BLOCKS`), end stone at the top.
36. The way down was the dig; fixed by the clutch descent (pillar left standing).
37. Knocked off by the charge; fixed by `knock_risk` within `KNOCK_NEAR` 30.
38. The climb began on a perch; fixed by climbs only with the dragon holding, and down by water on a descent phase.
39. The clutch's pour landed short under knock-back; fixed by `predict_landing`.
40. Water counted back that wasn't; fixed by `take_back_from` and the `left_water` refill.
41. The far caged crystal first while a near one healed; fixed by the perch's heal box first.
42. Stranded on a spike's top (the first `crystals_5` resume); fixed by `spike_edge` and the step off onto the clutch.
43. "Climb on through" inside 64; removed (one exception near the stand).
44. Its own stubs could use up a spike's approaches; fixed by climbing back onto stubs.
45. Stranded inside a spike's cage; fixed by breaking a bar of the cage.
46. The climb rule stopped almost every climb; fixed (start at `CLIMB_NEAR` 40 holding, go on while holding); the gates now read E3's `Plan::calm`.
47. A climb 1 under its stand stopped; fixed by `STAND_NEAR` 5 finishing.
48. Left water never refilled from; fixed by a dry stand and kept records.

### [`end-dragon.md`](end-dragon.md) (E5: breath, dodges, the fight, the credits)
1. A provoked enderman's pocket held the bot in the dragon's reach; fixed by `dragon_first_step`.
2. A fireball's cloud read as radius 0; fixed by the cloud's growth from 3 to 7.
3. The stand behind is in the landing's reach; fixed by the ring 12 off until it sits.
4. Breath killed bots outside the fight; one global rule, `breath_step` and `breath_ahead`.
5. The takeoff strike threw the bot off the island; fixed by `sat_damage` and the crossing strike from under the wings.
6. The sweep hits the bot on the surface; fixed by low stands.
7. The walk out of the breath chose the feet's own cell; fixed by `OUT_SLACK`.
8. A charge in the open; `charge_step`.
9. The strike window lost; fixed by `scan_stand` and the flame leave at 25 ticks.
10. The takeoff count never restarted; fixed.
11. Far from the perch while scanning draws a charge; fixed by `SCAN_KEEP` 12.
12. A low stand out of reach; fixed by `clear_of_blow` under the boxes.
13. Sealed in a pocket, the fight walked for the bunker; fixed by holding in cover.
14. Walled in a breath; fixed by digging toward the cell.
15. No pocket in the End, ever; `end_enderman_step` instead.
16. Breath near the bunker answered sideways; fixed by answering down (later reworked, walls 28, 36 and 38).
17. An enderman fought in the open; a notch (later replaced, wall 42).
18. No way out of a notch under the bunker; fixed by `rise_step`.
19. The way into the bunker flapped; fixed by keeping the way to its end.
20. Held in cover while the dragon sat; fixed by a shaft up.
21. Near the head stand but out of reach; fixed by `Want::Centre`.
22. The bunker's breath hold through the strike window; fixed.
23. Every real breath death began in a strafe; a shelter before the fireball (later replaced).
24. The takeoff strike never came; fixed by a low stand dug ahead.
25. Hidden for the whole strafe, the strafe never ended (the jar); fixed by meeting it in sight.
26. The old shelter still fired first; fixed (stand in sight).
27. A silent idle of 15,800 ticks; backstop `end_idle_watch`.
28. Cover is no answer to a fireball (the jar); the dodge, `fireball_dodge_step`.
29. A hold from an angry enderman never ended; fixed by `ENDER_HOLD_CAP` 600.
30. Out of sight under the sky the strafe held; fixed by rising until seen.
31. The dodge began too late; fixed by pacing in sight.
32. The dodge stood on an older cloud's edge; fixed (the dodge first).
33. Dodge cells it could not walk to; fixed by a route check. Open after the win: shallow water standable in the walker.
34. The fireball seen too late; fixed by `fire_imminent`.
35. No dodge cell from the bunker's stair; fixed by `DODGE_YS`.
36. The notch is a trap; no notch for a charge or a cloud, the charge dodged sideways.
37. The pace took the bot from its work; only an idle bot paces.
38. No block to rise on; the wall dug; the notch restored for a cloud already on the body with no walk out.
39. The fight waited for every crystal; a perch fight with crystals standing (later turned off, wall 45).
40. No dodge from where the bot stood; fixed by `charge_ready_step`.
41. The charge dodge inside the wings' sweep; fixed by `CHARGE_SPAN` 12 to 16.
42. The enderman's notch was no shelter; the sword over the danger line, else walk away.
43. The short-drop rule walked the bot into pits; fixed.
44. A meal held 2,360 ticks; fixed by `meal_cut`.
45. No perch fought while a crystal stands (it heals in flight too); `FIGHT_WITH_CRYSTALS` false.
46. Five of ten dead by 13,460; water against endermen, `recover_step`.
47. Waits at a breath's edge that grew; fixed by the end reach.
48. A single angry enderman beat a full-iron bot; fixed by water for any angry one.
49. The water against endermen went unaccounted; fixed, lost water goes to E4's refill.
50. The breath's loops ate the time; fixed by one episode with kept refusals and caps.
- Open: the flame cloud costs about 3.0 each flaming phase; sprint (its own issue); the fight itself is proven only from a development start.

### [`end-model.md`](end-model.md) (E6: parity rows e01 to e06, e09; the synthetic End)
1. The cover at 2 off is spent by each blast; the wall is rebuilt or re-read before every strike.
2. The last crystal left the bot on its pillar in the sim; fixed by E3's 7b4ccc1.
3. The fountain one too high in the sim; fixed in ae32015.
- Open: dragon flight, breath, wings, charges, the heal and the blocks a blast breaks are not modelled; e09-pour-8-4's late water is not explained.

### [`end-gaze.md`](end-gaze.md) (E7: never a stare)
The 17 stares of races 45994686575 and 45988967677 were all looks down; fixed by `look::GUARD` (4 tolerances), six eyes judged after the move, no line of sight trusted in the End, `safe_pitch`, and an unbent clutch pour. An angry enderman's stared-at flag is told apart from a stare.
- Open: the done count (`gaze near 0` on every lane) is read only from the frontier race on the merge; race 46046557927's two lanes had one near miss, the clutch pour as ruled.

### [`end-bow.md`](end-bow.md) (E8: the bow line). Frontier `island` (race 46072716511 lane 1); parity e07 hit 2 of 2.
1. Every flight from under the crystal read wide; fixed in 8838a9d.
2. Feathers and string dropped as junk; fixed by keeping them and picking them up.
3. String scarce by day; fixed by the stronghold's cobwebs (`bow_room_task`).
4. Cobwebs in reach but no walk; fixed by digging toward them.
5. The first bows, too few arrows, stranded after; fixed by the room's bow row and the way back to the room.
6. Three crystals shot from the ground (three hits in three); the rest given up from the bridge and no `bow_armed`; fixed by waiting for the island, retrying stands and `bow_keep`.
- Open: no lane has reached `bow_armed`; the carved pumpkin is not built.

### [`end-checkpoints.md`](end-checkpoints.md) (E9: `crystals_5`, `crystals_1`)
1. The resume transient fired `crystals_5` at tick 40; fixed: a crystal is gone only when destroyed (`crystals_destroyed`), or seen absent for 400 ticks.
2. The snapshot on a stand; fixed: the two stages hold only `grounded`.
- Risk: a blast out of the bot's hearing is not counted (safe, but can hold a stage back).

## What is open
- **`crystals_1`, `crystals` and `win` by a real bot.** The calm rule (island wall 30; crystals walls 46 and 47) is the newest change; its frontier race is the next count.
- **The fight starts only at `crystals`**: the dragon heals in flight from any standing crystal (dragon wall 45). The fight from `crystals` has not run on a real start.
- **Resources:** pickaxe uses (island walls 23 and 29), blocks (crystals walls 30 and 35), water buckets (crystals walls 40 and 48).
- **The bow line** is merged (e713f7d) but has not reached `bow_armed`; whether bow shots replace climbs at scale is unmeasured.
- **One world:** every race replays this seed's End; nothing is judged on another.
