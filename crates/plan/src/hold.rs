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

/// The next step toward holding `item`.
pub fn toward(w: &World, item: ItemKind) -> Hold {
    if held_item(w) == Some(item) {
        return Hold::Held;
    }
    if w.click.is_some() || w.window.is_some() {
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
        (Some(_), _) => match (9i16..=35).find(|s| !w.inventory.slots.contains_key(s)) {
            // What the swap brought back goes down in an empty slot.
            Some(s) => click(s),
            None => Hold::Missing,
        },
        (None, Some(s)) => click(s),
        (None, None) => Hold::Missing,
    }
}
