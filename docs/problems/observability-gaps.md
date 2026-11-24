# Issue 16: observability gaps (the files did not say what happened)

## Needs
A case file, a ledger and a senses file each read right the first time. On the night of 2026-11-23 the evidence of race 45204423376 was misread twice from files that were whole but said the wrong thing at a glance: a position printed rounded was taken for the feet's cell, and tool lines read as losses that were moves. Every wall here is a place where a careful reader gets the wrong answer, with the read that shows it and the change to the text or the fact.

## Done (world terms)
One recording of race 45204423376 re-read with this build (`replay ledger <rsr> --force --senses 10 --story`) prints, for each wall below, text that cannot be read the old wrong way; unit tests on hand-built worlds and ledgers hold each rule. Nothing in the `.rsl`, `.rss` or `.rsc` files is renamed or dropped: fields and variants are added, and old files still read.

## Risks
The text of `.ledger.txt`, `.senses.txt` and `.case.txt` changes shape, so anything that greps them by column must be looked at (nothing in the repository parses them; the MCP `race_read` returns them whole). `observe::diff` holds the holdings while a click waits for its answer, so an item fact can come up to 20 client ticks late; the bot's no-progress clock reads those facts and is late by the same.

## Wall 1: a rounded position read as the feet's cell
- **What happened.** Lane 3's senses rows read `30,31,-18` for 24,000 ticks and the case read `at Some([30.49940297953616, 31.0, -18.3])`. The orchestrator took the cell to be (30, 31, -18) and read the blocks round it one cell off.
- **The read.** `a45204423376/lane-3/rs-l03-b00.senses.txt`, rows 55020 to 55820: `30,31,-18 hp 20.0 food 16 held cobblestone | idle (…)`. `crates/results/src/lib.rs` printed `format!("{:.0},{:.0},{:.0}")`; `plan::path::feet_cell` floors: z -18.3 is cell -19.
- **The cause.** Two conventions for one number, and the printed one was the one the planner does not use. Nothing in the row said what the feet stood on or whether lava was against the body, so the block view had to be rebuilt by hand with `why`.
- **The fix.** A senses row keeps `cell` (the position floored, `observe::feet_cell`, held equal to `plan::path::feet_cell` by a test in `results`), `feet_block` (the block in that cell: air on a full block, soul sand or a fluid when the feet are inside its cell), `under` (the block under the cell) and `lava` / `water` (`Touch::In`, `Beside`, `No` over the body's two cells and the cells against them). The text prints the position to one decimal, then `cell 30,31,-19 on netherrack, lava beside`; rows from before the change print the floored cell alone. The case's position line prints `30.50,31.00,-18.30 (feet cell 30,31,-19)`. A change of dimension is a line of its own in the senses text (it was nowhere in it).
- **Not done here.** The live progress line is written in `crates/bot/src/lib.rs` (another agent's file): `observe::Senses::standing()` gives the same words for it; one `format!` argument there.

## Wall 2: a tool moved read as a tool lost or broken
- **What happened.** `item stone_pickaxe 1 -> 0` then `0 -> 1` at one tick, 62 such lines in lane 5; the same for buckets (89) and the sword. A reader counts pickaxes lost. The rule for `tool broke` was "gone from its slot, one fewer held, under 4 uses left", so a worn tool picked up on the cursor read as broken.
- **The read.** `a45204423376/lane-5/rs-l05-b00.ledger.txt` lines 1060 (`item iron_sword 1 -> 0`, `0 -> 1`, `item stone_pickaxe 1 -> 0`, `0 -> 1`: one hotbar swap), 14040 to 14160 (a bucket and the pickaxe, ten lines). `crates/observe/src/lib.rs` summed `inventory.slots` and left `inventory.carried` out; `world` forgets the cursor as it clicks (`Act::Click` sets `carried = None` until the server's answer), so between a click and its answer the stack is in no slot and not on the cursor.
- **The cause.** The holdings were the slots alone, read in the middle of a click.
- **The fix.** `Snapshot::items` counts the cursor's stack; `Snapshot::cursor` and `Fact::Cursor` (`on cursor: stone_pickaxe 1`, `cursor empty`) say where it is; a senses row keeps `cursor` (a stack left there is in no slot the bot can use). While a click waits for its answer (at most `CLICK_SETTLE`, 20 client ticks) `diff` reads the holdings as they last settled. `tool broke` is said only when a tool's uses run out in its slot, or it is gone from the holdings on its last use (`BREAK_AT`, 1); a tool gone with more uses left is `Fact::ToolGone` (`tool gone: stone_pickaxe with 40 uses left (dropped or lost, not broken)`), in the case's `tools:` list with the task running.
- **To check on the probe.** Lane 5's four real breaks (ticks 4260, 7540, 14520, 19340) must still read `tool broke` under the stricter rule, and the swap lines must be gone.

## Wall 3: many changes to one item in one second
- **What happened.** `item cobbled_deepslate 164 -> 170`, `170 -> 173`, `173 -> 177` at one tick; a death's last second was 14 lines. The ledger's stamp is the server's second (`set_time` comes every 20 ticks), so "one tick" is 20 client ticks of facts.
- **The read.** `lane-5` ledger at 14240; 283 `cobbled_deepslate` lines in that file.
- **The fix.** In the text, one line an item a stamp with the first and last count and the number of changes: `item cobbled_deepslate 164 -> 177 (3 changes)`; a stack that left and came back says `1 -> 1 (2 changes, none in the end)`. Health and food the same; a fact that repeats word for word is told once (`heard entity.generic.burn at 0 x2`). The file's header still counts every fact, and the `.rsl` holds them all.

## Wall 4: one note 23,982 times
- **What happened.** One ledger held 23,982 lines of `note cast paused: no pickaxe`; the case's "last two minutes" was 80 copies of it.
- **The fix.** `results::text::planner_lines`: a run of the same planner lines (a block of up to four lines, the same but for the task's number) is told once at its first tick with `[xN, to tick T (#last id)]`. The case's last two minutes are the ledger's lines over those ticks alone, told the same way, so a run that began an hour before still shows with its count inside the window.

## Wall 5: the case's head did not say where the bot stood, and a fix's firings had to be grepped
- **The fix.** The case opens, under `ending:`, with `where:`: the dimension, the position and the feet's cell, what the feet stand on, lava or water against the body, health, food, the held item and the cursor, the task and its reason at the last senses row, and the last stage with the ticks since. `notes by kind` counts every note of the ledger by its leading words (`seal lava at`, `magma under`, `cast paused: no pickaxe`, `craft * failed: Unopened`) with first and last tick; the race's totals are the diagnosis table in `summary.md` (`results diagnose`), which now counts every note of every ledger: it counted the case's kept notes, which leave repeats and the middle out, so its numbers were low without saying so.

## Wall 6: an escape's trail
- **The read.** No renderer in `results` or `observe` cuts a line; the stall note with its trail is whole in the `.rsl` and the text. The trail was missing because the planner wrote it only on a stall (another agent's fix).
- **The fix.** A test holds a 2,000-character trail whole through the ledger text; the case lists every note that carries `trail:` under `escape trails (whole)`, wherever in the run it was made.

## Wall 7: found by reading lanes 3, 5 and 7 of race 45204423376
- **7a. A task that ended before it began.** `22240 end #58 stairs after 0 ticks` was printed above `22240 start #58 stairs`, and `start #57` at 22220 had its end printed above it, out of the window: it read as a task never ended and another ended unstarted. `Ledger::render` sorted ends before starts at one tick. The planner's own order is the file's order (decoded from `rs-l05-b00.ledger.rsl`: `Started 57`, `Ended 57`, both at 22220) and the text now keeps it.
- **7b. 644 stairs refused at once read as 19,200 ticks of stairs.** Lane 3's `tasks by time` said `stairs 680 x, 19200 ticks`. It now says how many ended within the second they began.
- **7c. The dimension was not in the senses text**, and a row at y 31 in the Nether read like one in the overworld. A line when it changes.
- **7d. The note-kind table of `summary.md` was built from kept notes** (wall 5).
- **Not mine to fix, reported to the orchestrator:**
  - `doing: stairs failed: StepFailed after stairs (why: last refusal: ; turn the heading)` and `doing: stairs down toward the fortress at (why: …)`: the case's `doing` is a note with its numbers taken out, which reads as cut text (`crates/bot`).
  - `last refusal: ;` is an empty refusal: `stairs` ends `StepFailed` with no refusal text 644 times in lane 3 (`crates/plan/src/stairs.rs` or its caller).
  - A senses row `idle (last refusal: ; turn the heading)`: the reason printed for `idle` is the last note's, from the task before (`Plan::why`).
  - The progress line prints `held beef (- uses)` and no position (`crates/bot/src/lib.rs`).

## Wall 8: health lost with no hit in the ledger
- **What happened** (agent A4, race 45335627948, the `dev-spawner` start). Health fell by 5 with no hit fact in the ledger and none in the case's hit list: lane 1 at ticks 140, 160 and 180, lane 2 at tick 140. A4's guess, not confirmed: a blaze's melee blow inside the invulnerability window of a fire tick sends no damage event.
- **The read.** `observe::diff` wrote a `Hit` only for a damage event, and `health a -> b` for the fall: a fall with no event was a health line with no cause, and nothing counted such falls. A second cause was in the code whatever the first turns out to be: new blows were counted by how much `World::hits` grew, and the world prunes that list as it grows (`HITS_KEPT_TICKS`, 1,200), so once blows come for a minute the list's length stands still and every blow after goes unseen ("a list pruned in the same tick shows none new" was the comment).
- **The fix.** New blows are told from the newest blow's stamp and the count at that stamp (`Snapshot::hit_mark`), never from the list's length. A fall in health is answered by a damage event that came with it or within `HIT_WINDOW` (10 client ticks) before it and that no earlier fall took; a fall with none is `Fact::HealthDrop`: `health dropped 5.0 to 12.0 with no damage event; nearest hostile blaze at 2.1; on fire no`. The case lists them under `health lost with no damage event (N times)`.
- **What the packets are** at those ticks is read on the probe below (`replay export` of lane 1 round ticks 120 to 200: `DamageEvent`, `HurtAnimation`, `SetHealth`).

## Probe
(Filled in from the probe at the end: before and after for each wall.)
