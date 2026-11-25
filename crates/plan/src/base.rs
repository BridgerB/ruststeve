//! Issue 18d, the forest base (docs/problems/pearls-base.md): on reaching a warped forest, before
//! the first provoke, the bot makes the kit the hunt needs from what the forest and its pack hold.
//! The frontier bot arrives with no table and no wood, an iron sword of 161 uses (about 23 kills
//! at 7 hits), two stone pickaxes (144 uses), a furnace, 7 raw iron, one coal, 3 sticks and 126
//! cobblestone; the hunt wants about 30 kills of weapons, pickaxe uses for the burrow over the
//! reserve, and a shield for issue 19's trip back.
//!
//! The steps, in order, each skipped when what it makes is held, each ending done or given up
//! within `step_ticks`, each with a ledger note ("base: <step> begun / done / skipped / gave up"):
//! 1. **wood**: warped stems (the `#logs` tag) seen with an exposed face, dug by hand;
//! 2. **table**: sticks and a crafting table placed where the bot stands (`base-table`);
//! 3. **smelt**: the raw iron in the held furnace, with coal only (warped planks do not burn, and
//!    sticks and blaze rods are never fuel here);
//! 4. **iron sword**, 5. **stone swords** (`base-iron-sword`, `base-stone-sword`) until the swords
//!    held are worth `kills` kills;
//! 6. **pickaxe**: an iron one when the ingots spare it, else stone ones, until `pickaxe_uses`;
//! 7. **shield** (`base-shield`): six planks and an ingot.
//!
//! Every craft is the goal table's (`config/goals.toml`, the `base-*` rows and the generated
//! recipes) through the needs planner; this module only orders the steps, bounds them and turns
//! the planner's act into a task where the bot stands (no mine, no trip). The base never runs
//! with an angry enderman present, and every look leaves through `Plan::keep_look`.

use data::ItemKind;
use world::{Act, World};

use crate::make::{self, Choice};
use crate::path::{self, Cell};
use crate::walk::Walk;
use crate::{Plan, Task, craft, endermen};

/// Items never dropped by the base to free a slot, whatever the config says: what the run is for.
pub const NEVER_DROP: &[ItemKind] = &[
    ItemKind::EnderPearl,
    ItemKind::BlazeRod,
    ItemKind::BlazePowder,
    ItemKind::EnderEye,
];

/// The base's numbers: `[base]` in `config/goals.toml`, each with its source there. The defaults
/// are the config's values (a test holds them equal), for `base_done`, which has no table.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Base {
    /// Logs held before the table step.
    pub logs: i32,
    /// Kills' worth of swords held before the hunt.
    pub kills: i32,
    /// Hits a kill takes with each sword, by item name; a sword not listed counts for nothing.
    pub hits_per_kill: Vec<(String, i32)>,
    /// A sword with fewer uses left counts for nothing (the hunt switches swords under it).
    pub sword_floor: i32,
    /// Pickaxe uses (summed over the held pickaxes) held before the hunt.
    pub pickaxe_uses: i32,
    /// Free main slots kept; under it a stack of `drop` goes.
    pub free_slots: usize,
    pub drop: Vec<String>,
    /// A step ends done or given up within this many ticks, and after this many failed tasks.
    pub step_ticks: u64,
    pub step_fails: u32,
    /// Stems are dug within this flat distance of the feet, and this far above or below them.
    pub stem_radius: i32,
    pub stem_rise: i32,
    /// Ticks between two notes of the same wait.
    pub note_every: u64,
    /// Wooden pickaxe uses held after the `wood picks` step.
    pub wood_pick_uses: i32,
    /// Off a fungus cap: seen warped nylium with room over it walked to between these flat
    /// distances, and the moves down (walks and digs) tried at most this often.
    pub floor_near: i32,
    pub floor_far: i32,
    pub down_tries: u32,
    /// A wait longer than `wait_ticks` in a row pauses the base for `pause_ticks` (or until
    /// `Plan::base_resume`, after a kill), so the hunt runs with what is held; a step that waits
    /// that long again after `pause_max` pauses gives up.
    pub wait_ticks: u64,
    pub pause_ticks: u64,
    pub pause_max: u32,
}

impl Default for Base {
    fn default() -> Self {
        Self {
            logs: 10,
            kills: 30,
            hits_per_kill: vec![("iron_sword".to_owned(), 7), ("stone_sword".to_owned(), 8)],
            sword_floor: 10,
            pickaxe_uses: 100,
            free_slots: 4,
            drop: ["granite", "tuff", "dirt", "diorite", "andesite", "calcite"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect(),
            step_ticks: 2400,
            step_fails: 4,
            stem_radius: 48,
            stem_rise: 12,
            note_every: 600,
            wood_pick_uses: 150,
            floor_near: 8,
            floor_far: 24,
            down_tries: 24,
            wait_ticks: 200,
            pause_ticks: 6000,
            pause_max: 1,
        }
    }
}

/// The steps, in the order they run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Step {
    Wood,
    Table,
    Smelt,
    IronSword,
    StoneSword,
    Pickaxe,
    Shield,
    /// Wooden pickaxes for the hunt's filler (issue 18f: each burrow is about 39 blocks, and the
    /// model's 12-pearl loop ran out near the sixth): reclaiming shells and netherrack without
    /// spending the stone reserve. After the table, by the run order of `STEPS`.
    WoodPicks,
}

pub const STEPS: [Step; 8] = [
    Step::Wood,
    Step::Table,
    Step::WoodPicks,
    Step::Smelt,
    Step::IronSword,
    Step::StoneSword,
    Step::Pickaxe,
    Step::Shield,
];

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Wood => "wood",
            Step::Table => "table",
            Step::Smelt => "smelt",
            Step::IronSword => "iron sword",
            Step::StoneSword => "stone swords",
            Step::Pickaxe => "pickaxe",
            Step::Shield => "shield",
            Step::WoodPicks => "wood picks",
        }
    }
}

/// The base's state, one field of `Plan` (`Plan::base`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct BaseState {
    /// The step in hand, when it began, the tasks begun and failed for it.
    #[serde(default)]
    pub step: Option<Step>,
    #[serde(default)]
    pub since: u64,
    #[serde(default)]
    pub tasks: u32,
    #[serde(default)]
    pub fails: u32,
    /// The planner's failure count when the last task was begun: a rise is this step's failure.
    #[serde(default)]
    pub failures_seen: u64,
    /// The run in hand of a repeated step: the item and how many make it done.
    #[serde(default)]
    pub target: Option<(ItemKind, i32)>,
    /// Steps over, with whether each ended done (or skipped) rather than given up.
    #[serde(default)]
    pub ended: Vec<(Step, bool)>,
    #[serde(default)]
    pub over: bool,
    /// When a wait was last noted.
    #[serde(default)]
    pub noted_at: Option<u64>,
    /// Moves begun to get down off a fungus cap.
    #[serde(default)]
    pub down_tries: u32,
    /// When the wait in hand began, until when the base is paused, and the pauses of the step in
    /// hand.
    #[serde(default)]
    pub wait_since: Option<u64>,
    #[serde(default)]
    pub paused_until: Option<u64>,
    #[serde(default)]
    pub pauses: u32,
}

/// What a wait comes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waited {
    /// Wait on: look again shortly.
    On,
    /// Waited `wait_ticks`: the base pauses and the hunt runs.
    Paused,
    /// Waited that long again after `pause_max` pauses: the step gives up.
    Over,
}

/// The steps that need what `gone` was to supply, when it gave up with none of it held: with no
/// wood (no logs, no planks) every craft (the table, sticks, swords, pickaxes, shield) and the smelt
/// (its ingots make nothing without a table); with no table every 3x3 craft and the smelt.
pub fn starved_by(gone: Step, w: &World, table_near: bool) -> Vec<Step> {
    let wood = logs_held(w) + planks_held(w) > 0;
    let table = table_near || craft::count(w, ItemKind::CraftingTable) > 0;
    match gone {
        Step::Wood if !wood && !table => vec![
            Step::Table,
            Step::WoodPicks,
            Step::Smelt,
            Step::IronSword,
            Step::StoneSword,
            Step::Pickaxe,
            Step::Shield,
        ],
        Step::Table if !table => vec![
            Step::WoodPicks,
            Step::Smelt,
            Step::IronSword,
            Step::StoneSword,
            Step::Pickaxe,
            Step::Shield,
        ],
        Step::Wood
        | Step::Table
        | Step::Smelt
        | Step::IronSword
        | Step::StoneSword
        | Step::Pickaxe
        | Step::Shield
        | Step::WoodPicks => Vec::new(),
    }
}

/// What a step does next.
#[derive(Clone, Debug, PartialEq)]
pub enum Next {
    Task(Box<Task>),
    /// Send this act (a hotbar change, a drop) and look again next tick.
    Act(Act),
    Done(String),
    GaveUp(String),
    Wait(String),
}

fn item(name: &str) -> Option<ItemKind> {
    ItemKind::from_name(name)
}

/// Hits a kill takes with this sword, when the config counts it.
pub fn hits_for(base: &Base, k: ItemKind) -> Option<i32> {
    base.hits_per_kill
        .iter()
        .find(|(n, _)| item(n) == Some(k))
        .map(|(_, h)| *h)
        .filter(|h| *h > 0)
}

/// The kills the held swords are worth: each one's uses left over its hits a kill, a sword under
/// `sword_floor` uses counting for nothing. The main inventory, the hotbar and the off hand.
pub fn weapon_kills(w: &World, base: &Base) -> i32 {
    w.inventory
        .slots
        .iter()
        .filter(|(s, _)| (9..=45).contains(*s))
        .filter_map(|(s, (k, _))| {
            let hits = hits_for(base, *k)?;
            let uses = w.inventory.uses_left(*s)?;
            (uses >= base.sword_floor).then_some(uses / hits)
        })
        .sum()
}

/// The hunt's kit is held: swords worth `kills`, `pickaxe_uses` of pickaxes, a shield.
pub fn kit_held(w: &World, base: &Base) -> bool {
    weapon_kills(w, base) >= base.kills
        && reserve_pick_uses(w) >= base.pickaxe_uses
        && craft::count(w, ItemKind::Shield) >= 1
}

/// The forest base's kit is held (by the config's numbers, `Base::default`). The planner's own
/// end is `Plan::base_task` returning `None`: a step given up ends the base with the kit short.
pub fn base_done(w: &World) -> bool {
    kit_held(w, &Base::default())
}

/// Logs held (the `#logs` tag).
pub fn logs_held(w: &World) -> i32 {
    data::generated::tags::item_tag("logs")
        .unwrap_or(&[])
        .iter()
        .map(|k| craft::count(w, *k))
        .sum()
}

/// Planks held (the `#planks` tag).
pub fn planks_held(w: &World) -> i32 {
    data::generated::tags::item_tag("planks")
        .unwrap_or(&[])
        .iter()
        .map(|k| craft::count(w, *k))
        .sum()
}

/// Coal that burns in the held furnace: coal and charcoal (warped planks have no burn time, and
/// sticks and rods are never this step's fuel).
pub fn coal_held(w: &World) -> i32 {
    craft::count(w, ItemKind::Coal) + craft::count(w, ItemKind::Charcoal)
}

/// The raw iron one coal smelts: its burn time over the recipe's cook time (1,600 / 200 = 8).
pub fn smelts_per_coal(recipe: &data::types::Smelt) -> i32 {
    let burn = data::generated::fuel::fuel_ticks(ItemKind::Coal).unwrap_or(0);
    i32::try_from(burn / recipe.ticks.max(1)).unwrap_or(0)
}

/// The furnace recipe for raw iron.
pub fn iron_recipe() -> Option<&'static data::types::Smelt> {
    data::generated::recipes::SMELTING
        .iter()
        .find(|r| r.result == ItemKind::IronIngot && r.input.contains(&ItemKind::RawIron))
}

/// Free main slots (window 0's 9 to 44).
pub fn free_slots(w: &World) -> usize {
    (9i16..=44)
        .filter(|s| !w.inventory.slots.contains_key(s))
        .count()
}

/// With fewer than `free_slots` main slots free, the first main stack of a `drop` kind (in the
/// list's order), never one of `NEVER_DROP`.
pub fn drop_slot(w: &World, base: &Base) -> Option<(i16, ItemKind)> {
    if free_slots(w) >= base.free_slots {
        return None;
    }
    base.drop
        .iter()
        .filter_map(|n| item(n))
        .filter(|k| !NEVER_DROP.contains(k))
        .find_map(|k| {
            w.inventory
                .slots
                .iter()
                .find(|(s, (h, _))| (9..=44).contains(*s) && *h == k)
                .map(|(s, _)| (*s, k))
        })
}

/// Whether an item wears when it digs (a sword loses 2 a block, a pickaxe 1): not for stems.
fn wears(k: ItemKind) -> bool {
    data::generated::item_props::max_damage(k) > 0
}

/// The hotbar change that leaves no wearing tool in hand before a stem is dug by hand (a sword
/// digging a stem spends two uses of the hunt's weapon), or `None` when the hand is fine.
pub fn bare_hand(w: &World) -> Option<Act> {
    let held = crate::hold::held_item(w);
    if !held.is_some_and(wears) {
        return None;
    }
    (0u8..9)
        .find(|i| {
            w.inventory
                .slots
                .get(&(36 + i16::from(*i)))
                .is_none_or(|(k, _)| !wears(*k))
        })
        .map(|slot| Act::Hold { slot })
}

/// Stems (logs) seen with an exposed face (memory's sightings), each column's lowest seen cell
/// only, the cell a stem is dug from (race 45537373222: the cells round the feet of bots on a
/// fungus cap were all in the cap), within `stem_radius` flat of the feet and `stem_rise` of
/// their height, nearest first; `skip` leaves out refused cells (a column whose lowest is refused
/// is left out whole: the cells over it are no nearer a way).
pub fn stem_targets(w: &World, base: &Base, skip: impl Fn(Cell) -> bool) -> Vec<Cell> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let Some(logs) = data::generated::tags::block_tag("logs") else {
        return Vec::new();
    };
    let feet = path::feet_cell(body.pos);
    let (cx, cz) = (feet.0 >> 4, feet.2 >> 4);
    let r = (base.stem_radius >> 4) + 1;
    let flat = |c: Cell| {
        ((f64::from(c.0) + 0.5 - body.pos[0]).powi(2)
            + (f64::from(c.2) + 0.5 - body.pos[2]).powi(2))
        .sqrt()
    };
    let mut lowest: std::collections::BTreeMap<(i32, i32), Cell> =
        std::collections::BTreeMap::new();
    for dx in -r..=r {
        for dz in -r..=r {
            for s in w.memory.in_column(&dim.name, cx + dx, cz + dz) {
                let c = s.pos;
                let is_log = data::block_of_state(s.state).is_some_and(|k| logs.contains(&k));
                if !is_log || flat(c) > f64::from(base.stem_radius) {
                    continue;
                }
                let e = lowest.entry((c.0, c.2)).or_insert(c);
                if c.1 < e.1 {
                    *e = c;
                }
            }
        }
    }
    let mut v: Vec<(f64, Cell)> = lowest
        .into_values()
        .filter(|c| (c.1 - feet.1).abs() <= base.stem_rise && !skip(*c) && !crate::crowded(w, *c))
        .map(|c| (flat(c), c))
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, c)| c).collect()
}

/// What a huge fungus's cap and stem are made of: a bot standing on one is up in the cap, not on
/// the forest floor (race 45537373222 lanes 0 and 2 stood on warped wart at y 51 to 52 for 2,400
/// ticks). All are soft by hand (wart 1.0, shroomlight 1.0, stems 2.0).
pub const CAP_BLOCKS: &[data::BlockKind] = &[
    data::BlockKind::WarpedWartBlock,
    data::BlockKind::NetherWartBlock,
    data::BlockKind::Shroomlight,
    data::BlockKind::WarpedStem,
    data::BlockKind::WarpedHyphae,
    data::BlockKind::CrimsonStem,
    data::BlockKind::CrimsonHyphae,
];

/// The bot stands on a fungus cap (or a stem's top).
pub fn on_cap(w: &World) -> bool {
    w.block_under_player()
        .is_some_and(|k| CAP_BLOCKS.contains(&k))
}

/// The forest floor to walk down to: warped nylium seen (memory's sign index) between
/// `floor_near` and `floor_far` flat of the feet, under them, with room for a body over it; the
/// cell over it, nearest first.
pub fn floor_targets(w: &World, base: &Base, skip: impl Fn(Cell) -> bool) -> Vec<Cell> {
    let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
        return Vec::new();
    };
    let feet = path::feet_cell(body.pos);
    let r = (base.floor_far >> 4) + 1;
    let mut cells = path::Cells::new(w);
    let mut v: Vec<(i32, Cell)> = Vec::new();
    for dx in -r..=r {
        for dz in -r..=r {
            let seen = w
                .memory
                .indexed_in_column(
                    &dim.name,
                    (feet.0 >> 4) + dx,
                    (feet.2 >> 4) + dz,
                    data::BlockKind::WarpedNylium,
                )
                .unwrap_or_default();
            for n in seen {
                let c = (n.0, n.1 + 1, n.2);
                let d2 = (c.0 - feet.0).pow(2) + (c.2 - feet.2).pow(2);
                if d2 < base.floor_near.pow(2)
                    || d2 > base.floor_far.pow(2)
                    || c.1 >= feet.1
                    || skip(c)
                    || !cells.standable(c)
                {
                    continue;
                }
                v.push((d2, c));
            }
        }
    }
    v.sort();
    v.into_iter().map(|(_, c)| c).collect()
}

/// The cap block under the feet to dig by hand, when the body would come to rest within
/// `path::MAX_FALL` below it on a floor, with no lava on the way (a fall of more hurts, and the
/// bot cannot heal).
pub fn dig_down(w: &World) -> Option<Cell> {
    let body = w.body?;
    let (x, y, z) = path::feet_cell(body.pos);
    let under = (x, y - 1, z);
    let kind = w.block_at(world::BlockPos::new(under.0, under.1, under.2))?;
    if !CAP_BLOCKS.contains(&kind) {
        return None;
    }
    let mut cells = path::Cells::new(w);
    // The feet go to `under` once it is dug, then fall while the cell below is clear.
    for fall in 0..=path::MAX_FALL {
        let feet = (x, under.1 - fall, z);
        let below = (x, feet.1 - 1, z);
        if fall > 0 && cells.at(feet) != path::Class::Clear {
            return None;
        }
        if cells.at(below) == path::Class::Lava {
            return None;
        }
        if cells.floor(below) {
            return Some(under);
        }
    }
    None
}

/// Why a step is not wanted (its things held, or nothing to make it from), or `None` to run it.
pub fn skip_why(step: Step, w: &World, base: &Base, table_near: bool) -> Option<String> {
    let kills = weapon_kills(w, base);
    let ingots = craft::count(w, ItemKind::IronIngot);
    let shield = craft::count(w, ItemKind::Shield);
    match step {
        Step::Wood => {
            let logs = logs_held(w);
            let planks = planks_held(w);
            if kit_held(w, base) {
                Some("the kit is held".to_owned())
            } else if logs + planks / 4 >= base.logs {
                Some(format!("{logs} logs and {planks} planks held"))
            } else {
                None
            }
        }
        Step::Table => {
            let sticks = craft::count(w, ItemKind::Stick);
            (table_near && sticks >= 4).then(|| format!("a table near and {sticks} sticks held"))
        }
        Step::Smelt => {
            let raw = craft::count(w, ItemKind::RawIron);
            if raw == 0 {
                Some("no raw iron held".to_owned())
            } else if coal_held(w) == 0 {
                Some(format!(
                    "no coal: {raw} raw iron kept (warped planks do not burn; sticks and rods are never fuel)"
                ))
            } else {
                None
            }
        }
        Step::IronSword | Step::StoneSword if kills >= base.kills => Some(format!(
            "swords worth {kills} kills held, of {}",
            base.kills
        )),
        Step::IronSword => (ingots < 2).then(|| format!("{ingots} iron ingots, a sword is 2")),
        Step::StoneSword => None,
        Step::Pickaxe => {
            let uses = reserve_pick_uses(w);
            (uses >= base.pickaxe_uses)
                .then(|| format!("pickaxe uses {uses} held, of {}", base.pickaxe_uses))
        }
        Step::Shield if shield > 0 => Some(format!("{shield} shield held")),
        Step::Shield => (ingots < 1).then(|| "no iron ingot".to_owned()),
        Step::WoodPicks => {
            let uses = wood_pick_uses(w);
            (uses >= base.wood_pick_uses).then(|| {
                format!(
                    "wooden pickaxe uses {uses} held, of {}",
                    base.wood_pick_uses
                )
            })
        }
    }
}

/// The pickaxe uses the burrows are kept from: every held pickaxe but the wooden ones, which are
/// the hunt's for filler (`spare_pick`).
pub fn reserve_pick_uses(w: &World) -> i32 {
    endermen::pickaxe_uses_left(w) - wood_pick_uses(w)
}

/// The wooden pickaxes' uses left, summed (main inventory, hotbar and off hand).
pub fn wood_pick_uses(w: &World) -> i32 {
    w.inventory
        .slots
        .iter()
        .filter(|(s, (k, _))| (9..=45).contains(*s) && *k == ItemKind::WoodenPickaxe)
        .filter_map(|(s, _)| w.inventory.uses_left(*s))
        .sum()
}

/// The hunt's pickaxe for filler (P3: reclaiming old shells, restocking netherrack), never the
/// stone reserve: the slot of a held wooden pickaxe with uses left, the most worn first (one is
/// used up before the next is begun), or `None`.
pub fn spare_pick(w: &World) -> Option<i16> {
    w.inventory
        .slots
        .iter()
        .filter(|(s, (k, _))| (9..=45).contains(*s) && *k == ItemKind::WoodenPickaxe)
        .filter_map(|(s, _)| w.inventory.uses_left(*s).map(|u| (u, *s)))
        .filter(|(u, _)| *u > 0)
        .min()
        .map(|(_, s)| s)
}

/// The goal row a repeated step makes one run of, by what is held (`config/goals.toml`), or `None`
/// when the step is not a row's.
pub fn step_row(step: Step, w: &World) -> Option<&'static str> {
    match step {
        Step::Wood | Step::Smelt => None,
        Step::Table => Some("base-table"),
        Step::IronSword => Some("base-iron-sword"),
        Step::StoneSword => Some("base-stone-sword"),
        Step::Pickaxe => {
            // The shield's ingot is kept: three more for an iron pickaxe.
            let keep = i32::from(craft::count(w, ItemKind::Shield) == 0);
            if craft::count(w, ItemKind::IronIngot) >= 3 + keep {
                Some("base-iron-pickaxe")
            } else {
                Some("base-stone-pickaxe")
            }
        }
        Step::Shield => Some("base-shield"),
        Step::WoodPicks => Some("base-wood-pickaxe"),
    }
}

/// Whether a repeated step wants another run after one is done.
fn again(step: Step, w: &World, base: &Base) -> bool {
    match step {
        Step::StoneSword => weapon_kills(w, base) < base.kills,
        Step::Pickaxe => reserve_pick_uses(w) < base.pickaxe_uses,
        Step::WoodPicks => wood_pick_uses(w) < base.wood_pick_uses,
        Step::Wood | Step::Table | Step::Smelt | Step::IronSword | Step::Shield => false,
    }
}

impl Plan {
    /// The development aim `base` (`Aim::Base`): the base where the bot stands, then met.
    pub(crate) fn base_aim(&mut self, w: &World, tick: u64) -> Task {
        self.equip(w, tick);
        match self.base_task(w, tick) {
            Some(t) => t,
            // Paused, not over: the development aim waits it out (the race hunts meanwhile).
            None if !self.base.over => Task::Idle,
            None => {
                self.met = true;
                self.note(
                    tick,
                    "base met".to_owned(),
                    "the forest base is over".to_owned(),
                );
                Task::Idle
            }
        }
    }

    /// The forest base's next task, or `None` once every step is over (done, skipped or given
    /// up). `Task::Idle` while it waits (an angry enderman, an act in flight, nothing seen).
    pub fn base_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.base.over {
            return None;
        }
        let table = self.table.clone()?;
        let base = table.base.clone();
        match self.base.paused_until {
            Some(t) if tick < t => return None,
            Some(_) => {
                self.base.paused_until = None;
                self.base.wait_since = None;
                // The step in hand gets a fresh clock: the pause was the hunt's time.
                self.base.since = tick;
                self.note(
                    tick,
                    "base resumed".to_owned(),
                    format!(
                        "the step in hand: {}",
                        self.base.step.map_or("none", Step::name)
                    ),
                );
            }
            None => {}
        }
        if let Some(id) = endermen::angry(w) {
            return match self.base_wait(tick, format!("enderman {id} is angry"), false) {
                Waited::On => Some(Task::Idle),
                Waited::Paused | Waited::Over => None,
            };
        }
        // A batch left cooking is collected first (a smelt cut while loaded).
        if let Some((furnace, result, count)) = self.batch {
            self.note(
                tick,
                format!("base: collect {count} {result:?} at {furnace:?}"),
                "a batch was left cooking".to_owned(),
            );
            return Some(Task::Smelt(Box::new(crate::smelt::Smelt::collect(
                w, furnace, result, count, tick,
            ))));
        }
        if w.click.is_none()
            && w.window.is_none()
            && w.inventory.carried.is_none()
            && let Some((slot, kind)) = drop_slot(w, &base)
        {
            self.acts.push(Act::Click {
                slot,
                button: 1,
                mode: 4,
            });
            self.retry_at = tick + crate::RETRY_TICKS;
            self.note(
                tick,
                format!("base: drop {kind:?} from slot {slot}"),
                format!(
                    "{} main slots free, {} kept free",
                    free_slots(w),
                    base.free_slots
                ),
            );
            return Some(Task::Idle);
        }
        if on_cap(w)
            && self.base.down_tries < base.down_tries
            && let Some(t) = self.down_step(w, &base, tick)
        {
            self.base.wait_since = None;
            return Some(t);
        }
        loop {
            let step = match self.base.step {
                Some(s) => s,
                None => {
                    let Some(s) = STEPS
                        .iter()
                        .copied()
                        .find(|s| !self.base.ended.iter().any(|(e, _)| e == s))
                    else {
                        self.base_over(w, &base, tick);
                        return None;
                    };
                    let table_near = self
                        .near_block(w, data::BlockKind::CraftingTable, tick)
                        .is_some();
                    if let Some(why) = skip_why(s, w, &base, table_near) {
                        self.base.ended.push((s, true));
                        self.note(tick, format!("base: {} skipped", s.name()), why);
                        continue;
                    }
                    self.base.step = Some(s);
                    self.base.since = tick;
                    self.base.tasks = 0;
                    self.base.fails = 0;
                    self.base.target = None;
                    self.base.pauses = 0;
                    self.base.wait_since = None;
                    self.base.failures_seen = self.failures.values().sum();
                    self.note(
                        tick,
                        format!("base: {} begun", s.name()),
                        format!(
                            "swords worth {} kills, pickaxe uses {}, {} logs, {} planks, {} ingots, {} raw iron, {} coal, {} main slots free",
                            weapon_kills(w, &base),
                            reserve_pick_uses(w),
                            logs_held(w),
                            planks_held(w),
                            craft::count(w, ItemKind::IronIngot),
                            craft::count(w, ItemKind::RawIron),
                            coal_held(w),
                            free_slots(w)
                        ),
                    );
                    s
                }
            };
            // The progress invariant: a step ends within its clock and its failures.
            let failures: u64 = self.failures.values().sum();
            if failures > self.base.failures_seen {
                self.base.fails += u32::try_from(failures - self.base.failures_seen).unwrap_or(1);
                self.base.failures_seen = failures;
            }
            let spent = tick.saturating_sub(self.base.since);
            let limit = if spent > base.step_ticks {
                Some(format!("{spent} ticks, of {}", base.step_ticks))
            } else if self.base.fails >= base.step_fails {
                Some(format!("{} tasks failed", self.base.fails))
            } else {
                None
            };
            // The step is asked first: one whose things are held by now is done, not given up.
            let next = match (self.base_step(w, &table, &base, step, tick), limit) {
                (Next::Done(why), _) => Next::Done(why),
                (_, Some(why)) => Next::GaveUp(why),
                (next, None) => next,
            };
            match next {
                Next::Task(t) => {
                    self.base.wait_since = None;
                    self.base.tasks += 1;
                    self.base.failures_seen = self.failures.values().sum();
                    return Some(*t);
                }
                Next::Act(a) => {
                    self.acts.push(a);
                    self.retry_at = tick + crate::RETRY_TICKS;
                    return Some(Task::Idle);
                }
                Next::Wait(why) => {
                    match self.base_wait(tick, format!("{}: {why}", step.name()), true) {
                        Waited::On => return Some(Task::Idle),
                        Waited::Paused => return None,
                        Waited::Over => self.base_end(
                            w,
                            step,
                            false,
                            format!("waited again after {} pauses: {why}", self.base.pauses),
                            tick,
                        ),
                    }
                }
                Next::Done(why) => self.base_end(w, step, true, why, tick),
                Next::GaveUp(why) => self.base_end(w, step, false, why, tick),
            }
        }
    }

    /// A step over: done (or skipped) or given up, with its note.
    fn base_end(&mut self, w: &World, step: Step, done: bool, why: String, tick: u64) {
        self.base.ended.push((step, done));
        self.base.step = None;
        self.base.wait_since = None;
        self.note(
            tick,
            format!(
                "base: {} {}",
                step.name(),
                if done { "done" } else { "gave up" }
            ),
            format!(
                "{why}; {} tasks in {} ticks",
                self.base.tasks,
                tick.saturating_sub(self.base.since)
            ),
        );
        if done {
            return;
        }
        // A step whose inputs this one was to supply is skipped at once, never waited on (race
        // 45542558792: 8 bots waited 2,400 ticks a step on the missing wood, and no hunt began).
        let table_near = self
            .near_block(w, data::BlockKind::CraftingTable, tick)
            .is_some();
        let starved: Vec<Step> = starved_by(step, w, table_near)
            .into_iter()
            .filter(|s| !self.base.ended.iter().any(|(e, _)| e == s))
            .collect();
        if starved.is_empty() {
            return;
        }
        for s in &starved {
            self.base.ended.push((*s, false));
        }
        self.note(
            tick,
            format!(
                "base: no {}: the hunt goes on with what is held",
                if step == Step::Wood { "wood" } else { "table" }
            ),
            format!(
                "skipped: {}; {} logs, {} planks, {} sticks held",
                starved
                    .iter()
                    .map(|s| s.name())
                    .collect::<Vec<_>>()
                    .join(", "),
                logs_held(w),
                planks_held(w),
                craft::count(w, ItemKind::Stick)
            ),
        );
    }

    /// Lets a paused base run again at the next choice (the hunt calls it after a kill).
    pub fn base_resume(&mut self) {
        if self.base.paused_until.is_some() {
            self.base.paused_until = Some(0);
        }
    }

    /// A wait, noted once every `note_every` ticks; the choice is made again shortly. Past
    /// `wait_ticks` in a row the base pauses for `pause_ticks` (the hunt runs); a step's wait
    /// that long after `pause_max` pauses is over (`for_step`).
    fn base_wait(&mut self, tick: u64, why: String, for_step: bool) -> Waited {
        let base = self
            .table
            .as_ref()
            .map_or_else(Base::default, |t| t.base.clone());
        self.retry_at = tick + crate::RETRY_TICKS;
        let since = *self.base.wait_since.get_or_insert(tick);
        if tick.saturating_sub(since) >= base.wait_ticks {
            self.base.wait_since = None;
            if for_step && self.base.pauses >= base.pause_max {
                return Waited::Over;
            }
            if for_step {
                self.base.pauses += 1;
            }
            self.base.paused_until = Some(tick + base.pause_ticks);
            self.note(
                tick,
                "base paused: the hunt goes on with what is held".to_owned(),
                format!(
                    "waited {} ticks ({why}); again at tick {} or after a kill",
                    tick.saturating_sub(since),
                    tick + base.pause_ticks
                ),
            );
            return Waited::Paused;
        }
        if self
            .base
            .noted_at
            .is_none_or(|t| tick >= t + base.note_every)
        {
            self.base.noted_at = Some(tick);
            self.note(tick, "base waits".to_owned(), why);
        }
        Waited::On
    }

    fn base_over(&mut self, w: &World, base: &Base, tick: u64) {
        self.base.over = true;
        let gave_up: Vec<&str> = self
            .base
            .ended
            .iter()
            .filter(|(_, done)| !done)
            .map(|(s, _)| s.name())
            .collect();
        self.note(
            tick,
            if kit_held(w, base) {
                "base over: the kit is held".to_owned()
            } else {
                "base over: the kit is short".to_owned()
            },
            format!(
                "swords worth {} kills of {}, pickaxe uses {} of {}, {} shield; given up: {}",
                weapon_kills(w, base),
                base.kills,
                reserve_pick_uses(w),
                base.pickaxe_uses,
                craft::count(w, ItemKind::Shield),
                if gave_up.is_empty() {
                    "none".to_owned()
                } else {
                    gave_up.join(", ")
                }
            ),
        );
    }

    /// One step's next move.
    fn base_step(
        &mut self,
        w: &World,
        table: &make::Table,
        base: &Base,
        step: Step,
        tick: u64,
    ) -> Next {
        match step {
            Step::Wood => {
                let logs = logs_held(w);
                if logs >= base.logs {
                    return Next::Done(format!("{logs} logs held"));
                }
                self.stem_step(w, base, tick)
            }
            Step::Smelt => self.smelt_step(w, tick),
            Step::Table
            | Step::IronSword
            | Step::StoneSword
            | Step::Pickaxe
            | Step::Shield
            | Step::WoodPicks => self.row_step(w, table, base, step, tick),
        }
    }

    /// Down off a fungus cap to the forest floor: a walk to seen nylium with room over it, by
    /// route; with none, the cap block under the feet dug by hand when the drop is safe. Noted
    /// "base: down off the fungus cap". `None` when neither can be done now.
    fn down_step(&mut self, w: &World, base: &Base, tick: u64) -> Option<Task> {
        self.base.down_tries += 1;
        let feet = w.body.map(|b| path::feet_cell(b.pos));
        let targets = floor_targets(w, base, |c| self.refused_now(c, tick));
        for c in targets.into_iter().take(crate::CANDIDATES_PER_TICK) {
            let mut walk = Walk::new(
                c,
                path::Goal::Block { pos: c },
                crate::walk::Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    "base: down off the fungus cap".to_owned(),
                    format!(
                        "walk from {feet:?} on {:?} to the floor at {c:?}; move {} of {}",
                        w.block_under_player(),
                        self.base.down_tries,
                        base.down_tries
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            self.refuse(c, tick);
        }
        let under = dig_down(w)?;
        if let Some(a) = bare_hand(w) {
            self.acts.push(a);
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        }
        self.note(
            tick,
            "base: down off the fungus cap".to_owned(),
            format!(
                "dig {under:?} under the feet {feet:?} by hand: no floor with a way; move {} of {}",
                self.base.down_tries, base.down_tries
            ),
        );
        Some(Task::Dig(crate::dig::Dig::new(w, under, tick)))
    }

    /// The nearest stem with a way to beside it, dug by hand (the dig follows the walk, as every
    /// gather's does). The hand is emptied of any wearing tool first.
    fn stem_step(&mut self, w: &World, base: &Base, tick: u64) -> Next {
        if let Some(a) = bare_hand(w) {
            return Next::Act(a);
        }
        let targets = stem_targets(w, base, |c| self.refused_now(c, tick));
        for log in targets.into_iter().take(crate::CANDIDATES_PER_TICK) {
            let mut walk = Walk::to_dig(log, tick);
            if walk.route_now(w, tick) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("base: walk to stem {log:?}"),
                    format!("{} logs held, {} wanted", logs_held(w), base.logs),
                );
                return Next::Task(Box::new(Task::ToLog(walk)));
            }
            self.expanded += walk.expanded;
            self.refuse(log, tick);
        }
        Next::Wait(format!(
            "no stem seen within {} with a way to it",
            base.stem_radius
        ))
    }

    /// The raw iron into the held furnace with the coal held, as much as the coal cooks.
    fn smelt_step(&mut self, w: &World, tick: u64) -> Next {
        let raw = craft::count(w, ItemKind::RawIron);
        let coal = coal_held(w);
        let ingots = craft::count(w, ItemKind::IronIngot);
        let Some(recipe) = iron_recipe() else {
            return Next::GaveUp("no furnace recipe for raw iron".to_owned());
        };
        if raw == 0 || coal == 0 {
            return Next::Done(format!(
                "{ingots} ingots held, {raw} raw iron left, {coal} coal"
            ));
        }
        let Some(furnace) = self.near_block(w, data::BlockKind::Furnace, tick) else {
            return match crate::place::Place::new(data::BlockKind::Furnace, tick) {
                Some(p) if craft::count(w, ItemKind::Furnace) > 0 => {
                    self.note(
                        tick,
                        "base: place Furnace".to_owned(),
                        format!("{raw} raw iron to smelt with {coal} coal"),
                    );
                    Next::Task(Box::new(Task::Place(p)))
                }
                Some(_) | None => Next::GaveUp("no furnace near or held".to_owned()),
            };
        };
        let count = raw.min(coal * smelts_per_coal(recipe));
        match crate::smelt::Smelt::new(w, furnace, recipe, count, tick) {
            Some(sm) => {
                self.note(
                    tick,
                    format!("base: smelt {count} raw iron at {furnace:?}"),
                    format!("{coal} coal cooks {}", coal * smelts_per_coal(recipe)),
                );
                Next::Task(Box::new(Task::Smelt(Box::new(sm))))
            }
            None => Next::GaveUp("the smelt could not begin".to_owned()),
        }
    }

    /// A step made from a goal row: one run of it (one more of its first provide than was held
    /// when the run began), the needs planner's next act where the bot stands.
    fn row_step(
        &mut self,
        w: &World,
        table: &make::Table,
        base: &Base,
        step: Step,
        tick: u64,
    ) -> Next {
        let Some(id) = step_row(step, w) else {
            return Next::GaveUp("no row".to_owned());
        };
        let Some(goal) = table.until(id) else {
            return Next::GaveUp(format!("no goal row {id}"));
        };
        let mut row = goal.clone();
        // The table's row is as written (four sticks and a table near); the others make one
        // more of their first provide per run, repeated while the step wants it.
        if step != Step::Table {
            let Some((need, n)) = row.provides.first().cloned() else {
                return Next::GaveUp(format!("{id} provides nothing"));
            };
            let Some(k) = need.items.first().copied() else {
                return Next::GaveUp(format!("{id} provides no item"));
            };
            let held = craft::count(w, k);
            let target = match self.base.target {
                Some((t, want)) if t == k && held < want => want,
                Some((t, want)) if t == k => {
                    // A run is done: another if the step still wants it.
                    if !again(step, w, base) {
                        return Next::Done(format!("{held} {k:?} held ({id}); run to {want}"));
                    }
                    held + n
                }
                Some(_) | None => held + n,
            };
            self.base.target = Some((k, target));
            if let Some(p) = row.provides.first_mut() {
                p.1 = target;
            }
        }
        let table_cell = self.near_block(w, data::BlockKind::CraftingTable, tick);
        let furnace_near = self.near_block(w, data::BlockKind::Furnace, tick).is_some();
        let table_near = table_cell.is_some();
        let near = move |b: data::BlockKind| match b {
            data::BlockKind::CraftingTable => table_near,
            data::BlockKind::Furnace => furnace_near,
            _ => false,
        };
        let counts = crate::held(w);
        let gather_now = true;
        let (act, rid) = match make::make_with_wear(
            table,
            &counts,
            &near,
            gather_now,
            &make::Wear::default(),
            &row,
        ) {
            Choice::Done => {
                if step == Step::Table {
                    return Next::Done(format!(
                        "a table near and {} sticks held",
                        craft::count(w, ItemKind::Stick)
                    ));
                }
                // A run's provide held: the next call judges another.
                self.base.target = self.base.target.map(|(k, _)| (k, craft::count(w, k)));
                return self.row_step(w, table, base, step, tick);
            }
            Choice::Stuck => {
                return Next::GaveUp(format!("{id} is not makeable from what is held"));
            }
            Choice::Act(r) => (r.act, r.id.clone()),
        };
        self.base_act(w, base, act, &rid, step, table_cell, tick)
    }

    /// The planner's act as a task where the bot stands: a craft (at the table near when it does
    /// not fit the grid), a placing, or a stem gathered; never a mine, a smelt or a trip.
    #[allow(clippy::too_many_arguments)]
    fn base_act(
        &mut self,
        w: &World,
        base: &Base,
        act: Option<make::Act>,
        id: &str,
        step: Step,
        table_cell: Option<Cell>,
        tick: u64,
    ) -> Next {
        let why = format!("the needs planner's next step toward the {}", step.name());
        match act {
            Some(make::Act::Craft(r)) if !make::fits_grid(r) => match table_cell {
                Some(t) => {
                    self.note(
                        tick,
                        format!("base: craft {} at the table {t:?}", r.id),
                        why,
                    );
                    Next::Task(Box::new(Task::Craft(crate::craft::Craft::at_table(
                        r, t, tick,
                    ))))
                }
                None => Next::GaveUp(format!("{id} wants a table and none is near")),
            },
            Some(make::Act::Craft(r)) => {
                self.note(tick, format!("base: craft {}", r.id), why);
                Next::Task(Box::new(Task::Craft(crate::craft::Craft::new(r, tick))))
            }
            Some(make::Act::Place(b)) => match crate::place::Place::new(b, tick) {
                Some(p) => {
                    self.note(tick, format!("base: place {b:?}"), why);
                    Next::Task(Box::new(Task::Place(p)))
                }
                None => Next::GaveUp(format!("{b:?} has no item to place")),
            },
            Some(make::Act::Gather) => self.stem_step(w, base, tick),
            Some(make::Act::Smelt(_)) => Next::GaveUp(format!(
                "{id}: only the smelt step burns fuel, and only coal"
            )),
            Some(make::Act::Mine { .. } | make::Act::Redig(_) | make::Act::Scoop) | None => {
                Next::GaveUp(format!("{id} would be a trip"))
            }
        }
    }
}
