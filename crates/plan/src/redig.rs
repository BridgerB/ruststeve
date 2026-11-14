//! Re-digging (issue 13b, brain/ruststeve/design/veins-and-flint.md): flint from gravel, a port of
//! clojurecraft's `:redig` act. Put a held block down on a full floor beside the feet (the place
//! task) and dig it again (the dig task, its drop collected): one cycle a task. Gravel drops flint
//! one time in ten by its loot table; the needs planner asks again until flint is held, and the
//! gravel spent per flint is counted (the progress invariant's resource), so a run out of gravel
//! is a need for gravel, never a loop.

use data::BlockKind;
use physics::Controls;
use world::{Act, World};

use crate::dig::{Dig, DigFailure, DigStatus};
use crate::place::{Place, PlaceStatus};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RedigFailure {
    Place,
    Dig(DigFailure),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedigStatus {
    Running,
    Done,
    Failed(RedigFailure),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Redig {
    pub block: BlockKind,
    pub place: Option<Place>,
    pub dig: Option<Dig>,
    pub started: u64,
    /// How many of the block's item were held before it was put down: the dig counts its
    /// pickup against one less (the slot update for the placed one can come after the block
    /// update that ends the place, and a count taken then never rose: docs/problems/iron-kit.md
    /// wall 8).
    #[serde(default)]
    pub held_before: i32,
}

impl Redig {
    pub fn new(w: &World, block: BlockKind, tick: u64) -> Option<Self> {
        let place = Place::new(block, tick)?;
        let held_before = crate::craft::count(w, place.item);
        Some(Self {
            block,
            place: Some(place),
            dig: None,
            started: tick,
            held_before,
        })
    }

    pub fn step(&mut self, w: &World, tick: u64) -> (Controls, Option<Act>, RedigStatus) {
        let still = Controls::default();
        if let Some(p) = &mut self.place {
            let (act, status) = p.step(w, tick);
            return match status {
                PlaceStatus::Running => (still, act, RedigStatus::Running),
                PlaceStatus::Done(cell) => {
                    self.place = None;
                    let mut d = Dig::new(w, cell, tick);
                    d.before = self.held_before - 1;
                    self.dig = Some(d);
                    (still, act, RedigStatus::Running)
                }
                PlaceStatus::Failed(_) => (still, act, RedigStatus::Failed(RedigFailure::Place)),
            };
        }
        let Some(d) = &mut self.dig else {
            return (still, None, RedigStatus::Done);
        };
        let (c, act, status) = d.step(w, tick);
        match status {
            DigStatus::Running => (c, act, RedigStatus::Running),
            DigStatus::Done => (c, act, RedigStatus::Done),
            DigStatus::Failed(f) => (c, act, RedigStatus::Failed(RedigFailure::Dig(f))),
        }
    }
}
