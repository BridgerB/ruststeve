//! The dig task (brain/ruststeve/design/digging.md): break one block from where the bot stands
//! and collect what it drops, believing only the server.
//!
//! - **Start**: `player_action` START at the block.
//! - **Finish**: FINISH once the block's break time has passed by the server's clock, with
//!   clojurecraft's margin: `ticks × 1.15 × slowdown + 2` (the 100 ms), the slowdown measured
//!   from `set_time`. An early FINISH is not lost (the server finishes a delayed destroy itself,
//!   brain/game/mc-dig-timing.md), but it is not counted on either.
//! - **Confirm**: the block update to air. Past the deadline without it the dig is retried, and
//!   the third try fails `Refused`.
//! - **Collect**: the drop is picked up when the player's grown box touches it; when it is not
//!   picked up where it fell, the bot walks onto it. Done when the inventory holds more of what
//!   the block drops; a drop that cannot be reached fails `Uncollected`.

use data::{BlockKind, ItemKind};
use world::{Act, BlockPos, DigStep, World};

use crate::path::{self, Cell, Goal};
use crate::walk::{Arrive, TaskStatus, Walk};

/// clojurecraft's `finish-margin`: FINISH this much later than the break time.
pub const FINISH_MARGIN: f64 = 1.15;
/// And this many ticks more (clojurecraft's 100 ms).
pub const FINISH_EXTRA_TICKS: u64 = 2;
/// Ticks after FINISH (beyond the break time) to wait for the block update (`confirm-ticks`).
pub const CONFIRM_TICKS: u64 = 60;
/// Tries before a dig fails `Refused`.
pub const MAX_TRIES: u32 = 3;
/// Ticks to wait where the drop fell before walking onto it (it falls, then the 10-tick delay).
pub const PICKUP_WAIT_TICKS: u64 = 20;
/// Ticks a collect may take in all before it fails.
pub const COLLECT_TICKS: u64 = 300;
/// The collect walks to within this of the drop's cell: a neighbouring column does, since the
/// touch box reaches 1.3 blocks out (the drop of a trunk's lowest log lies under the trunk,
/// where no one can stand).
pub const BESIDE: f64 = 1.0;
/// A drop farther than this from the block is not the block's.
pub const DROP_RANGE: f64 = 3.0;
/// Ticks a vanished drop is given for this bot's own pickup to show before it counts as taken.
pub const TAKEN_TICKS: u64 = 10;
/// The server's clock over ours is measured once this many server ticks have been heard.
const SLOWDOWN_AFTER: u64 = 200;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DigStage {
    Start,
    Finish { at: u64, started: u64 },
    Confirm { deadline: u64 },
    Collect { since: u64, walk: Option<Walk> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DigFailure {
    /// The block stood after three tries.
    Refused,
    /// The drop was not picked up in time.
    Uncollected,
    /// No route to beside the drop.
    DropUnreachable,
    /// The walk to the drop failed.
    DropWalkFailed,
    /// The drop went to someone else: it vanished and the bot neither took it nor holds more.
    Taken,
    /// The block was gone before the dig began (another player broke it).
    Gone,
    /// The body is gone.
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigStatus {
    Running,
    Done,
    Failed(DigFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Dig {
    pub target: Cell,
    pub stage: DigStage,
    pub tries: u32,
    /// What the block drops and how many the bot held before.
    pub drop: Option<ItemKind>,
    pub before: i32,
    pub started: u64,
    /// Where the drop was last seen and where the feet were then: for the case file.
    #[serde(default)]
    pub drop_seen: Option<[f64; 3]>,
    #[serde(default)]
    pub feet: Option<[f64; 3]>,
    /// The drop's entity once seen, and the tick it was last there.
    #[serde(default)]
    pub drop_id: Option<i32>,
    #[serde(default)]
    pub drop_last: u64,
}

fn pos((x, y, z): Cell) -> BlockPos {
    BlockPos::new(x, y, z)
}

/// The block's break time in game ticks by hand (any item that is not a tool digs as a hand;
/// tools come with issue 11): `ceil(1 / progress)`, the progress
/// `speed / hardness / (30 when the hand harvests it, else 100)`, the speed 1 × 0.2 with the
/// eyes in water, ÷ 5 off the ground.
pub fn break_ticks(kind: BlockKind, eye_in_water: bool, on_ground: bool) -> Option<u64> {
    let hardness = f64::from(data::generated::hardness::hardness(kind)?);
    if hardness < 0.0 {
        return None;
    }
    if hardness == 0.0 {
        return Some(0);
    }
    let harvests = !data::generated::hardness::needs_correct_tool(kind).unwrap_or(false);
    let mut speed = 1.0;
    if eye_in_water {
        speed *= 0.2;
    }
    if !on_ground {
        speed /= 5.0;
    }
    let progress = speed / hardness / if harvests { 30.0 } else { 100.0 };
    Some((1.0 / progress).ceil() as u64)
}

/// How much slower the server's ticks run than ours: our ticks over the server's, measured
/// once enough of the server's clock has been heard; 1 before, never under 1.
pub fn slowdown(w: &World) -> f64 {
    let server = w.session.server_ticks;
    if server < SLOWDOWN_AFTER {
        return 1.0;
    }
    (w.ticks as f64 / server as f64).clamp(1.0, 8.0)
}

fn held_of(w: &World, item: ItemKind) -> i32 {
    crate::craft::count(w, item)
}

impl Dig {
    pub fn new(w: &World, target: Cell, tick: u64) -> Self {
        let drop = w
            .block_at(pos(target))
            .and_then(|k| data::generated::loot::drops_of(k).first())
            .filter(|d| d.chance >= 1.0)
            .map(|d| d.item);
        Self {
            target,
            stage: DigStage::Start,
            tries: 0,
            drop,
            before: drop.map_or(0, |d| held_of(w, d)),
            started: tick,
            drop_seen: None,
            feet: None,
            drop_id: None,
            drop_last: tick,
        }
    }

    fn gone(&self, w: &World) -> bool {
        w.block_at(pos(self.target))
            .is_none_or(|k| matches!(k, BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir))
    }

    /// The nearest item entity of the drop's kind within `DROP_RANGE` of the block.
    fn drop_entity(&self, w: &World) -> Option<(i32, [f64; 3])> {
        let want = self.drop?;
        let (x, y, z) = self.target;
        let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
        w.entities
            .iter()
            .filter(|(_, e)| e.item().is_some_and(|(k, _)| k == want))
            .map(|(id, e)| (*id, e.pos))
            .filter(|(_, p)| {
                (0..3).map(|i| (p[i] - c[i]).powi(2)).sum::<f64>().sqrt() <= DROP_RANGE
            })
            .min_by(|(_, a), (_, b)| {
                let d = |p: &[f64; 3]| (0..3).map(|i| (p[i] - c[i]).powi(2)).sum::<f64>();
                d(a).total_cmp(&d(b))
            })
    }

    /// One tick: the walk's controls while collecting, the act to send, how the dig stands.
    pub fn step(&mut self, w: &World, tick: u64) -> (physics::Controls, Option<Act>, DigStatus) {
        let still = physics::Controls::default();
        let Some(body) = w.body else {
            return (still, None, DigStatus::Failed(DigFailure::Lost));
        };
        let gone = self.gone(w);
        let seen = self.drop_entity(w);
        let drop_at = seen.map(|(_, p)| p);
        if drop_at.is_some() {
            self.drop_seen = drop_at;
        }
        let in_collect = matches!(self.stage, DigStage::Collect { .. });
        if let Some((id, _)) = seen.filter(|_| in_collect) {
            self.drop_id.get_or_insert(id);
        }
        if self.drop_id.is_some_and(|id| w.entities.contains_key(&id)) {
            self.drop_last = tick;
        }
        // The drop vanished, not into this bot's hands: someone else took it.
        let taken_by_other = self.drop_id.is_some_and(|id| {
            !w.entities.contains_key(&id) && !w.pickups.iter().any(|p| p.item_entity == id)
        }) && tick.saturating_sub(self.drop_last) > TAKEN_TICKS;
        self.feet = Some(body.pos);
        match &mut self.stage {
            DigStage::Start => {
                if gone && self.tries == 0 {
                    return (still, None, DigStatus::Failed(DigFailure::Gone));
                }
                if gone {
                    self.stage = DigStage::Collect {
                        since: tick,
                        walk: None,
                    };
                    return (still, None, DigStatus::Running);
                }
                let Some(kind) = w.block_at(pos(self.target)) else {
                    return (still, None, DigStatus::Failed(DigFailure::Refused));
                };
                let Some(ticks) = break_ticks(kind, body.eye_in_water, body.on_ground) else {
                    return (still, None, DigStatus::Failed(DigFailure::Refused));
                };
                self.tries += 1;
                let wait =
                    (ticks as f64 * FINISH_MARGIN * slowdown(w)).ceil() as u64 + FINISH_EXTRA_TICKS;
                self.stage = DigStage::Finish {
                    at: tick + wait,
                    started: tick,
                };
                let act = Act::Dig {
                    step: DigStep::Start,
                    pos: pos(self.target),
                    face: 1,
                };
                (still, Some(act), DigStatus::Running)
            }
            DigStage::Finish { at, started } => {
                if tick < *at {
                    return (still, None, DigStatus::Running);
                }
                let waited = tick - *started;
                self.stage = DigStage::Confirm {
                    deadline: tick + waited + CONFIRM_TICKS,
                };
                let act = Act::Dig {
                    step: DigStep::Finish,
                    pos: pos(self.target),
                    face: 1,
                };
                (still, Some(act), DigStatus::Running)
            }
            DigStage::Confirm { deadline } => {
                if gone {
                    self.stage = DigStage::Collect {
                        since: tick,
                        walk: None,
                    };
                    return (still, None, DigStatus::Running);
                }
                if tick < *deadline {
                    return (still, None, DigStatus::Running);
                }
                if self.tries >= MAX_TRIES {
                    return (still, None, DigStatus::Failed(DigFailure::Refused));
                }
                self.stage = DigStage::Start;
                (still, None, DigStatus::Running)
            }
            DigStage::Collect { since, walk } => {
                let Some(item) = self.drop else {
                    return (still, None, DigStatus::Done);
                };
                if held_of(w, item) > self.before {
                    return (still, None, DigStatus::Done);
                }
                if taken_by_other {
                    return (still, None, DigStatus::Failed(DigFailure::Taken));
                }
                if tick.saturating_sub(*since) > COLLECT_TICKS {
                    return (still, None, DigStatus::Failed(DigFailure::Uncollected));
                }
                if tick.saturating_sub(*since) < PICKUP_WAIT_TICKS {
                    return (still, None, DigStatus::Running);
                }
                let Some(at) = drop_at else {
                    // Not in sight: it may be picked up as the server tells it; wait out the
                    // collect's time.
                    return (still, None, DigStatus::Running);
                };
                let cell = path::feet_cell(at);
                let fresh = walk.as_ref().is_none_or(|wk| wk.target != cell);
                if fresh {
                    let mut wk = Walk::new(
                        cell,
                        Goal::Near {
                            pos: cell,
                            range: BESIDE,
                        },
                        Arrive::Goal,
                        tick,
                    );
                    if !wk.route_now(w, tick) {
                        return (still, None, DigStatus::Failed(DigFailure::DropUnreachable));
                    }
                    *walk = Some(wk);
                }
                let Some(wk) = walk.as_mut() else {
                    return (still, None, DigStatus::Running);
                };
                match wk.step(w, tick) {
                    (c, TaskStatus::Running) => (c, None, DigStatus::Running),
                    // At the drop: the touch takes it on the next ticks.
                    (_, TaskStatus::Done) => (still, None, DigStatus::Running),
                    (_, TaskStatus::Failed(_)) => {
                        (still, None, DigStatus::Failed(DigFailure::DropWalkFailed))
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_times_by_hand() {
        // A log, hardness 2: 60 ticks; in water 300; in water and off the ground 1,500.
        assert_eq!(break_ticks(BlockKind::OakLog, false, true), Some(60));
        assert_eq!(break_ticks(BlockKind::OakLog, true, true), Some(300));
        assert_eq!(break_ticks(BlockKind::OakLog, true, false), Some(1500));
        // Stone by hand does not harvest: 1.5 / 100 → 150 ticks; bedrock never.
        assert_eq!(break_ticks(BlockKind::Stone, false, true), Some(150));
        assert_eq!(break_ticks(BlockKind::Bedrock, false, true), None);
        assert_eq!(break_ticks(BlockKind::ShortGrass, false, true), Some(0));
    }
}
