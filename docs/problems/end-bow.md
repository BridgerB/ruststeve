# Issue 20, the bow line (agent E8)

The orchestrator's index is [`end.md`](end.md); this file is agent E8's. Branch `issue-20-bow`, a separate line: it is merged into `issue-20-end` only once a real bow bot reaches `island` or further. Code: `crates/plan/src/bow.rs` (the pure aim and the shot), the bow kit in `crates/plan/src/end_portal.rs` (the `Bow` phase of the End kit), the stage `bow_armed`; frontier `config/frontier-bow.toml`.

## Needs
Every real player breaks the End's open crystals with a bow from the ground and climbs only for the two caged ones. Our pillar line climbs every spike, and the time up high is what kills it (knock-offs, falls) and what uses up its pickaxes (docs/problems/end-crystals.md, races 4 to 34). The bow line goes back to the overworld from the `armed` snapshot, makes a bow and arrows, then takes the existing chain to `end` and `island` and shoots each open crystal from the ground; the caged ones fall back to E4's pillar.

## Done (world terms)
1. Parity row e07: on a real server, an arrow from the ground destroys an end crystal at range.
2. A real bow-line race from `armed` reaches `bow_armed` (a bow and arrows held in the overworld with the End kit), then `end` and `island`, each with its snapshot, and `config/frontier-bow.toml` moves with it.
3. In the End, a ledger with "bow: crystal N gone by arrow" for open crystals and no climb for them.

## The jar (probe 46039452326, `javap -c -p -constants` of 26.1.2 and its data files)
- **The draw** (`BowItem.releaseUsing`, `getPowerForTime`): the power of a draw held `t` ticks is `f = t / 20; f = (f * f + 2 f) / 3`, at most 1.0, so **full power at 20 ticks**. Under 0.1 (3 ticks) nothing is shot. A shot needs an arrow in the inventory (`Player.getProjectile`).
- **The shot** (`BowItem.shootProjectile`, `Projectile.shootFromRotation`, `getMovementToShoot`): along the player's own rotation at release, `(-sin yaw cos pitch, -sin pitch, cos yaw cos pitch)`, normalised, each axis plus `triangle(0, 0.0172275 x inaccuracy)`, then scaled by the speed **`power x 3.0`** (3.0 at full power), inaccuracy **1.0**; a crit arrow at power 1.0. The shooter's own movement is added (its y only off the ground), so a shot is made standing still.
  - The spread's standard deviation is 0.0172275 / sqrt(6) = 0.0070 per axis of a unit vector: 0.34 blocks across at 48 blocks.
- **The release** comes from the client's `player_action` status 5 (RELEASE_USE_ITEM), our `Act::ReleaseUse`; the use starts with `use_item` (our `Act::UseItem`), whose packet carries the look and the server snaps to it. The release is along the rotation the server last heard.
- **The arrow's start** (`AbstractArrow(type, owner, level, ...)`): the shooter's x and z, `getEyeY() - 0.1`.
- **The arrow's flight** (`AbstractArrow.tick`): each tick a block clip (`ClipContext.Block.COLLIDER`, fluids NONE) from its position to position + velocity, entities hit along that segment, the move; then **velocity x 0.99** (`INERTIA`; 0.6 in water, `WATER_INERTIA`), then **gravity 0.05** off its y (`getDefaultGravity`). So per tick: `p += v; v *= 0.99; v.y -= 0.05`.
- **What it hits** (`findHitEntities`, `ProjectileUtil.getManyEntityHitResult`, `computeMargin`): the segment against each entity's box grown by a margin of `clamp((ticks alive - 2) / 20, 0, 0.3)`, then a block clip from the hit point to the box's centre. An end crystal's box is 2 by 2 by 2 over its position (`EntityType.END_CRYSTAL`), so 2.6 across after 8 ticks.
- **An arrow destroys an end crystal** (`EndCrystal.hurtServer`, read again from probe 45863431767): any damage whose source entity is not the dragon removes it and sets off a power-6 explosion at its position unless the damage was itself an explosion. An arrow's source entity is its owner, the player. The blast hurts nothing farther than 12 (`ServerExplosion`: impact `1 - distance / 12`), so a shot from 16 or more is safe from it.
- **Iron bars stop arrows** (`IronBarsBlock` is a `CrossCollisionBlock(2, 16, 2, 16, 16)`): the post is 2 px wide and each connected side 2 px thick and full height, so a cage's connected wall and its roof are a full plane under `COLLIDER`. **A caged crystal cannot be shot**; it goes to E4's pillar.
- **The carved pumpkin** (`EnderMan.isBeingStaredBy`): a player that fails `LivingEntity.PLAYER_NOT_WEARING_DISGUISE_ITEM` (a carved pumpkin on the head) is never stared at. A carved pumpkin is made by shears used on a pumpkin block (`PumpkinBlock.useItemOn`).
- **The recipes** (data/minecraft/recipe): bow = 3 sticks and 3 string, 2 by 3 (a table); arrow x4 = flint over a stick over a feather, 1 by 3 (a table); shears = 2 iron ingots on a diagonal (2 by 2).
- **The sources** (data/minecraft/loot_table): a cobweb broken without shears or silk touch drops 1 string; a spider 0 to 2 string; a chicken 0 to 2 feathers; gravel flint one time in ten (the redig row already makes flint); a skeleton 0 to 2 arrows.

## The start (race 45882199358 lane 0, `snapshot-armed-…-lane-0`)
Not in the portal room: on the surface at (1612.2, 69.0, -96.4), 60 off its stairs' top, at dusk (server tick 13,820; end-portal.md wall 13). Its `snap.toml`: iron helmet, chestplate, leggings and boots, an iron sword, 2 stone swords, a shield, 3 stone pickaxes, 24 cooked food, 13 eyes, a crafting table, 1 oak log, 8 planks, **15 gravel**, **1 iron ingot** (no raw iron: no shears without more iron), 2 water buckets, a bucket, flint and steel, 90 cobblestone, 64 cobbled deepslate, 148 netherrack.
So the bow wants: 3 string, feathers and flint (one each per 4 arrows), and sticks (3 for the bow, one per 4 arrows, 2 for the spare pickaxe): about 4 more logs.

## Walls
(none yet)
