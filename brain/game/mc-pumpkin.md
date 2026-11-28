---
title: Pumpkins and the carved pumpkin
description: The vanilla 26.1.2 facts behind wearing a carved pumpkin against endermen - the stare check and the head slot, shears on a pumpkin, the carved pumpkin's hardness, tool and drop, the shears' recipe and wear, where pumpkin patches grow.
type: reference
tags: [game, pumpkin, endermen, shears, armour]
aliases: [carved pumpkin, pumpkin, shears, gaze disguise, gaze_disguise_equipment, patch_pumpkin, pumpkin patch, head slot]
status: verified
lastUpdated: 2026-11-28
verifiedAgainst: ruststeve@201dfcf
sourceRefs:
  - "ruststeve:crates/data/src/generated/hardness.rs#1.0,   // carved_pumpkin"
  - "ruststeve:crates/data/src/generated/recipes.rs#id: \"shears\","
  - "ruststeve:crates/data/src/generated/item_props.rs#238,  // shears"
  - "ruststeve:crates/sim/src/enderman.rs#pub fn stares_at(&self, at: [f64; 3]) -> bool {"
  - "ruststeve:config/moments.toml#id = \"e00-pumpkin\""
related:
  - "[[game/_moc|Game]]"
  - "[[mc-endermen]]"
  - "[[mc-mob-facts]]"
---

# Pumpkins and the carved pumpkin

A carved pumpkin worn on the head makes an enderman's stare test false: the bot can look any enderman in the eye and none is provoked by it. Measured from the 26.1.2 server jar (`javap -c -p -constants`, probe runs named below) and the generated tables.

## From the generated tables
- **Hardness**: pumpkin 1.0, carved pumpkin 1.0 (`hardness.rs`); best tool the axe for both (`harvest.rs`), no tool needed to drop; each drops itself (`loot.rs`).
- **Shears**: shaped 2 by 2, two iron ingots on the diagonal (`recipes.rs`); stack 1, 238 uses (`item_props.rs`).
- **The tag** `gaze_disguise_equipment` is [carved_pumpkin] (`tags.rs`).

## From the jar (probes 46159976076, 46160993651)
- **The stare** (`EnderMan.isBeingStaredBy`): `LivingEntity.PLAYER_NOT_WEARING_DISGUISE_ITEM` first; false, and it returns false with no look test. The predicate: not a player, true; a player, `getItemBySlot(HEAD).is(#gaze_disguise_equipment)` negated. Only the head counts: a pumpkin in the hand does nothing. Then `isLookingAtMe(player, 0.025, true, false, eyeY)` ([[mc-endermen]]).
- **Both callers** go through it: `EndermanLookForPlayerGoal` (its start's `isAngerInducing` selector, and, once targeted, the teleport away from a stare within distance squared 16) and `EndermanFreezeWhenLookedAt`. So the pumpkin stops a new anger from a look and nothing else: an angry enderman stays angry (anger is kept, [[mc-mob-facts]]), and with the pumpkin on it is never stared at, so it never teleports away from a look within 4. A hit still angers it.
- **The head slot**: the item's `equippable` is `{slot: head, swappable: false}` (the reports' default components) and its only `attribute_modifiers` entry is `waypoint_transmit_range` -1: no armour, no toughness. Stack 64.
- **Shift-click** (`InventoryMenu.quickMoveStack`): from the inventory (9..44), `getEquipmentSlotForItem` (the equippable's slot) is HEAD, of type `HUMANOID_ARMOR`: menu slot `8 - HEAD.getIndex()` = 5. Empty, the pumpkin goes there. Holding a helmet, there is no swap: the click falls through to the plain move between the main rows (9..35) and the hotbar (36..44). The helmet comes off first (a shift-click on slot 5), then the pumpkin goes on (one: `ArmorSlot.getMaxStackSize` is 1, `mayPlace` is `isEquippableInSlot`). `swappable: false`: using it from the hand does not swap it with a worn helmet either.
- **Shears on a pumpkin** (`PumpkinBlock.useItemOn`): only `Items.SHEARS` in the using hand. The face is the hit face; a top or bottom hit takes `player.getDirection().getOpposite()` (toward the player). The loot table `BuiltInLootTables.CARVE_PUMPKIN` (`loot_table/carve/pumpkin.json`: 4 `pumpkin_seeds`, probe 46160993651) drops as item entities at the block's centre plus 0.65 along the face, y + 0.1, moving 0.05 out of the face (plus up to 0.02 random) and 0.05 up. The sound `PUMPKIN_CARVE` (blocks, volume 1, pitch 1) is played for everyone, the player too. `setBlock(pos, carved_pumpkin[facing=face], 11)`: the block turns into a carved pumpkin in place (a block update; no item yet). The shears take 1 damage (`hurtAndBreak(1, ...)`); a `SHEAR` game event; the stat `ITEM_USED`.
- **The carved pumpkin then has to be dug**: hardness 1, axe; by the break rule ([[mc-dig-timing]]) 30 ticks by hand, 15 with a wooden axe, 8 stone, 5 iron. It drops itself.
- **Patches** (`placed_feature/patch_pumpkin.json`): `rarity_filter` chance 300 (a chunk in 300), `in_square`, the `MOTION_BLOCKING` heightmap, `biome`, 96 tries spread by a trapezoid of 7 across and 3 up or down, each placed only into air over `grass_block`. The feature is a plain (uncarved) pumpkin. 46 of the 65 biome files list it (plains, forest, taiga, savanna, jungle, swamp, birch and dark forest, snowy plains and taiga among them; no meadow, no cherry grove); where there is no grass (desert, badlands, oceans) it places nothing. `pile_pumpkin` is the village pile.

## Proven on a real server
- Parity row `e00-pumpkin` ([[parity-table]]): `e00-stare`'s look in the eye at 12, a carved pumpkin on the head: the enderman stays neutral and never stared, in real and in the model (parity 46160481997, 2 of 2 with `e00-stare`).

## What the pumpkin does not stop: the dragon's anger (probe 46160185304)
- **Target goals** (`EnderMan.registerGoals`): 1 `EndermanLookForPlayerGoal` (the stare, the only goal that picks a player on its own), 2 `HurtByTargetGoal` with no classes and no `setAlertOthers` (no others alerted), 3 `NearestAttackableTargetGoal<Endermite>`, 4 `ResetUniversalAngerTargetGoal`.
- **The creepy flag** (`EnderMan.setTarget`): any target, dragon or player, sets `DATA_CREEPY` and the attack speed; no target clears creepy and `DATA_STARED_AT`. Creepy says "has a target", never whom.
- **Hurt by the dragon**: `LivingEntity` sets `lastHurtByMob` from a source whose entity is living and not `#no_anger` (only `mob_attack_no_aggro`). The dragon's blows are `mobAttack(dragon)`: `EnderDragon.knockBack` (both wings' boxes grown 4, 2, 4 and moved 2 down, `NO_CREATIVE_OR_SPECTATOR`: every mob) pushes and, while the dragon is not sitting and the mob was not hurt in the last 2 ticks, hurts **5**; `EnderDragon.hurt` (the head's and neck's boxes grown 1) hurts **10**. Its breath (`DragonFireball` on hit): an `AreaEffectCloud` owned by the dragon, radius 3 growing to 7 over 600 ticks, `INSTANT_DAMAGE` amplifier 1; the cloud applies it with factor 0.5 (`HealOrHarmMobEffect`: `(int)(0.5 × (6 << 1) + 0.5)` = **6**) as `indirectMagic(cloud, dragon)`, reapplied every 20 ticks. Each makes the dragon the enderman's hurt-by target: it turns creepy against the dragon.
- **Kept, then dropped** (`TargetGoal.canContinueToUse`, `mustSee` true): while the dragon is within the follow range (64), attackable and seen at least every `reducedTickDelay(60)`; then `setTarget(null)` and the creepy flag clears.
- **When it turns on the player**: only by goal 1 (a stare, which the pumpkin removes) or goal 2 on a new hurt by the player. An enderman angry at the dragon is not alerted to the player, and a worn pumpkin leaves it on the dragon.

## Unread
- `NeutralMob`'s persistent anger timer for a dragon target; where the 3 after a 5 in the crystals races' ledgers comes from (no dragon damage is 3).

## See also
- [[mc-endermen]] - neutral until stared at or hit.
- [[mc-mob-facts]] - anger kept for the run, armour arithmetic.
- [[mc-dig-timing]] - the break rule behind the dig times.
