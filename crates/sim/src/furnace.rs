//! A furnace as the server keeps it (issue 13a, brain/ruststeve/design/smelting.md), read from the
//! 26.1.2 jar (`AbstractFurnaceBlockEntity`):
//!
//! - **The tick** (`serverTick`): a lit furnace burns one tick of its fuel. Lit, or with fuel and
//!   input both: when the input has a smelting recipe whose result fits the result slot
//!   (`canBurn`), an unlit furnace lights from the fuel (`getBurnDuration`, then `consumeFuel`:
//!   one taken, a bucket's remainder left); lit and able, the cook goes up one, and at the
//!   recipe's cook time the input becomes the result (`burn`). Not able, the cook resets. Neither
//!   lit nor fed, a cook in progress cools by 2 a tick (`BURN_COOL_SPEED`).
//! - **The input changed** (`setItem` slot 0, another item): the cook total becomes the new
//!   input's recipe time (200 without one) and the cook starts over.
//! - **The `lit` block state** follows whether it burns.
//! - Its window's properties (`dataAccess`): 0 lit time left, 1 lit total, 2 cook, 3 cook total.

use data::ItemKind;

use crate::survival::Stack;

pub const INPUT: usize = 0;
pub const FUEL: usize = 1;
pub const RESULT: usize = 2;
/// `getTotalCookTime` with no recipe for the input.
pub const DEFAULT_COOK: i32 = 200;
/// `BURN_COOL_SPEED`: what an unfed cook loses each tick.
pub const COOL_SPEED: i32 = 2;
/// The furnace's slots: input, fuel, result.
pub const SLOTS: usize = 3;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Furnace {
    pub items: [Option<Stack>; SLOTS],
    pub lit_remaining: i32,
    pub lit_total: i32,
    pub cook: i32,
    pub cook_total: i32,
}

/// The smelting recipe for an input item, if any.
pub fn recipe(item: ItemKind) -> Option<&'static data::types::Smelt> {
    data::generated::recipes::SMELTING
        .iter()
        .find(|r| r.input.contains(&item))
}

fn max_stack(item: ItemKind) -> i32 {
    data::generated::item_props::max_stack(item)
}

impl Furnace {
    pub fn lit(&self) -> bool {
        self.lit_remaining > 0
    }

    /// The window's four properties, in `dataAccess` order.
    pub fn properties(&self) -> [i32; 4] {
        [
            self.lit_remaining,
            self.lit_total,
            self.cook,
            self.cook_total,
        ]
    }

    /// `setItem`: a new input item restarts the cook with its recipe's time.
    pub fn set(&mut self, slot: usize, v: Option<Stack>) {
        let Some(cell) = self.items.get_mut(slot) else {
            return;
        };
        let changed = slot == INPUT && cell.map(|s| s.0) != v.map(|s| s.0);
        *cell = v;
        if changed {
            self.cook_total = v.and_then(|(k, _)| recipe(k)).map_or(DEFAULT_COOK, |r| {
                i32::try_from(r.ticks).unwrap_or(DEFAULT_COOK)
            });
            self.cook = 0;
        }
    }

    /// `canBurn`: a recipe for the input whose result the result slot can take.
    fn can_burn(&self) -> Option<Stack> {
        let (input, _) = self.items[INPUT]?;
        let r = recipe(input)?;
        let out = (r.result, i32::from(r.count));
        match self.items[RESULT] {
            None => Some(out),
            Some((k, n)) if k == out.0 && n + out.1 <= max_stack(k) => Some(out),
            Some(_) => None,
        }
    }

    /// One server tick; whether the `lit` state changed.
    pub fn tick(&mut self) -> bool {
        let was_lit = self.lit();
        if self.lit() {
            self.lit_remaining -= 1;
        }
        let has_fuel = self.items[FUEL].is_some();
        let has_input = self.items[INPUT].is_some();
        if self.lit() || (has_fuel && has_input) {
            match self.can_burn() {
                Some(out) => {
                    if !self.lit()
                        && let Some((fuel, n)) = self.items[FUEL]
                    {
                        let burn = data::generated::fuel::fuel_ticks(fuel)
                            .and_then(|t| i32::try_from(t).ok())
                            .unwrap_or(0);
                        self.lit_remaining = burn;
                        self.lit_total = burn;
                        if burn > 0 {
                            // `consumeFuel`: one taken; an emptied bucket of lava leaves the bucket.
                            self.items[FUEL] = if n > 1 {
                                Some((fuel, n - 1))
                            } else if fuel == ItemKind::LavaBucket {
                                Some((ItemKind::Bucket, 1))
                            } else {
                                None
                            };
                        }
                    }
                    if self.lit() {
                        self.cook += 1;
                        if self.cook == self.cook_total {
                            self.cook = 0;
                            self.cook_total = self.items[INPUT]
                                .and_then(|(k, _)| recipe(k))
                                .map_or(DEFAULT_COOK, |r| {
                                    i32::try_from(r.ticks).unwrap_or(DEFAULT_COOK)
                                });
                            // `burn`: the result added, one input taken.
                            self.items[RESULT] = match self.items[RESULT] {
                                Some((k, n)) => Some((k, n + out.1)),
                                None => Some(out),
                            };
                            self.items[INPUT] =
                                self.items[INPUT].and_then(|(k, n)| (n > 1).then_some((k, n - 1)));
                        }
                    } else {
                        self.cook = 0;
                    }
                }
                None => self.cook = 0,
            }
        } else if self.cook > 0 {
            self.cook = (self.cook - COOL_SPEED).clamp(0, self.cook_total);
        }
        was_lit != self.lit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_coal_smelts_raw_iron_in_200_ticks_each() {
        let mut f = Furnace::default();
        f.set(INPUT, Some((ItemKind::RawIron, 3)));
        f.set(FUEL, Some((ItemKind::Coal, 1)));
        let lit_changes = (0..600).filter(|_| f.tick()).count();
        assert_eq!(f.items[RESULT], Some((ItemKind::IronIngot, 3)));
        assert_eq!(f.items[INPUT], None);
        assert_eq!(f.items[FUEL], None);
        // Lit once; coal burns 1600 ticks, so it is still lit.
        assert_eq!(lit_changes, 1);
        assert!(f.lit());
    }

    #[test]
    fn a_plank_burns_out_after_one_and_a_half_items() {
        let mut f = Furnace::default();
        f.set(INPUT, Some((ItemKind::RawIron, 3)));
        f.set(FUEL, Some((ItemKind::OakPlanks, 1)));
        for _ in 0..600 {
            f.tick();
        }
        assert_eq!(f.items[RESULT], Some((ItemKind::IronIngot, 1)));
        assert!(!f.lit());
        // The second item's cook cooled back to nothing.
        assert_eq!(f.cook, 0);
    }
}
