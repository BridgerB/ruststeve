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
