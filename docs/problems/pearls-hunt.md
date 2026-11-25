# Issue 18e and 18f: the burrow and the hunt

Agent P3's wall file (the index is [`pearls.md`](pearls.md)). The forest base (18d) moved to agent P5.

## Needs
A bot in a warped forest (health about 12.5 that it cannot heal, an iron sword with 161 uses, filler by the hundred) gets 12 ender pearls, about 24 to 31 kills, without being hit once.

## Done (world terms)
- Ledgers show "burrow built at …", "provoke enderman N", "provoked enderman N", "fight enderman N from B", "enderman N killed" (with hits and resources per pearl) and "pearl picked (k held)" up to 12, and "pearls met".
- "breach: hurt by an enderman" 0 for a bot in B; hits per kill about 7 (iron) or 8 (stone); angers per provoke and the step-out count read from the notes.

## Risks
- The geometry is the jar's, not yet a real server's (P4's parity rows e04 to e08 first).
- Parked endermen (clojurecraft's main stall): the parked look and the fight cap are the answers.
- The bot cannot heal: a breach is permanent damage.
- One world: every race from the frontier is one forest.

## What is built (branch `issue-18-p3-hunt`)
- `plan::burrow` (pure): the site by cell, nearest the feet within 24 (B, M, O level; O and its outward neighbours three clear; four walkable beyond O; no lava within 4, no drop over 2 within 3, no bricks within 8; walls only non-holdable blocks or room for one), the shell (38 blocks at most: the tunnel in a solid 4 by 5), the missing blocks.
- `plan::pearls`: the nine-row table (`pearls_step` after the always-rows, `pearls_task` for the walks and digs) with its invariants; `[pearls]` in `config/goals.toml`.
- `look::keep_off_except` / `stares_except` (the provoke target only), `count_near_misses` skips it; `ender_threat` yields to a bot in its built burrow; `keep_look`'s eyes-down not in the hunt; `defend::box_distance_of` (2.9-high box); `cover::fill_block`.
- Tests: `crates/plan/tests/pearls.rs`.

## Walls

### 1. The first dev-forest race (race 45534591345, aaa10f9, dev-forest, 4 x 1, cap 12,000; never evidence)
- **What happened**: 3 of 4 bots built a burrow (11 to 12 blocks, the first shape) within 300 ticks of the hunt; one kill (lane 2: provoked from M at 30.4, 7 iron hits from B, health 12.5 throughout, no pearl dropped). Lane 1 provoked one at 6.1 that came close and stayed out of reach 500 ticks (the fight given up, sealed). Lane 0 stood on a fungus cap at y 56, found no site within 4 up or down, walked away blind into the basalt deltas (71 bridge blocks) and died to a magma cube.
- **Read**: each bot waited 3,300 to 4,000 ticks with endermen 15 to 30 off out of the mouth's one-way sight. Lane 1's enderman stood beside B outside the side wall: P4's real parity run 45535279839 showed the cause (O, the cells beside B and behind its back wall all lie 2 from B, a tie for a 2.9-high mob's partial path).
- **Fixes**: the shell a solid 4 by 5 with the tunnel in it (O the one cell at 2); the mouth turned toward the endermen seen lately; a provoke from O at one 12 or more off when M sees none; burrow_idle_ticks 2,400; the walk on is to warped nylium seen, the site search 8 up and down; the sword held from the fight's start (the first swing after the switch was weak); the bot pressed back in B (P4's e04 drift).

### 2. The second dev-forest race (race 45537373222, 2de67ab, P5's base in; never evidence)
- **What happened**: lane 3: base over at tick 3,060 (8 logs, 7 ingots, a second iron sword, a shield), the solid burrow at 3,420 (33 blocks, the mouth toward 61 of 120 sightings), two provokes from O (24.8 and 19.9 off), back in B 8 and 9 ticks after the creepy flag, two kills (243 and 172 ticks, 7 iron hits for the second), a pearl collected from M, 0 damage; 2 pearls in the run. Lanes 0 and 2 stood on fungus caps in the base's stem search for the cap (P5's wall, fixed in 57f25de).
- **Read**: after the two kills no calm enderman came into sight from M or O for 2,400 ticks: the burrow moved (36 blocks again).
- **Then**: P4's real runs (B never hit in 9 rows over 4 runs, M hit in 2 of 4): the bot holds in B while any enderman is within 8 of O (281ab40).

### 3. The frontier races (45542558792 to 45551462309; real, `forest` snapshot)
- 45542558792: every bot resumed in Reach and walled itself in with issue 17's pocket at tick 40 (fixed: "resumed in the forest: on to the hunt"). 45544744907: sites 20+ off with no route, bots drifted west onto basalt (fixed: a route before a site; never delta ground). 45548022681: 2 kills, 0 pearls; caps at y 71, the hunt ended or the planner stopped (fixed). 45551462309: all 8 built at (500, 46, 154), 5 kills in lanes 1 to 3 with 0 damage, the first real pearl (lane 3, dropped 0.6 from O, collected from M); five lanes stood 3,600 ticks at O with no provoke while one enderman parked out of sight 26 off held the burrow (fixed in 0e9fa0f).
- **Lane 3 of 45548022681 died at tick 220** (`death.attack.lava.player ghast`): standing at the start cell (476, 41, 152) during the base's first wait, a ghast's fireball at tick 140 knocked it off the ledge into the lava at y 30. Not a hunt path (no task ran, no escape walk began before the fall); still reachable on 0e9fa0f in the first ticks before the burrow (the forest's edge borders the deltas, where ghasts fly). The burrow and the wait at O are not near that ledge; the opening's ghast cover is issue 17's always-row.

### 4. Supply, piglins and one shared loot roll (races 45578610032 to 45598935342; real)
- **Supply ran out at every site** (45578610032, dbcd429, the hunt point [490, 5]): 39 kills and 23 pearls by tick ~28,800 with no damage, then about one kill in 20,000 ticks; three bots ended at 5 pearls on the no-progress clock. Endermen spawn only in ticking chunks 24+ from the player, about 0.4 a minute (P1), so the locals get used up. Fix (64cd7d5): a site with a kill moves after 4,800 quiet ticks (9,600 with none), 48+ blocks (new chunks) to the place with the most endermen seen, no delta ground within 16, within 96 of the hunt point; three failed walks drop a place (895d973; one bot began the same far walk 15 times).
- **Piglins at a restock** (45588764709 lane 1, tick 17,120): two piglins took a bot in iron with no gold from 19.8 to 9.6 in 20 ticks while it dug floor netherrack out of its burrow; that bot's pearls_6 snapshot then started every lane of 45594389220 at 9.6 with no heal. Fix (20b6203): out of B a piglin within 12 seals a pocket held until none is within 16; new places are 24+ from any piglin in sight. A magma cube is no foe in the hunt even after it hits (3cdb8ff; 45586888479 lane 2 fought one as provoked, 18.3 to 12.8).
- **One loot roll for eight lanes**: vanilla mob loot draws from per-world random sequences saved in `data/minecraft/random_sequences.dat` (the pearls_6 snapshot holds `minecraft:entities/enderman`), so every lane restored from one world rolls the same pearl at its nth kill: in 45588764709 the only two pearls came at the 5th kill of lanes 0 and 1 and no lane with 4 kills or fewer got one (23 kills, 2 pearls); 45594389220 had 17 kills and 45598935342 9 kills with none. The 8 lanes were one sample of the drop. Fix (race.yml 883d5d3, the orchestrator): after the server starts each lane resets the enderman sequence to a seed from the run and the lane (`randomsequence reset`); the game's drop chance is unchanged, only the lanes' rolls are independent.

### 5. Moves, tools and the run home, to pearls_12 (races 45602491038 to 45621953332; real)
- **Moves that wasted the race**: a resumed bot's old shell was not in `placed`, so every move "reclaimed 0 blocks" and restocked ~4,000 ticks; once the 6,000-tick avoid ran out it built again 10 to 18 off the site it left (45602491038). Fix 5beceab: the old walls and roof are reclaimed too (38 blocks a move), guarded (f4b65fd: no dig with an angry enderman or a hostile within 16, never the old B/M/O floor or the bot's column). A 24,000-tick avoid with moves of 48+ was worse (45609209398: 6 of 7 moved bots built nothing for 20,000+ ticks; one walked into the nether wastes): back to 6,000 and 24 (5a12ded).
- **Tools and filler on a move**: three wooden pickaxes were used up (45602491038 lane 6): the reclaim falls back to the pickaxe over its reserve (a485663) and, after a move, the base opens again for wood, a table and wooden pickaxes under 30 uses (aab012d, 97410b8; "base again" fired in 5 lanes of 45615781100). Under 20 burrow blocks no walk off is begun; new ground only in chunks with 48+ nylium faces (f5b8466, 4f1a3f1: memory keeps 64 faces a kind a chunk).
- **Provokes past 36 anger none** (45602491038 lane 6 at 60 to 63): capped at 36 everywhere (aab012d).
- **The run home from a look-out** (45615781100 lane 3, the only enderman hit of the late races): provoked 23.8 off from the look-out 4 out, it sneaked to within 0.2 of O and then of M, over 40 ticks, and was hit at M (19.8 to 15.5; the breach row sealed M). Fix ba14203: the run home goes to O at a run and turns for B within 0.8; provokes from O at 20+, from the near look-out at 30+.
- **Passed**: the pearls_6 frontier (45588764709 lane 0, 19.8 hp, after the loot-sequence fix) gave pearls_9 (45615781100 lane 6, e2e4707), and the pearls_9 frontier gave pearls_12 at server tick 11,360 (45621953332 lane 1, a414553): 12 pearls and 7 rods held, 4 kills and 3 pearls in 11,360 ticks, no enderman hit. Its only damage: two blaze fireballs and their fire at ticks 980 to 1,100 while the base crafted wooden pickaxes by the table (19.8 to 10.0; a blaze in the forest, shield down); it hunted on at 10.0 with no heal. Open: a blaze near the base craft is not yet answered.
