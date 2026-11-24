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
- **Fix**: the lava hunt goes down by its own stairs in rock (`Plan::own_stairs`), never by a walk through caves, when it stands over the lava sea's level and no pool of ten sources is known within `DESCENT_WALK` of its height. Each leg of six stairs heads for a known pool of ten along the longer axis, else takes the next heading of a spiral (`own_heading`); no terrain is read to choose it (strict no X-ray), and each stair's own refusal turns it as before. It stops with the feet at y -54, one over the lava that fills every cave below, and tunnels from there. A far pool of fewer than ten sources is no longer walked toward from above.
- **Unit test**: `own_stairs_head_for_the_pool_then_spiral` (plan).
- **Also, a retreat that swings**: a bot backing away from a zombie under the danger line took six hits in a row without a swing (45204423376 lanes 0 and 2, both dead). Those two were under the danger line, where the rule is never to fight, so they are not answered here. What is: a retreat from a fight that went nowhere (no hit in 200 ticks, or 600 ticks long), over the danger line, strikes a melee foe within reach while backing away (`Defend::retreat_strike`; never a creeper, never a shooter, never under the danger line). The ledger's defend line counts the swings. Test `a_retreat_strikes_only_over_the_danger_line` (plan).

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

## 4. Too little food for the fight, and all of it raw (2026-11-23, late: the measured limit at the spawner)

- **What happened** (agent A4's spawner races, the orchestrator's read): ten raw meals were gone by tick 3,600 for three blazes killed; the bot then sat at 3.9 health and food 17 with nothing to eat until it died. A fireball costs 6 to 9 health in armour; healing 13 health took 1,000 ticks and five raw porkchops. Raw beef and porkchop give 3 hunger and 1.8 saturation, cooked 8 and 12.8.
- **Cause, on this leg**: the reserve was 32 points (11 raw pieces) and nothing ever cooked meat. The snapshot bot holds 12 raw pieces and is `ready` the tick it resumes, so it never hunts again, though pigs and chickens are heard 10 to 15 blocks off at ticks 60 to 200 in every ledger. A chicken was never hunted at all: its raw meat is unsafe, so it was not game.
- **Fix**:
  - `meat_stock = 30` pieces (config, with the arithmetic: 14 blazes at 6 to 8 health a kill). With it set the clean flow's second stocking is: animals for one `provision_ticks` ("provisions N of 30 pieces: look for animals", the hunt row taking those in view; the snapshot resumes near noon and a longer look would end after dark); then `Flow::Cook` while raw meat is held; then the logs the cook burned; then "stocked again". A bot is counted ready only with the stock met (or the clock run out) and nothing raw. The bot resumed from the `ready` snapshot does all of it before it goes down.
  - The cook goes through the needs planner (`cook_goal`): first the planks that burn for every raw piece (crafted from held logs; with none, a log gathered), then for each raw kind all of it cooked, the planner making and placing the table and a furnace (8 cobblestone) where the bot stands and the smelt task doing the batch. Only a craft, a placing, a smelt or a gather is taken (never a mine), 40 tasks at most; a table or furnace counts only within 6 blocks. Ledger: "cook cooked_porkchop" a batch, "smelted", "cooked (N pieces and P food points held …)" at the end, or "cooking given up".
  - A kind whose meat cooks into safe food is game (`hunt::meat`): chickens are hunted, raw chicken is still never eaten.
  - `eat::food_points` and `best_food` already read each item's own hunger from the generated food table, so cooked pieces count 8 each and are eaten first. `food_reserve` 48 and `portal_food` 36 (points) stand for the first stocking and the portal's check.
- **Unit tests** (plan, on the real goal table): `raw_meat_is_cooked_through_the_needs_planner`, `a_chicken_is_hunted_and_never_eaten_raw`, `the_portal_food_check_is_proportionate`.
- **Raced** (race 45343519424, afa3de5, 3 lanes, read at tick 16,600): the hunt took the bots from 12 pieces to 21, 25 and 20 by tick 4,600; all three made planks, a table and a furnace and began "cook cooked_porkchop". Lane 1 smelted 9 porkchops in 1,800 ticks (57 to 102 food points). Every other smelt ended "smelt failed: TooManyClicks" in 80 ticks (8, 7 and 9 times) and the phase ended "cooking given up (41 tasks begun)" in all three, at 51, 102 and 45 points.
- **Cause of the failed smelts**: the pack held 5 oak and 12 birch planks. `Smelt::fuel_plan` loads the first kind; when that kind alone does not cover the batch it finds none of it left held and errs, and the leftover-fuel arm then shift-clicks the fuel out again: in and out to 80 clicks ("on cursor: oak_planks 3", "1", "cursor empty" at each failure). The iron smelt has the same fault.
- **Fix**, in the smelt itself: with no more of the slot's kind held, the batch goes on with what is loaded; when that is burned and input is left, the next kind held is loaded (`MAX_REFUELS` 4), and with none the smelt fails `Missing` once. The fuel asked for is for what is still to cook. The leftover arm is gone (its case, a plank left and sticks held, is the same refuel). And the cook's batch is capped to what the first fuel kind burns for (`fuel_batch`).
- **Unit tests**: `two_kinds_of_planks_fuel_one_batch` (bot, on the model: 9 results, 7 of 17 planks burned), `a_cook_batch_is_what_one_fuel_burns_for` (plan).
- **A pool near the surface** (lane 1, tick 16,620): "own stairs … over a pool of ten at (-30, 46, -254)": ten lava sources seen at y 46, 22 under the grass, 100 blocks from the snapshot spot. If a site is found there the portal is cast near the surface; read at this race's end.
- **The shields' ingots** (2026-11-24): the Nether kit is to hold two shields (one ingot and six planks each; agent A11's row). While the cook's furnace stands, held raw iron is smelted until `kit_ingots` (3) ingots are held (`cook_goal`, the `cook-iron` goal, after the meat; the snapshot bot holds 8 raw iron). The logs phase after the cook brings the pack back to 4 logs: 12 planks for two shields.
- **Risks to read in the first race**: up to 6,000 ticks hunting and 200 ticks a piece cooking, on the surface, ending near or after dusk; a smelt cut by a mob leaves its batch to be collected; planks crafted from the four logs leave the pack short of logs until the stock row gathers again.

## Weighed, not built: a portal near the surface

The cast needs ten lava sources at one site. Carried up they are ten trips of 120 blocks each way. Memory at the snapshot holds no surface pool: the nearest sources are at y -19 and y -15, single-source pools ("sources 1 … refused/spent 124"), and one sighting at (172, 57, -81) that no walk reached. The sea at y -55 is the only lava this world is known to hold in tens. Not built.

## Status (2026-11-23, evening): written and linted, not run

- Walls 1 to 3 are fixed in commit 91b81d9 on `issue-16-a1-overworld`, with their unit tests (`crates/plan/tests/overworld.rs`, `a_retreat_strikes_only_over_the_danger_line` in `crates/plan/tests/foe.rs`). Clippy and brain-check pass. The tests have not run: the branch was not pushed, so no CI, no probe and no race has seen this code.
- **Nothing here is proven.** No firing is counted. The ledger lines to count in the first race from the `ready` snapshot (3 lanes, cap 30,000): "own stairs down, leg N" (wall 1; and no "walk toward lava" before the feet reach y -54), "spare pickaxes" and "spare pickaxes held" (wall 2; and "cast paused: no pickaxe" and the case's `no pickaxe` onset near zero), "into the Nether short of food" (wall 3), and defend lines that read swings with `Retreat`.
- **What to watch for, each a reason to revert that part**: own stairs that end in a tunnel at y -54 with no lake found (the old descent saw lava from the caves it walked; a tunnel through rock sees only what it opens); a table left behind or a log spent on a new one at every batch of spares; a bot entering the Nether under 24 hunger points.

## 5. Casts that fail an op with nothing said (race 45337088346 lane 1): a blind spot first

- **What happened**: lane 1 reached the lava sea by its own stairs at tick 5,620, alive and with a pickaxe all the way, and lit no portal in 30,000 ticks: four casts ended "cast failed: Op after 13 ops, at :dig :room [30 -47 -136]", "Op after 70 ops, at :place :floor [29 -53 -126]", "out of lava after 114 ops … 26 regressions" and "NoRefill after 23 ops". Lane 2 beside it lit in one cast of 3,960 ticks.
- **Read**: the ledger holds the op each cast failed on and nothing of why its three tries failed: the dig's and the walk's failure kinds are dropped in `Cast::try_again`.
- **Fix (observability, before any behaviour change)**: each failed try at the op in hand is kept with its reason (`Cast::op_trail`, the last 6: "dig failed NoSight", "no walk to the stand and no block of a way to dig", "a placed block not shown" …) and printed in the cast's ending note after the op ("tries: … / …"). The next failed cast says what it met.

## 6. Relocations spent on this leg (race 45343519424, lanes 0 and 1: "relocate (4 of 6)" by tick 18,240 and 26,680)

- **What happened**: twelve failed tasks running cost a relocation, and six relocations end the run. Lanes 0 and 1 had spent four each by tick 27,000: one on the look for animals (every "explore toward … (provisions 21 of 30 pieces: look for animals)" ended "NoProgress" after 200 ticks, 12 in a row), two on the failed smelts (wall 4), one on own stairs begun 10 and 11 times from one cell, each "StepFailed after 0 stairs (last refusal: the walk from (75, 30, -124) onto (75, 29, -125) failed NoPath (its floor Some(Air)))": a ledge over a cave, every heading's floor air.
- **Fix**: the look for animals ends after six failed walks running ("the look for animals is over"); own stairs begun twice from one cell with the feet still there give the choice back to the old hunt from that cell ("own stairs make no way here"), a walk where one goes. The smelts are wall 4's fix.
- **Not fixed here**: lane 2's own stairs cut into water at (42, 55, -136) at tick 14,780 and the water escape held it there to the cap ("escape stalled (no progress for 1200 ticks …)" ten times). The escape is another agent's file; the read is in the orchestrator's hands.

## 7. Own stairs cut beside water (race 45343519424 lane 2)

- **What happened**: leg 3 of own stairs began at (43, 57, -136), feet y 55, at tick 11,700; at 11,740 "heard entity.generic.splash at 1" and from 11,800 the senses read "in water over stone" at (45, 55, -136). Legs 4 to 8 were begun from the same place, each ending in 40 to 180 ticks and an "escape-water" of 20 to 40 ticks, two "stairs stalled" of 1,260 ticks; at 14,780 the escape began from (42, 55, -136) and stalled every 1,200 ticks to the cap (16 times), every target refused.
- **Read**: `stairs::refusal` refuses water in a cell to be opened and lava beside one, never water beside. The leg's cells at y 55 to 57 had water beside them (an aquifer 12 under the grass); the cut let it in.
- **Cause**: the own stairs' refusal is the Nether tunnel's and the mine's, made for lava; water beside a cell is taken.
- **Fix**: a dry staircase (`Stairs::dry`, set by `own_stairs` only) refuses water beside a cell it opens and turns, as for lava. Reading the fluid beside a cell about to be opened is the existing no-X-ray exception. Ledger: the leg's ending note "cut N stairs (K turned from water beside)" or "stairs failed: … (K turned from water beside)".
- **Unit test**: `a_dry_staircase_turns_from_water_beside` (bot, on the model).

## 8. The shields made at the cook site (2026-11-24)

- **Need**: the fight's winning kit is a shield and cooked food (a development bot with both took 7 rods at 20 health, agent A11's race 45347052453). A11's kit rows (`shield`, `shields`, on its branch) make them at the portal from what is held; a portal at y -50 is where casts fail, mobs walk and tables are left behind.
- **Built**: `kit_shields = 2`. After the meat and the ingots, `cook_goal` asks for shields at the cook's table, up to two, from the ingots held (`cook-shield`; planks from held logs; the furnace's table). A made shield counts as an ingot of `kit_ingots`, so none is smelted again for it. Ledger: "shields at the cook site: craft:shield" (and the planks), the craft's own "crafted shield", and "cooked (… N iron ingots, M shields …)". With A11's rows merged, two shields held meet them at the portal with nothing to do.
- **Unit test**: `the_shields_are_made_at_the_cook_site` (plan, the real goal table).

## 9. Own stairs boxed on a ledge over a cave (race 45349387912 lane 1)

- **What happened**: cooked and ready at tick 11,500 (30 pieces, 214 food points, 3 ingots), own stairs from y 74. Leg 8 began at y 23 heading (1, 0) toward a pool at (282, -43, 9) and ended "Boxed after 3 stairs" at (250, 20, 12) on dripstone at 16,200: "no floor" at y 18 and y 19 in all four headings. From there: legs 9 and 10 boxed at once, "own stairs make no way here", the old hunt's "stairs down" boxed 8 times, "relocate (1 of 6)" at 16,420 with every explore NoPath, and the same loop again ("relocate (2 of 6)" at 16,680). Six relocations end the run.
- **Read**: the feet stand on a ledge in a dripstone cave, every cell around it over a fall of more than three. The one way off is the leg's own three stairs, back up.
- **Cause**: the own-stairs bound handed a ledge to the old hunt, which only digs down or walks; neither goes back up.
- **Fix**: the leg that led to a cell is kept (`own_into`). When two legs from one cell left the feet there, the bot walks back to that leg's start once ("own stairs back to …"), and the next leg from there does not take the heading that led to the ledge ("own stairs turned from … to …", `own_back`). With no route back, the old hunt as before.
- **Not unit-tested**: the planner's state machine needs a model world with a cave; counted in the next race by its two notes.
