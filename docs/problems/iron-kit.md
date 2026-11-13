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
