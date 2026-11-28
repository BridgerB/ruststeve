---
title: Pumpkins and the carved pumpkin
description: The vanilla 26.1.2 facts behind wearing a carved pumpkin against endermen - the stare check and the head slot, shears on a pumpkin, the carved pumpkin's hardness, tool and drop, the shears' recipe and wear, where pumpkin patches grow.
type: reference
tags: [game, pumpkin, endermen, shears, armour]
aliases: [carved pumpkin, pumpkin, shears, gaze disguise, gaze_disguise_equipment, patch_pumpkin, pumpkin patch, head slot]
status: draft
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

## From the jar
(Pending: probes 46159976076 and 46160185304.)

## See also
- [[mc-endermen]] - neutral until stared at or hit.
- [[mc-mob-facts]] - anger kept for the run, armour arithmetic.
