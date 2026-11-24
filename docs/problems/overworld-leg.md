# The overworld leg: from `ready` to the Nether with the kit (issue 16)

The stage: a real bot resumed from the clean `ready` snapshot (race 45128866973 lane 6: the surface at (55, 70, -133), iron armour and sword, both buckets, flint and steel, a stone and a wooden pickaxe, 25 sticks, 4 logs, 470 cobblestone, a crafting table, 12 raw meat) stands in the Nether with four stone pickaxes and a table. Issue 16's win is one bot from this snapshot to seven rods, so every bot lost here is an attempt at the whole run lost.

- **Needs**: lava (ten sources by one site), a way down to it, a pickaxe in hand the whole way, 24 hunger points of food at the portal.
- **Risks**: the mobs of the caves between the surface and the lava; digging with no pickaxe; a cast cut short and begun again; a food hunt where no animal lives.
- **Done, in world terms**: from the snapshot, no bot dead in its first minutes in two 3-lane races running, and a bot in the Nether with the kit before tick 30,000.

## The read (2026-11-23): races 45194253932 and 45204423376, 16 bots on file

| race / lane | ending | portal lit | in the Nether | between |
|---|---|---|---|---|
| 45194253932 / 0 | died in Nether lava, 35,340 | 34,260 | 34,480 | 9,640 ticks with no pickaxe (6 spans), 5 casts |
| / 5 | died in Nether lava, 25,840 | 24,760 | 24,960 | 3,340 ticks with no pickaxe, 4 casts (2 paused) |
| / 10 | died in Nether lava, 32,680 | 31,740 | 31,920 | 3,160 ticks with no pickaxe, 5 casts |
| / 1 | creeper, tick 2,500, (116, 18, -83) | - | - | the cave walk |
| / 6 | creeper, tick 3,860, (81, 31, -36) | - | - | the cave walk, three retreats |
| / 9 | creeper, tick 5,480, (170, 12, -83) | - | - | the cave walk |
| 45204423376 / 3 | idle in the Nether | 31,060 | 31,280 | 5,180 ticks with no pickaxe, 6 casts |
| / 5 | died in Nether lava, 22,780 | 21,740 | 21,940 | 1,940 ticks with no pickaxe, 4 casts |
| / 7 | idle in the Nether | 25,680 | 26,240 | 5,180 ticks with no pickaxe, 4 casts; kit given up |
| / 1 | `no-progress`, 66,640 | 38,880 | never | "look for animals before the Nether" 70 times at 24 of 32 |
| / 10 | `no-progress`, 87,760 | 63,760 | never | the same 98 times at 21 of 32; 12,480 ticks with no pickaxe |
| / 4 | `no-progress`, 116,720 | never | never | 13 casts, 11 paused; 58,960 ticks with no pickaxe (74 spans) |
| / 0 | zombie, tick 6,320, y -36 | - | - | shot to 7 by a skeleton it could not reach, then hit six times in a retreat |
| / 2 | zombie, tick 10,440, y -34 | - | - | a retreat of 1,260 ticks, a fall, then hit six times in a retreat |
| / 6 | fell (skeleton), tick 6,420, y -43 | - | - | the cave walk |
| / 9 | witch, tick 3,140, y 14 | - | - | the cave walk |

**Ranked, bots lost on this leg (16 read):** 7 to mobs in the caves before any cast (wall 1); 2 at the food check with a lit portal beside them (wall 3); 1 in casts with no pickaxe (wall 2). The 6 that reached the Nether took 21,940 to 34,480 ticks, and wall 2 is most of that time.

**Where the time went** (lane 5 of 45194253932, the case's tool story and the ledger): the descent to the first landing 0 to 7,440; digs toward a landing 7,440 to 14,460, of them 8,940 to 12,160 by hand (one task, "cut 6 stairs" after 3,640 ticks); four casts 14,460 to 24,760, two of them "cast paused: no pickaxe" after 3,100 and 2,200 ticks, one site lost between. A cast alone takes 2,000 to 3,500 ticks.

## 1. The cave walk (7 of 16 dead before a cast; blaze.md wall 20)

- **What happened**: every bot's first task is "walk toward lava at (89, -19, -111) (seen from (55, 70, -133), no site within 48)". The walk goes down the snapshot bot's old stairs and into a cave band between y 36 and y 10 that runs from x 50 to 170. By tick 340 to 400 five creepers, a zombie and two endermen are in view at 32 blocks. From then on the bot alternates "stairs down" (six steps from the cave floor) and another walk through open cave, to y -19 and beyond, for 3,000 to 7,000 ticks.
- **Read, three creeper deaths tick by tick**:
  - 45175708203 lane 4: "defend Creeper 124 (Retreat)" at tick 1,040 with the creeper at 8 blocks and a zombie at 13. "creeper#124 fuse at 3" at 1,100: the creeper closed 5 blocks in 60 ticks, so the retreat made no ground. It got clear once; at 1,220 "creeper#124 fuse at 2", "at 1" and "creeper#126 fuse at 3" in the same tick, the zombie's hit at 1,240, dead from 20 health.
  - 45164371770 lane 13: a blast at 2 blocks took 20 to 8.3 at tick 1,100; it ate, cut stairs, fought a skeleton, and at 1,900 began a retreat of 460 ticks from creeper 147 in which a zombie hit it four times (13.7 to 5.6) with no swing back; two fuses at 3 in one tick ended it.
  - 45194253932 lane 6: three retreats (220, 580 and 120 ticks) in 1,400 ticks in the same cave at (83, 21, -87); the last ended with "creeper#551 fuse at 2" and "creeper#145 fuse at 3" together.
- **Cause**: not the retreat. One creeper with room is survived (57 of 61 retreats in the fed races, blaze.md wall 13). The bot spends its first minutes walking an open cave with five creepers in it, and is caught by two at once or by a zombie holding it in place. The lava hunt takes any walk that comes nearer to a far source, and the nearest far source here is a pool the site choice would refuse on arrival ("sources 1 … refused/spent 124": fewer than the ten sources a frame needs).
- **Fix**: the lava hunt goes down by its own stairs in rock (`Plan::own_stairs`), never by a walk through caves, when it stands over the lava sea's level and no pool of ten sources is known within `DESCENT_WALK` of its height. Each leg of six stairs takes the heading (toward a known pool of ten, else the next of a spiral) whose cut holds the fewest cells already seen open, so it turns away from caves it has seen; no X-ray, the cells are memory's. It stops with the feet at y -54, one over the lava that fills every cave below, and tunnels from there. A far pool of fewer than ten sources is no longer walked toward from above.
- **Unit test**: `own_stairs_turn_from_a_cave_seen_open` (plan).
- **Also, a retreat that swings**: a bot backing away from a zombie under the danger line took six hits in a row without a swing (45204423376 lanes 0 and 2, both dead). A retreat from a kind the bot does fight now strikes that foe when it is within reach and a swing is ready, still backing away (`Defend::step`, the `Retreat` arm; never a creeper). The ledger's defend line counts the swings.

## 2. No pickaxe in the hand (1 of 16 lost, and 1,940 to 12,480 ticks of every bot that passed)

- **What happened**: all nine bots that outlived the caves spent 1,940 to 58,960 ticks holding no pickaxe, with 21 to 25 sticks, stacks of cobblestone and a table in the pack. 16 of their 43 casts ended "cast paused: no pickaxe".
- **Read** (lane 5 of 45194253932, tool story): stone pickaxe broke at 5,680 during "stairs down"; the wooden one dug deepslate until it broke at 8,940 in the middle of "dig toward the landing (113, -44, -105)"; that task ran on by hand to 12,160. A stone pickaxe was crafted at 12,180, in 20 ticks. The next two broke inside casts at 17,560 and 22,700.
- **Cause**: a pickaxe is made only when none is held and only between tasks. A wooden pickaxe counts as one; a pickaxe with 13 uses counts as one; a task that breaks its last pickaxe goes on by hand.
- **Fix**: spare pickaxes ahead of need. From `ready` on, in the overworld, whenever fewer than `spare_pickaxes` (2) stone pickaxes with `spare_uses` left are held and the makings are in the pack, the planner works the `nether-kit` row (four stone pickaxes) before the next task, notes "spare pickaxes", and takes its table along after. Crafts only: never a trip for wood.
- **Unit test**: `spare_pickaxes_are_made_before_the_last_one_breaks` (plan).

## 3. The food hunt beside a lit portal (2 of 16 lost)

- **What happened**: lanes 1 and 10 of 45204423376 lit portals at 38,880 and 63,760, noted "provisions 24 of 32" and "21 of 32", and explored "look for animals before the Nether" 70 and 98 times at y -52 to -54 until `no-progress` 24,000 ticks later, at 20 health.
- **Cause**: the check has no clock, wants the whole reserve, and looks for animals where none live.
- **Fix**: the bot goes in with `portal_food` (24) hunger points or more; under that it looks for animals only under the open sky and for at most `provision_ticks`, else it notes "into the Nether short of food" and goes in.
- **Unit test**: `the_portal_food_check_is_proportionate` (plan).

## Weighed, not built: a portal near the surface

The cast needs ten lava sources at one site. Carried up they are ten trips of 120 blocks each way. Memory at the snapshot holds no surface pool: the nearest sources are at y -19 and y -15, single-source pools ("sources 1 … refused/spent 124"), and one sighting at (172, 57, -81) that no walk reached. The sea at y -55 is the only lava this world is known to hold in tens. Not built.

## Status (2026-11-23, evening): written and linted, not run

- Walls 1 to 3 are fixed in commit 91b81d9 on `issue-16-a1-overworld`, with their unit tests (`crates/plan/tests/overworld.rs`, `crates/bot/tests/overworld.rs`, `a_cornered_retreat_strikes_back` in `crates/bot/tests/defend.rs`). Clippy and brain-check pass. The tests have not run: the branch was not pushed, so no CI, no probe and no race has seen this code.
- **Nothing here is proven.** No firing is counted. The ledger lines to count in the first race from the `ready` snapshot (3 lanes, cap 30,000): "own stairs down, leg N" (wall 1; and no "walk toward lava" before the feet reach y -54), "spare pickaxes" and "spare pickaxes held" (wall 2; and "cast paused: no pickaxe" and the case's `no pickaxe` onset near zero), "into the Nether short of food" (wall 3), and defend lines that read swings with `Retreat`.
- **What to watch for, each a reason to revert that part**: own stairs that end in a tunnel at y -54 with no lake found (the old descent saw lava from the caves it walked; a tunnel through rock sees only what it opens); a table left behind or a log spent on a new one at every batch of spares; a bot entering the Nether under 24 hunger points.
