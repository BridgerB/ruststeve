//! The shapes the generated tables are made of. Hand-written; the values are generated.

use crate::generated::blocks::BlockKind;
use crate::generated::items::ItemKind;

/// The server version the tables were generated from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Version {
    pub id: &'static str,
    pub protocol: u32,
    pub world: u32,
    pub java: u32,
}

/// One block property and its values, in the order the state ids count through them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Property {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

/// A block's state ids (`first_state..=last_state`, the last property counting fastest), its
/// default state and its properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockInfo {
    pub name: &'static str,
    pub first_state: u32,
    pub last_state: u32,
    pub default_state: u32,
    pub properties: &'static [Property],
}

/// How a crafting recipe is laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A pattern trimmed like vanilla trims it; `cells` row by row, `None` an empty cell. Each
    /// cell accepts any of its items.
    Shaped {
        width: u8,
        height: u8,
        cells: &'static [Option<&'static [ItemKind]>],
    },
    /// Ingredients in any cells; each accepts any of its items.
    Shapeless {
        ingredients: &'static [&'static [ItemKind]],
    },
}

/// A shaped or shapeless crafting recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Recipe {
    pub id: &'static str,
    pub result: ItemKind,
    pub count: u8,
    pub shape: Shape,
}

/// A furnace recipe: any one of `input` cooks into `count` of `result` in `ticks` game ticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Smelt {
    pub id: &'static str,
    pub input: &'static [ItemKind],
    pub result: ItemKind,
    pub count: u8,
    pub ticks: u32,
}

/// One loot try: `item` drops with probability `chance` (1.0 always).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drop {
    pub item: ItemKind,
    pub chance: f64,
}

/// A block's loot tries, in the order the game tries them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlockDrops {
    pub block: BlockKind,
    pub tries: &'static [Drop],
}

/// A dimension type's column: `sections` 16-block sections from `min_y`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DimensionType {
    pub name: &'static str,
    pub id: u32,
    pub min_y: i32,
    pub height: u32,
    pub sections: u32,
    pub logical_height: u32,
    pub coordinate_scale: f64,
}

/// The tool kind a `mineable/*` tag names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ToolKind {
    Pickaxe,
    Axe,
    Shovel,
    Hoe,
}

/// The lowest tool tier a `needs_*_tool` tag asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Stone,
    Iron,
    Diamond,
}

/// Which tool kind mines a block faster, and the lowest tier that makes it drop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Harvest {
    pub block: BlockKind,
    pub tool: Option<ToolKind>,
    pub needs: Option<Tier>,
}

/// The index of `name` in a registry name table, which is what the wire sends.
pub fn index_of(table: &[&str], name: &str) -> Option<u32> {
    table
        .iter()
        .position(|n| *n == name)
        .and_then(|i| u32::try_from(i).ok())
}
