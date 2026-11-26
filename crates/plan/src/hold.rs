//! Holding an item (brain/ruststeve/design/tools-and-held-slot.md): the selected hotbar slot
//! must hold it before a dig or a placement, checked on the world, never assumed (clojurecraft
//! and ruststeve v1 both lost digs to a drifted held slot). An item in the main inventory is
//! swapped into the hotbar's last slot first: pick it up, click the hotbar slot (a swap), put
//! what came back down where the item was, one click per round trip.

use data::ItemKind;
use world::{Act, World};

/// The hotbar slot an item from the main inventory is brought to (window 0's 44).
const SWAP_SLOT: i16 = 44;

/// Window 0's slot for hotbar slot `i` (0-8).
fn hotbar(i: u8) -> i16 {
    36 + i16::from(i)
}

/// The item in the selected hotbar slot.
pub fn held_item(w: &World) -> Option<ItemKind> {
    w.inventory.slots.get(&hotbar(w.held)).map(|(k, _)| *k)
}

/// Where holding an item stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hold {
    /// The selected slot holds it.
    Held,
    /// Send this, then look again.
    Act(Act),
    /// A click is in flight, or another window is open: wait.
    Wait,
    /// It is not in the inventory.
    Missing,
}

/// An empty slot for a stack on the cursor: the main inventory first, then the hotbar.
pub(crate) fn put_down_slot(w: &World) -> Option<i16> {
    (9i16..=44).find(|s| !w.inventory.slots.contains_key(s))
}

/// The next step toward holding `item`.
pub fn toward(w: &World, item: ItemKind) -> Hold {
    let busy = w.click.is_some() || w.window.is_some();
    if held_item(w) == Some(item) {
        // The swap's last click: what came back on the cursor goes down before the item counts
        // as held (blaze.md wall 17: a pickaxe swapped out for the bucket stayed on the cursor,
        // in no slot, and the cast paused for want of it for ever).
        return match (w.inventory.carried, put_down_slot(w)) {
            (Some(_), _) if busy => Hold::Wait,
            (Some(_), Some(slot)) => Hold::Act(Act::Click {
                slot,
                button: 0,
                mode: 0,
            }),
            _ => Hold::Held,
        };
    }
    if busy {
        return Hold::Wait;
    }
    if let Some(i) = (0u8..9).find(|i| {
        w.inventory
            .slots
            .get(&hotbar(*i))
            .is_some_and(|(k, _)| *k == item)
    }) {
        return Hold::Act(Act::Hold { slot: i });
    }
    let click = |slot| {
        Hold::Act(Act::Click {
            slot,
            button: 0,
            mode: 0,
        })
    };
    // In the main inventory: a swap through the cursor into the hotbar's last slot.
    let source = (9i16..=35).find(|s| w.inventory.slots.get(s).is_some_and(|(k, _)| *k == item));
    match (w.inventory.carried, source) {
        (Some((k, _)), _) if k == item => click(SWAP_SLOT),
        (Some(_), _) => match (
            (9i16..=35).find(|s| !w.inventory.slots.contains_key(s)),
            source,
        ) {
            // What the swap brought back goes down in an empty slot.
            (Some(s), _) => click(s),
            // No empty slot: click the item itself, which swaps it onto the cursor for the
            // hotbar (blaze.md wall 13: a full inventory left a pickaxe on the cursor and every
            // lava scoop failed NoBucket with the empty bucket in slot 34).
            (None, Some(s)) => click(s),
            (None, None) => Hold::Missing,
        },
        (None, Some(s)) => click(s),
        (None, None) => Hold::Missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blaze wall 13: every main slot full, a pickaxe left on the cursor, the bucket in slot 34:
    /// the bucket's own slot is clicked (a swap onto the cursor), never `Missing`.
    #[test]
    fn a_full_inventory_with_a_carried_stack_still_reaches_the_item() {
        let mut w = World::new();
        for s in 9i16..=44 {
            w.inventory.slots.insert(s, (ItemKind::Cobblestone, 64));
        }
        w.inventory.slots.insert(34, (ItemKind::Bucket, 1));
        w.inventory.carried = Some((ItemKind::WoodenPickaxe, 1));
        assert_eq!(
            toward(&w, ItemKind::Bucket),
            Hold::Act(Act::Click {
                slot: 34,
                button: 0,
                mode: 0
            })
        );
        // No bucket anywhere and no empty slot: nothing to reach.
        w.inventory.slots.insert(34, (ItemKind::Cobblestone, 64));
        assert_eq!(toward(&w, ItemKind::Bucket), Hold::Missing);
    }

    /// Blaze wall 17: the bucket swapped into the hand, the pickaxe still on the cursor: it goes
    /// down in an empty slot first; with no empty slot the bucket is held as it is.
    #[test]
    fn a_stack_left_on_the_cursor_is_put_down_before_held() {
        let mut w = World::new();
        for s in (9i16..=43).filter(|s| *s != 16) {
            w.inventory.slots.insert(s, (ItemKind::Cobblestone, 64));
        }
        w.inventory.slots.insert(44, (ItemKind::Bucket, 1));
        w.held = 8;
        w.inventory.carried = Some((ItemKind::StonePickaxe, 1));
        assert_eq!(
            toward(&w, ItemKind::Bucket),
            Hold::Act(Act::Click {
                slot: 16,
                button: 0,
                mode: 0
            })
        );
        w.inventory.slots.insert(16, (ItemKind::Gravel, 7));
        assert_eq!(toward(&w, ItemKind::Bucket), Hold::Held);
        w.inventory.carried = None;
        assert_eq!(toward(&w, ItemKind::Bucket), Hold::Held);
    }
}
