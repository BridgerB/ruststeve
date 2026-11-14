# The iron kit (issue 13)

## Needs
From the `stone` start (stone and wooden pickaxes, a sword, a table, 3 logs, 3 sticks, 4 cobblestone), within 36,000 server ticks: a furnace placed, 7 iron ingots smelted with coal or planks, an iron pickaxe, a bucket, gravel dug until it drops flint, and flint and steel.

## Risks
- Iron is underground: stairs and tunnels meet water, lava, caves and mobs in the dark.
- Ore is only acted on once seen with an exposed face (no X-ray): a tunnel sees only its own walls.

## Done, in world terms
At least 28 of 56 real bots hold an iron pickaxe, a bucket and flint and steel within 36,000 ticks of the `stone` start.

## Walls

### 1. Iron mined at the wrong height (race 43506580854, c1c7b7f: 0 of 56)
- **What happened**: 8 of 56 bots ever held raw iron (12 in all), while they dug 9,209 cobblestone (about 165 each); 29 found coal, 35 gravel, 18 flint. The bots ended near y 68, tunnelling level.
- **Read**: three case files (rs-l00-b00, rs-l01-b05, rs-l02-b11): 107 to 313 cobblestone each, coal 7 and raw copper but no iron, ending at y 68 to 79.
- **Cause**: `depth` was ported as blocks below where the planner began (spawn minus 16, about y 68 here). clojurecraft's `:goal/depth` is the lowest *feet height* a mine row's stairs go to (spec.clj: "the lowest feet height a :mine row cuts stairs to"): iron 16, coal and gravel 40. 26.x iron generates mostly in a band from y -24 to 56 peaking at 16 (and in mountains over 80); at y 68 there is almost none.
- **Fix**: a mine row's `depth` is the feet height its stairs go down to; tunnelling level only from there.

### 2. Stairs waited out every drop (race 43512564091, 3ad2c88: 0 of 56)
- **What happened**: with the depth fixed, bots reached the iron band (most ended between y 16 and 45) and none died; 14 held raw iron and 5 an iron pickaxe, none the kit. A 6-stair cut took 1,600 to 2,000 ticks.
- **Read**: rs-l00-b00's recording: each stair's dig took 16 to 68 ticks, then about 310 ticks passed before the next. 14,000 ticks went to the stairs between its first table and its first smelt; 15 of 56 bots ended `stuck` after 12 failed walks in a row.
- **Cause**: the stairs' dig waited to collect its drop, which falls into the stair hole ahead; the collect ran out its 300 ticks every stair.
- **Fix**: a stair's dig collects nothing; the drop is picked up on the step onto the stair.

### 3. A spare sent the bot from the mine to the trees (race 43512564091)
- **What happened**: 15 of 56 bots ended `stuck` (12 failed walks in a row). rs-l00-b00, at y 32 with its iron pickaxe made, walked at logs at y 69 to 99 until it stopped.
- **Cause**: `mine-iron` keeps a spare of 24 sticks; a gather counted as worth a spare when any log was remembered, and the bot remembered the surface trees. clojurecraft's `spare-worth?` counts a gather only when one can be started now, stairs up included; ruststeve has no stairs up.
- **Fix**: a gather can be started now only on a log remembered within 6 blocks of the feet's height.

### 4. Fourteen bots in one patch (race 43518373160, e89e2f5: 1 of 56)
- **What happened**: 25 bots held raw iron (60 in all), 13 an iron pickaxe, 3 the stage, 1 the kit. 11 ended `stuck`; 54 crafts or smelts failed `Unopened`, 11 digs `Taken`.
- **Read**: rs-l00-b04 smelted at tick 4,080 with its table a block away; at 4,090 the table's cell had been air since tick 3,860 and no table was held; with its logs turned into sticks by the spare, it walked at surface trees until it stopped. Ten other bots were within 17 blocks. The same lane's furnace was placed by a click on the table (which opens it instead).
- **Cause**: every bot of a lane starts at the same spawn and mines the same ground, so tables, furnaces and drops are dug and taken by the others; a place could click an interactive block as its floor.
- **Fix**: the `stone` start spreads its bots (`spread = 128`, vanilla's `spreadplayers` from where each joined); a place never clicks a table, furnace, chest or barrel to place on it.

### 5. Stopping for good (race 43523765895, 1d19e72: 12 of 56)
- **What happened**: spread apart, 12 bots made the kit and 19 the stage (an iron pickaxe and a bucket); 6 died to mobs; 13 ended `stuck`.
- **Read**: rs-l01-b00 (walks at surface trees), rs-l01-b01 (boxed on a ledge, every stair refused for no floor, stopped at tick 1,267) and rs-l01-b03 (stairs refused for water, then no path to coal, stopped at 6,745): each stopped after 12 failures in a row and idled out the cap.
- **Cause**: issue 09's rule (12 failed walks in a row: stop) was made for a five-minute log aim; in a half-hour goal it throws the rest of the run away on one bad spot.
- **Fix**: a goal aim relocates instead: refusals forgotten, the heading turned a quarter, an explore walk, up to 6 times before it stops.

### 6. A tool swap that looped (race 43523765895)
- **What happened**: rs-l01-b09, with three ingots, a bucket and an iron pickaxe in its main inventory, made one decision in its last 19,000 ticks; it stood still at y 19 and sent 19,080 acts, nearly all clicks on slot 13.
- **Cause**: the dig chose its best tool from the inventory slots only; picking the iron pickaxe up to swap it into the hotbar took it out of the slots, the next best (a stone pickaxe) was chosen, and the cursor was put down in the first empty slot, the one it came from.
- **Fix**: the cursor counts among the tools a dig chooses from.

### 7. Relocations spent where there was no way off (race 43529108313, 489b9e6: 8 of 56)
- **What happened**: 13 bots still ended `stuck`. rs-l00-b00 used all six relocations in 1,700 ticks: each one's explore walk found no route, so the next choice cut stairs again, refused for water on every side.
- **Cause**: a relocation's explore walk tried only the full 48 blocks on two headings; on a shore or a ledge the way off is short.
- **Fix**: four headings at 48, then 16, then 8 blocks.

### 8. Flint's re-dig never counted its gravel back (race 43529108313)
- **What happened**: of 17 bots with a bucket, 8 made flint and steel; several held gravel and no flint; 38 re-digs failed in their dig. The model test reproduced it: every re-dig failed `Uncollected`.
- **Cause**: the dig counted the gravel held when it began, right after the place's block update; the slot update for the gravel put down can come after that, so the count already held the placed one and picking it back up never rose above it.
- **Fix**: the re-dig counts the item held before the place, and the dig its pickup against one less.

### 9. Gravel passed on the way down (race 43529108313)
- **What happened**: rs-l00-b05 held an iron pickaxe, a bucket and two ingots by tick 26,100 and spent the rest tunnelling at y 14 for exposed gravel, though gravel had been in sight on the way down.
- **Cause**: the needs planner plans the goal's provides in order (the pickaxe, the bucket, then flint), so a block only a later provide needs is not wanted while an earlier one is planned.
- **Fix**: each provide is also planned on its own; a mine row it leads to whose block is exposed within 6 with a route is dug now.

### 10. Buried by falling gravel (race 43534571094, 6e918dd: 12 of 56)
- **What happened**: 12 deaths, 5 of them `in_wall` (suffocation); flint improved (16 bots held flint, 12 made flint and steel).
- **Read**: rs-l00-b01 died tunnelling at y 12, its first hit `in_wall` at tick 22,300, with 13 gravel gained.
- **Cause**: stairs and tunnels opened cells under gravel, which fell into the head's cell; nothing answered a block at the eyes.
- **Fix**: a stair or tunnel step is refused when the block over its highest opened cell falls (sand, gravel, concrete powder, the suspicious blocks); an `escape-wall` always-row digs out a full block at the eyes, then at the feet.

## Where it stands (race 43538864412, ee1f110: 9 of 56)
- No suffocation since wall 10; 7 deaths (4 zombies, 2 skeleton arrows, a cave spider); 11 bots `stuck`.
- The funnel: 31 bots held raw iron (106 in all, about 3 a finder against the 7 the kit needs), 18 an iron pickaxe, 14 a bucket, 9 flint and steel; 9,370 cobblestone dug.
- Races 4 to 7 (12, 8, 12, 9 of 56) are a plateau: the fixes since removed losses (stuck, tool loops, flint, falling gravel) but not the main limit, **finding iron**. A bot sees only ore with an exposed face, and in a one-wide tunnel that is its own walls: a finder gets about 3 raw iron in 30 minutes.
- Open: how the bot searches for ore (caves, branch tunnels, the surface's exposed ore) is the next decision; ranged mobs (skeletons) are not answered.

### 11. Ore seen and walked away from (race 43523765895 onward)
- **What happened**: 1,108 walks failed `NoPath` in race 43523765895; the case files' unreachable targets were exposed ore and gravel in caves behind rock.
- **Cause**: walks do not dig; a seen block with no walk to it was refused, and the stairs or tunnel then went off on the explore heading, away from it.
- **Fix**: with no walk to any candidate, the stairs (if the nearest exposed block is below the feet) or a level tunnel head toward it, six steps at a time, and the planner looks again from nearer.

### 12. Raw iron and no fuel; a dig toward a seam that never came nearer (race 43557818721, 862a118: 11 of 56)
- **What happened**: 4 bots stuck (down from 11), 4 deaths; still 21 bots with raw iron. Three of lane 0's bots ended holding 3 raw iron, unsmelted.
- **Read**: rs-l00-b00 held 3 raw iron, 23 sticks, no coal and no logs; its last stretch was a dig toward one unreachable coal seam every 51 ticks.
- **Cause**: clojurecraft's 24-stick spare turned the kit's three logs (the planks that would have been the fuel) into sticks before any smelt, and sticks are never burned; the dig toward a block had no progress invariant. Then, with sticks allowed, a furnace holding a leftover plank refused any other fuel.
- **Fix**: sticks over a reserve of 8 burn, last (a ruststeve rule; the differential's inventories hold at most 2 sticks and still equal clojurecraft's); three cuts toward a block without coming nearer refuse it; a leftover fuel no longer held is taken out of the slot so another can go in.

### 13. Flint's gravel searched for at the end (race 43561063288, 24b9737: 14 of 56, the stage 26)
- **What happened**: 30 iron pickaxes, 23 buckets, 14 flint and steel; most of the 12 with the stage and not the kit were a step short at the cap (no gravel, gravel and no flint, or one smelt).
- **Read**: lane 0's milestones: rs-l00-b00 had its bucket at tick 7,504 and spent 8,000 ticks finding gravel; rs-l00-b03 had its bucket at 10,909 and found no gravel in the 25,000 left.
- **Cause**: wall 9's opportunism took a block only when it was the next act on a provide's way; flint and steel smelts its ingot first, so gravel passed on the way was still not taken.
- **Fix**: a goal row's `wants`: materials taken whenever exposed within 6, up to a count, whatever is being planned. The kit wants 3 gravel, 7 raw iron and 2 coal.

### 14. A task hung for the rest of the run (race 43564489198, 64d1afc: 16 of 56)
- **What happened**: 16 kits (the best yet), 18 the stage; 7 deaths (3 creepers); 16 bots never held any iron, several of them at the same few places in every lane (a seed and a spread place each bot index alike).
- **Read**: rs-l02-b00 made its last decision at tick 2,502 (stairs down), dug three stair cells, and sent nothing more for 33,000 ticks, standing on stone at y 55; no window open, no click pending.
- **Cause**: some task state waits with no deadline (not found from the recording); the planner had no watchdog of its own.
- **Fix**: a planner watchdog: a task with no progress for 1,200 ticks (the body not a block from its mark, the items held unchanged, no planner event) is cut and counted (`Stalled`).

### 15. Boxed in by water and drops (race 43567763309, 7bbcd52: 23 of 56, the stage 25)
- **What happened**: 23 kits, 2 deaths; 15 bots ended `stuck`, most with about 80 `Boxed` stairs (rs-l00-b12 in a pit by a lake, every stair refused for water; rs-l00-b00 refused for no floor).
- **Tried**: a boxed staircase filling the water or the missing floor with a held block (2d8de8c). Race 43571234795 got 14 of 56, with 212 `Stalled` and 181 `StepFailed` where race 43567763309 had almost none, and as many `Boxed` (983); reverted (ae1f54f).
- **Open**: being boxed in is still the most common way to stop; race-to-race spread (14 to 23 kits on near code) is wide, so the next fix is judged on more than one race.

### 16. Shot from beyond the defence (race 43574192321, dd06007: 17 of 56, the stage 25)
- **What happened**: on race 43567763309's code again, 17 kits (23 before: the spread); 9 deaths, 5 of them skeleton arrows.
- **Cause**: `defend` answered only mobs within 4 blocks; a skeleton shoots from up to 15.
- **Fix**: `[survival] ranged` (skeleton, stray, bogged) are answered within `ranged_radius` (16): closed in on and fought, under the same danger line.

### 17. Boxed in by short drops (race 43577176871, f13f394: 15 of 56, the stage 20)
- **What happened**: no skeleton deaths (4 deaths in all); 17 of 56 bots ended `stuck`, at every height from y 1 to 62, most with 60 to 80 `Boxed` stairs, the refusal mostly "no floor".
- **Cause**: a stair whose new feet had no full block under them was refused, however short the drop: cave floors and ledges a block or two down boxed bots in on every side.
- **Fix**: a stair over a short drop is taken when the cells down to a full landing at most 3 under the floor are open air (no fluid) and no lava is beside the landing; the step is done where the feet land. Judged on an 8-lane race (112 bots).

### 18. Dead ends with no way back (race 43580239360, 0061adc, 8 lanes: 38 of 112, the stage 50)
- **What happened**: 34% made the kit (about 19 a 56); 20 deaths (7 skeleton arrows); 28 of 112 `stuck`, still mostly `Boxed` (1,833): rs-l00-b11's stairs refused for no floor (deeper than 3) 37 times and water 28.
- **Cause**: a relocation's explore walk needs a route 8 to 48 blocks ahead; a bot at the bottom of its own staircase, boxed by a ravine or water, has none ahead, though the way it came is open.
- **Fix**: the planner keeps a trail of where the body stood (a mark every 200 ticks when it has moved 2, 60 kept); a relocation with no explore walk walks back to the newest mark at least 10 away that a route reaches, then plans again on the turned heading.

### 19. Stopping for good after relocating (race 43583394116, 33a43ec, 8 lanes: 43 of 112, the stage 52)
- **What happened**: 38% (from 34%); 11 deaths (from 20); still 27 bots `stuck`, for several causes: rs-l00-b02 had six relocations with no walk ahead and none back along the trail (just out of water: marooned), rs-l00-b11 54 stairs refused for a deep drop, rs-l00-b04 failed walks.
- **Cause**: after its six relocations a goal aim stopped for good, at tick 4,000 to 8,000 of 36,000; water settles, mobs move and chunks load in the time left.
- **Fix**: a goal aim never stops: after its relocations it rests 600 ticks and starts relocating again (issue 09's log aim still stops).

### 20. One smelt short, sticks kept back (race 43586564109, 11ab4c8, 8 lanes: 48 of 112, the stage 61)
- **What happened**: 43% (from 38%); 9 deaths; 13 bots had a pickaxe and a bucket and not flint and steel: some with flint and no seventh ingot, some with ingots and no gravel.
- **Read**: rs-l05-b12 held an iron pickaxe, a bucket, 2 flint, 4 raw iron and 7 sticks, and tunnelled for coal at y 7 for its last 20,000 ticks.
- **Cause**: wall 12's stick reserve of 8 kept all 7 sticks out of the furnace; coal is sparse that deep.
- **Fix**: the reserve is 2, one pickaxe's handle (the differential's inventories hold at most 2 sticks and still equal clojurecraft's).

### 21. Creepers met too close (race 43589713703, a27519b, 8 lanes: 50 of 112, the stage 60)
- **What happened**: 45% (from 43%); 13 deaths, 7 of them creepers.
- **Cause**: a creeper (`never_fight`) was backed away from only once within `defend_radius` (4); its fuse starts at 3, and a walk or a dig does not stop for it until then.
- **Fix**: `never_fight` kinds are answered first and from `flee_radius` (8), backed away from until past it and the margin.
