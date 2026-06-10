//! Shared types for the speedrun bot — game state, inventory/equipment, steps.
//! Port of steve's `types.ts` (the subset needed for the early phases).

#[derive(Debug, Clone, Default)]
pub struct Inventory {
    pub logs: i32,
    pub planks: i32,
    pub sticks: i32,
    pub cobblestone: i32,
    pub coal: i32,
    pub iron_ore: i32,
    pub iron_ingots: i32,
    pub diamonds: i32,
    pub food: i32,
    pub crafting_tables: i32,
    pub buckets: i32,
    pub water_buckets: i32,
    pub flint: i32,
    pub flint_and_steel: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    None,
    Wood,
    Stone,
    Iron,
    Diamond,
}

impl Tier {
    /// Numeric rank for comparisons (none=0 … diamond=4).
    pub fn rank(self) -> i32 {
        match self {
            Tier::None => 0,
            Tier::Wood => 1,
            Tier::Stone => 2,
            Tier::Iron => 3,
            Tier::Diamond => 4,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Equipment {
    pub pickaxe: Option<Tier>,
    pub sword: Option<Tier>,
    pub has_crafting_table: bool,
    pub has_furnace: bool,
}

impl Equipment {
    pub fn pickaxe_tier(&self) -> Tier {
        self.pickaxe.unwrap_or(Tier::None)
    }
}

#[derive(Debug, Clone, Default)]
pub struct WorldState {
    pub dimension: String,
    pub dragon_dead: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GameState {
    pub inventory: Inventory,
    pub equipment: Equipment,
    pub world: WorldState,
    pub health: f64,
    pub food: f64,
    pub position: (f64, f64, f64),
    pub alive: bool,
}

/// Result of executing a step.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub success: bool,
    pub message: String,
}

pub fn success(msg: impl Into<String>) -> StepResult {
    StepResult { success: true, message: msg.into() }
}

pub fn failure(msg: impl Into<String>) -> StepResult {
    StepResult { success: false, message: msg.into() }
}

/// A speedrun step: gated by `can_execute`, satisfied when `is_complete`.
/// `execute` is dispatched by `id` in `steps::execute_step`.
#[derive(Clone)]
pub struct Step {
    pub id: &'static str,
    pub name: &'static str,
    pub priority: i32,
    pub can_execute: fn(&GameState) -> bool,
    pub is_complete: fn(&GameState) -> bool,
}
