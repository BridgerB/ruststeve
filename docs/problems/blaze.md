# Issue 16: blaze rods (one bot first)

## Needs
From the frontier fortress snapshot, one real bot holds 7 blaze rods (`rods_7`), its snapshot published and made the frontier (`brain/ruststeve/decisions/one-bot-first.md`). Races run `revive=false`: a death ends the attempt (Bridger, 2026-11-20: the compute stays in the Nether).

## Done (world terms)
One case file from a race resumed at the frontier shows `stage reached: rods_7`, and the race's release carries its `snapshot-rods_7-…` asset.

## Risks
Blazes hover out of reach; burning (45% of clojurecraft's lost health; no water in the Nether); volleys in the open (9 of 20 of clojurecraft's deaths); the fortress maze between the gate and the spawner; tick-paced swings bunching on a loaded server; no food to heal with.

## 16a: food first, then a fresh frontier
- **Why**: the frontier fortress snapshot (race 44678197603) came from a bot that started beside a built portal with no food; healing between blaze fights needs food over 18. Bridger (2026-11-20): the bot gathers enough food before the Nether, and a fresh snapshot is taken once it is ready.
- **Built**: `food_reserve` 32 hunger points in the survival rules; `eat::food_points`; the `hunt` row also fires while the `race`/`nether` aim is short of the reserve in the overworld; with a portal to enter and the reserve short, the bot explores for animals before stepping in ("provisions N of 32"); `snap.toml` lists the held items.
