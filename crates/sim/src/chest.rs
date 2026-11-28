//! A chest as the server keeps it (issue 77, brain/ruststeve/design/chests.md), read from the
//! 26.1.2 jar:
//!
//! - **The block entity** (`ChestBlockEntity`): 27 slots, kept when the menu closes and when no
//!   one is near. The model's chests are single; a double chest's `generic_9x6` is not modelled.
//! - **Open** (`ChestBlock.useWithoutItem`, then `openMenu`): the next container id,
//!   `open_screen` with menu `generic_9x3`, then the whole menu (`initMenu`). The menu's slots:
//!   the chest's 27, then the inventory's main 27 and hotbar 9 (window 0's 9-44).
//! - **Shift-click** (`ChestMenu.quickMoveStack`): a chest slot moves to the inventory, end
//!   first (`moveItemStackTo(stack, 27, 63, true)`); an inventory slot moves to the chest, start
//!   first (`moveItemStackTo(stack, 0, 27, false)`). Each pass fills stacks of the same item
//!   first, then empty slots; what does not fit stays in the clicked slot.
//! - **Close** (`ChestMenu.removed`): the chest keeps its slots; the cursor goes back to the
//!   inventory.
//! - **Blocked** (`ChestBlock.isChestBlockedAt`): a redstone conductor over it and it does not
//!   open; the model takes a full collision cube for a conductor.
//! - **Still valid** (`Container.stillValidBlockEntity`): as the table's, the block there and
//!   the player within 8 blocks.

use data::BlockKind;
use protocol::{EncodeError, S2c, State};

use crate::survival::{MENU_SLOTS, Stack};
use crate::{Sim, Wire, wire};

/// A single chest's slots (`ChestBlockEntity.getContainerSize`).
pub const SLOTS: usize = 27;
/// Its menu's slots: its 27, then the inventory's 36.
pub const MENU: usize = SLOTS + 36;

/// A chest's block entity: its slots.
#[derive(Clone, Debug, PartialEq)]
pub struct Chest {
    pub items: [Option<Stack>; SLOTS],
}

impl Default for Chest {
    fn default() -> Self {
        Self {
            items: [None; SLOTS],
        }
    }
}

impl Chest {
    /// Every kind and its count, as the chest holds them.
    pub fn count(&self, item: data::ItemKind) -> i32 {
        self.items
            .iter()
            .flatten()
            .filter(|(k, _)| *k == item)
            .map(|(_, n)| n)
            .sum()
    }
}

/// A chest's open menu (`ChestMenu`, `generic_9x3`): its slots live in the block entity.
#[derive(Clone, Debug, PartialEq)]
pub struct ChestMenu {
    pub id: i32,
    pub pos: (i32, i32, i32),
    pub state_id: i32,
    pub(crate) remote: [Option<Stack>; MENU_SLOTS],
}

/// `ChestMenu.quickMoveStack`'s target for a clicked menu slot: the slots tried and whether in
/// reverse. A chest slot goes to the inventory from its end; an inventory slot to the chest
/// from its start.
pub fn quick_move_target(slot: usize) -> (std::ops::Range<usize>, bool) {
    if slot < SLOTS {
        (SLOTS..MENU, true)
    } else {
        (0..SLOTS, false)
    }
}

/// A chest menu's slot in window 0's numbering, for the inventory's part (27-62 to 9-44).
pub fn inventory_slot(slot: usize) -> Option<usize> {
    (SLOTS..MENU).contains(&slot).then(|| slot - SLOTS + 9)
}

impl Sim {
    /// `openMenu` for a chest: its block entity made if new, the next container id,
    /// `open_screen` (`generic_9x3`) and the whole menu.
    pub(crate) fn open_chest(&mut self, pos: (i32, i32, i32)) -> Result<Vec<Wire>, EncodeError> {
        // `ChestBlock.isChestBlockedAt`: a redstone conductor over it (taken as a full collision
        // cube) and nothing opens.
        let above = crate::survival::block_kind(self, (pos.0, pos.1 + 1, pos.2));
        if above
            .and_then(data::generated::shapes::collision)
            .is_some_and(|c| c == data::types::Collision::Full)
        {
            return Ok(Vec::new());
        }
        let mut out = self.close_open_menu()?;
        self.survival.chests.entry(pos).or_default();
        let id = self.survival.container_counter % 100 + 1;
        self.survival.container_counter = id;
        self.survival.chest = Some(ChestMenu {
            id,
            pos,
            state_id: 0,
            remote: [None; MENU_SLOTS],
        });
        let menu = data::generated::registries::MENU
            .iter()
            .position(|m| *m == "generic_9x3")
            .and_then(|i| i32::try_from(i).ok())
            .unwrap_or(-1);
        out.push(wire(State::Play, &S2c::OpenScreen { window: id, menu })?);
        out.push(self.full_state()?);
        Ok(out)
    }

    /// Chests whose block is gone are forgotten (their items would drop; the model's chests are
    /// only broken empty).
    pub(crate) fn forget_broken_chests(&mut self) {
        let gone: Vec<(i32, i32, i32)> = self
            .survival
            .chests
            .keys()
            .copied()
            .filter(|p| crate::survival::block_kind(self, *p) != Some(BlockKind::Chest))
            .collect();
        for p in gone {
            self.survival.chests.remove(&p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The jar's rule: chest slots out to the inventory's end, inventory slots into the chest's
    /// start.
    #[test]
    fn quick_move_goes_chest_to_inventory_end_and_inventory_to_chest_start() {
        assert_eq!(quick_move_target(0), (27..63, true));
        assert_eq!(quick_move_target(26), (27..63, true));
        assert_eq!(quick_move_target(27), (0..27, false));
        assert_eq!(quick_move_target(62), (0..27, false));
        assert_eq!(inventory_slot(27), Some(9));
        assert_eq!(inventory_slot(54), Some(36));
        assert_eq!(inventory_slot(62), Some(44));
        assert_eq!(inventory_slot(26), None);
        assert_eq!(inventory_slot(63), None);
    }
}
