//! The dig rules both the bot and the server model use (brain/game/mc-dig-timing.md): vanilla's
//! `Player.getDestroySpeed` and `BlockBehaviour.getDestroyProgress`, read from the 26.1.2 jar, over
//! the generated tables (hardness, needs-correct-tool, and each tool's speeds and drops from its
//! `tool` component).

use crate::generated::hardness::{hardness, needs_correct_tool};
use crate::generated::item_props::tool_of;
use crate::{BlockKind, ItemKind};

/// The held item's speed against a block: a tool's (`Item.getDestroySpeed`), else 1.0. The
/// `mining_efficiency` attribute (0 without enchantments) and effects are not modelled.
pub fn speed(block: BlockKind, held: Option<ItemKind>) -> f32 {
    held.and_then(tool_of).map_or(1.0, |t| t.speed(block))
}

/// Whether breaking the block with the held item gets its drops (`Player.hasCorrectToolForDrops`):
/// it needs no tool, or the held tool is correct for it.
pub fn harvests(block: BlockKind, held: Option<ItemKind>) -> bool {
    !needs_correct_tool(block).unwrap_or(false)
        || held.and_then(tool_of).is_some_and(|t| t.correct_for(block))
}

/// One tick's dig progress (`getDestroyProgress`): the speed (× 0.2 with the eyes in water, ÷ 5
/// off the ground) over the hardness, over 30 when it harvests, else 100. Infinite for hardness
/// 0 (it breaks at once), 0 for an unbreakable block.
pub fn progress(
    block: BlockKind,
    held: Option<ItemKind>,
    eye_in_water: bool,
    on_ground: bool,
) -> f32 {
    let h = hardness(block).unwrap_or(0.0);
    if h < 0.0 {
        return 0.0;
    }
    if h == 0.0 {
        return f32::INFINITY;
    }
    let mut s = speed(block, held);
    if eye_in_water {
        s *= 0.2;
    }
    if !on_ground {
        s /= 5.0;
    }
    s / h / if harvests(block, held) { 30.0 } else { 100.0 }
}

/// Ticks to break by the server's count: the fewest n with `progress × n ≥ 1` in `float`, the
/// server's own comparison (`ceil(1 / progress)` in doubles is a tick high at times); `None` for an
/// unbreakable block.
pub fn break_ticks(
    block: BlockKind,
    held: Option<ItemKind>,
    eye_in_water: bool,
    on_ground: bool,
) -> Option<u64> {
    let p = progress(block, held, eye_in_water, on_ground);
    if p <= 0.0 {
        return None;
    }
    if p.is_infinite() {
        return Some(0);
    }
    let mut n = (1.0 / f64::from(p)).ceil() as u64;
    while n > 1 && p * ((n - 1) as f32) >= 1.0 {
        n -= 1;
    }
    while p * (n as f32) < 1.0 {
        n += 1;
    }
    Some(n)
}

/// The fastest tool among held items for a block that also gets its drops, if any beats the hand.
pub fn best_tool(block: BlockKind, items: impl IntoIterator<Item = ItemKind>) -> Option<ItemKind> {
    items
        .into_iter()
        .filter(|i| tool_of(*i).is_some() && harvests(block, Some(*i)))
        .filter(|i| speed(block, Some(*i)) > 1.0)
        .max_by(|a, b| speed(block, Some(*a)).total_cmp(&speed(block, Some(*b))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stone_by_hand_and_by_pickaxe() {
        // Stone, hardness 1.5: by hand it does not harvest, 1 / 1.5 / 100 → 150 ticks, no drop;
        // a wooden pickaxe (speed 2) harvests: 2 / 1.5 / 30 → 23 ticks; stone (speed 4) 12.
        assert_eq!(break_ticks(BlockKind::Stone, None, false, true), Some(150));
        assert!(!harvests(BlockKind::Stone, None));
        assert_eq!(
            break_ticks(BlockKind::Stone, Some(ItemKind::WoodenPickaxe), false, true),
            Some(23)
        );
        assert!(harvests(BlockKind::Stone, Some(ItemKind::WoodenPickaxe)));
        assert_eq!(
            break_ticks(BlockKind::Stone, Some(ItemKind::StonePickaxe), false, true),
            Some(12)
        );
        // A wooden pickaxe does not get iron ore's drops; a stone one does.
        assert!(!harvests(BlockKind::IronOre, Some(ItemKind::WoodenPickaxe)));
        assert!(harvests(BlockKind::IronOre, Some(ItemKind::StonePickaxe)));
        // A log by hand: 60 ticks; a pickaxe is no faster on it.
        assert_eq!(break_ticks(BlockKind::OakLog, None, false, true), Some(60));
        assert_eq!(
            best_tool(
                BlockKind::Stone,
                [ItemKind::OakLog, ItemKind::WoodenPickaxe]
            ),
            Some(ItemKind::WoodenPickaxe)
        );
        assert_eq!(
            best_tool(BlockKind::OakLog, [ItemKind::WoodenPickaxe]),
            None
        );
    }
}
