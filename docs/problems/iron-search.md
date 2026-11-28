# The iron search (generality item B, wall G3 of seeds.md)

The stage is `stone_pickaxe` → `gear`: an iron helmet, chestplate, leggings and sword, about 22 iron plus the pickaxe's. It is G3 in [`seeds.md`](seeds.md). The design note is [`brain/ruststeve/design/iron-search.md`](../../brain/ruststeve/design/iron-search.md); the code is `crates/plan/src/iron.rs` and the `// >>> iron search (gen)` block of `Plan::mine`.

## Needs
- With no exposed iron in sight, the bot looks somewhere new each time: new rock on every tunnel, not its own tunnels walked again.
- It mines where iron is: the middle band (y -24 to 56, densest at 16), in stone rather than deepslate where it can.
- It uses the free faces the world already gives: a cave or ravine seen beyond the 24 blocks `exposed_near` reads is walked to (or dug toward), and its walls are read from there.
- Each search ends on its own progress: a strip that stops getting longer is turned, and a strip that runs its length showing nothing is turned too.
- Every choice writes a ledger line, so its firings can be counted.

## Risks
- **Strict no X-ray.** A cave counts only when the bot sees it: a clear line from the eye to the cave cell through cells seen open (`defend::sight`, where a cell never seen exposed is rock). A strip's "fresh rock" counts air the eye has no line to as rock, so unseen air steers it neither toward nor away. Ore is still only taken through `exposed_near`. Air in the loaded chunk data is not a cave seen (the near miss below).
- **Caves are dangerous**: mobs in the dark (skeletons, cave spiders, creepers), water and lava. The walk's own route rules and the defence rows still apply, and caves are sought only inside iron's band (above y -24: vanilla fills the caves under y -54 with lava).
- **Stairs up and level strips meet water, gravel and lava like any staircase**: the stairs' own refusals apply (`stairs::refusal`), and a heading is "fresh" only when its first step is not refused.
- **Turning too often** wastes the strip: a strip is turned only after `strip_len` blocks or `strip_stalls` tunnels in a row with no progress.

## Done (world terms)
Paired against `stack` on the 8-seed relay's `stone_pickaxe` snapshots (race 46124595679, lanes 0 to 6), cap 72,000: gear is reached on more seeds and in more runs than the baseline 4a1bda89 (typecraft 2/14, far-stronghold 1/14, cold 3/14, pale-garden 3/14, badlands 0/14, jungle 0/14). The new paths' ledger lines are counted in the judging races, and no bot is seen walking its own tunnel for a whole task.

## The last recorded failure (baseline 7f1af997, the bot of 4a1bda89)

### 1. Tunnels that walk the bot's own tunnel again (race 46130982111, typecraft, 14 bots)
- **What happened**: 2 of 14 geared. Of the 3,711 level tunnels that ended "cut 6 stairs (look again for what to mine)", **2,727 ended in under 100 ticks**. Six level steps through stone with a stone pickaxe take at least twelve digs, about 300 ticks; under 100 ticks, nothing was cut. Those tasks took 138,980 of the race's 1,008,000 bot ticks; the tunnels that did cut took 377,900 more and the race gained 154 raw iron in all.
- **Read (rs-l00-b00, the case and the senses)**: from tick 58,400 to the cap (13,600 ticks) the feet moved between x 133.9 and 139.4 at y -40, z -124.5, on deepslate, and the stone pickaxe stayed at 93 uses: nothing dug. The ledger is one line pair after another:
  ```text
   70440 end #836 stairs after 60 ticks: cut 6 stairs (look again for what to mine)
   70440 start #837 stairs: tunnel (no exposed Some(IronOre) in sight, at its depth)
   70500 end #837 stairs after 60 ticks: cut 6 stairs (look again for what to mine)
  ```
  The same bot did it at y -10 from tick 43,600 to 54,200 (x 126.8 to 131.5, the pickaxe at 85 uses throughout), until a creeper moved it.
- **The other checks** (fast = a "cut 6 stairs" tunnel under 100 ticks):

  | race | seed | fast tunnels (ticks) | tunnels that cut (ticks) | raw iron gained |
  |---|---|---|---|---|
  | 46127598383 | relay leg 2, 7 lanes | 1,555 (80,860) | 1,406 (489,100) | 527 |
  | 46130982111 | typecraft | 2,727 (138,980) | 984 (377,900) | 154 |
  | 46130984192 | far-stronghold | 671 (34,540) | 355 (120,320) | 127 |
  | 46130986262 | badlands | 21 (1,080) | 56 (21,740) | 2 |
  | 46130988348 | cold | 150 (7,840) | 388 (142,480) | 148 |
  | 46130990489 | jungle | 591 (29,380) | 363 (120,400) | 93 |
  | 46130992689 | pale-garden | 1,330 (66,240) | 348 (128,260) | 160 |

- **Cause (structural)**: the fallback's level tunnel (`Stairs::level` on the held heading's cardinal) is the same tunnel every time. `Plan::mine` takes the heading from `self.heading`, which a tunnel that ends "done" never turns. At a face it cannot cut (refused: lava beside, a falling block, no floor), the tunnel's level step turns a quarter inside the task, and a cell already open is never refused, so it turns back into its own tunnel and walks it, each step counted as a stair. Six such steps are "done", and the next task starts on the same heading at the same face. Nothing measures what a tunnel showed: there is no invariant on new rock opened or ore seen per block dug.
- **A second cause (typecraft)**: the bots end deep: final feet y -40, -5, -16, -23, -27, -41, -23, 15, -12, -20, -53, -19, -35, -35 (median about -23). The dig toward ore below (`Stairs::new` toward it) and short drops take them down and nothing brings them up. Below y 0 the rock is deepslate (twice stone's dig time) and the middle band thins to nothing at y -24.
- **Not this file's walls** (read, left to their owners): pale-garden's mountain bots died to skeleton arrows walking at exposed mountain iron (7 of 14 deaths); badlands had no logs (G3's own note); jungle lost 4 bots to creepers.

### The fix (this branch)
`crates/plan/src/iron.rs`, used by the iron rows (iron ore, deepslate iron ore) at their depth, before the old tunnel:
1. **Rise to the band**: feet under `rise_below` (y 0) cut stairs up (`Stairs::up`), noted "rise toward iron's band".
2. **A cave seen**: an open cave cell (a cross of open cells at the feet's height with room over it and a floor under it, which a one-wide tunnel never makes) 24 to 48 blocks off on the plane, inside y -24 to 56, not near one already taken, **with a clear line of sight from the eye** (at most `cave_rays` 64 lines traced a search, nearest first), is walked to; with no route it is dug toward (at most 8 tasks). Noted "walk into the cave at …" or "dig toward a cave at …".
3. **A strip in fresh rock**: the level tunnel goes only on a heading whose next 3 columns hold no cell seen open from the eye and whose first step is not refused, noted "strip begun …" or "strip turned from open cells ahead …". A strip that runs `strip_len` (24) blocks, or makes no progress for `strip_stalls` (2) tunnels in a row, is ended and turned a quarter, left and right in turn, noted with its blocks and the raw iron it gave.
4. **No fresh rock on any heading**: stairs down while the feet are over both the depth and y 6 (the rise line plus a staircase), else stairs up, never the old shuttle, noted "no fresh rock on any heading" and "stairs down for fresh rock" or "stairs up for fresh rock".

### The near miss: a cave read from the chunk data (8f26631f, caught before any judging)
- **What it did**: the first version's `cave_near` took any cave-shaped air within 48 blocks from `w.block_at`, so a cave sealed behind 40 blocks of rock counted as seen. That is air read from loaded chunk data, not a cave the bot observed: X-ray. `fresh` likewise turned from air ahead whether or not it had been seen.
- **Caught by**: the director, on review, before any judging race. The two short races on it (46162793178 typecraft, 46162796333 far-stronghold) were cancelled.
- **Fix**: a cave counts only with a line of sight from the eye through cells seen open (`iron::cave_seen`, `defend::sight`); `fresh` counts unseen air as rock. Tests: `a_cave_sealed_behind_rock_is_not_sought`, `a_cave_seen_beyond_view_is_found_and_a_tunnel_is_not` (a seen room taken, a sealed one beside it never), `unseen_air_ahead_is_rock_to_a_strip` (plan), and the fleet guard `no_target_in_rock_never_seen` (bot, crates/bot/tests/no_xray.rs): the whole planner closed loop against the model with sealed caves and sealed iron ore round the descent, every target it holds each tick checked against them.
