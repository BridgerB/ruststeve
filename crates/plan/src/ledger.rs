//! Every task the planner starts and ends, numbered, with its reason and its outcome (issue 16e,
//! phase C): what the case file and the ledger read instead of a 64-note ring. A task starts
//! where `Plan::begin` sets it (a choice, a dig after a walk, an always-row cutting in) and ends
//! when the planner drops it to `Idle` or begins another. Its reason is the note the planner made
//! as it chose it; its outcome is the note made as it ended (done, failed, cut).

use serde::{Deserialize, Serialize};

/// One record of the task ledger. `at` is in server ticks (one clock for the whole ledger).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TaskRecord {
    Started {
        id: u64,
        at: u64,
        /// `Task::name()`: `stairs`, `walk-to-log`, `defend`.
        task: String,
        /// The note made as it was chosen: what, and why. `None` is a task with no reason, which
        /// coverage counts.
        what: Option<String>,
        why: Option<String>,
    },
    Ended {
        id: u64,
        at: u64,
        task: String,
        /// Server ticks it ran.
        ticks: u64,
        /// The note made as it ended (`walk to … failed: NoPath`), when one was.
        outcome: Option<String>,
        why: Option<String>,
    },
    /// Every planner note, whether or not it began or ended a task: waits, refusals, "low on
    /// digs", "cast paused" (the first races with a ledger left these out: blaze wall 8).
    Note { at: u64, what: String, why: String },
}

/// The task open now: its id, name and start.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Open {
    pub id: u64,
    pub task: String,
    pub at: u64,
}

/// Counts for coverage: tasks started, those with a reason, those ended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskCounts {
    pub started: u64,
    pub with_reason: u64,
    pub ended: u64,
}
