//! Issue 20c and 20d (docs/problems/end-portal.md): from the portal room's floor to the End.
//!
//! **20c, the kit** (`Plan::end_kit_task`). The frontier bot stands on the room's floor with no
//! pickaxe, no wood and no table, 3 sticks, 30 raw meat, 7 raw iron, a furnace and 116
//! cobblestone. In phases, each noted when it begins and ends, each bounded by its clock and its
//! failed tasks (the progress invariant):
//! 1. **Up** its own stairs and tunnel (dug cells the server sends as air) to their top,
//!    `[end_kit] way_up`; with no walk, the dry stairs up (`stairs_to_surface`).
//! 2. **Wood** by hand from the trees seen (memory's sightings, an exposed face), to `logs`.
//! 3. **Row**: the `end-kit` goal row through the needs planner where the bot stands: a table,
//!    sticks, the stone pickaxes, the iron smelted with planks, an iron sword and iron boots
//!    (worn by `equip`).
//! 4. **Cook** all the raw meat (the way home's cook, `home_cook`: a hut by night).
//! 5. **Eat** to `fed`.
//! 6. **Down** the same stairs to the room's standing cell; with no walk, the stronghold's own way.
//!
//! **20d, the portal** (`Plan::end_frames_task`): the room's spawner broken with the pickaxe from a
//! cell not over lava; an eye into each empty frame (`empty_frames`, the frame's `eye` property)
//! from the floor outside the ring within `frame_reach` of the frame's top; then the step in.

use data::{BlockKind, ItemKind};
use memory::Memory;
use world::{Act, BlockPos, World};

use crate::make::{self, Choice};
use crate::path::{self, Cell, Goal};
use crate::stronghold::{self, Room};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task, base, craft, eat};

/// The overworld's dimension name.
pub const OVERWORLD: &str = "minecraft:overworld";

/// `[end_kit]` in `config/goals.toml` (moves to E1's `config/end.toml`); each number's source is
/// there. The defaults are the config's values.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EndKitRules {
    /// The top of the bot's own stairs down to the room (the frontier's ledger).
    pub way_up: [i32; 3],
    /// The way's points under it, from the room up (its legs' ends).
    pub way: Vec<[i32; 3]>,
    /// Walks to one way point before it is skipped.
    pub up_walks: u32,
    /// Logs (and planks / 4) held before the row.
    pub logs: i32,
    /// Trees walked to within this flat distance of the feet, and logs counted only within
    /// `log_rise` of the feet's height.
    pub log_reach: f64,
    pub log_rise: i32,
    /// Walks to logs, and walks to look for trees, at most.
    pub log_walks: u32,
    pub look_walks: u32,
    /// With no log seen, the points (x, z) the overland's legs go to, in order, and how near is
    /// reached.
    pub wood_points: Vec<[i32; 2]>,
    pub point_reach: f64,
    /// Stone (or better) pickaxes held after the row.
    pub stone_pickaxes: i32,
    /// Food eaten up to.
    pub fed: i32,
    pub eat_tasks: u32,
    /// A phase ends done or given up within this many ticks and failed tasks.
    pub phase_ticks: u64,
    pub phase_fails: u32,
    /// Walks down to the room begun before the stronghold's own way takes over.
    pub down_walks: u32,
    /// An eye is put in from a feet cell whose eye is within this of the frame's top face.
    pub frame_reach: f64,
    /// UseOns sent to one frame before it is given up for now.
    pub eye_tries: u32,
    /// Ticks between two UseOns (the server's block update comes back in one or two).
    pub eye_every: u64,
    /// Dig tasks begun on the spawner before it is left.
    pub spawner_tries: u32,
    /// The way in: walks to its start, block clicks, and steps in, before the next frame's way.
    pub entry_walks: u32,
    pub entry_clicks: u32,
    pub entries: u32,
}

impl Default for EndKitRules {
    fn default() -> Self {
        Self {
            way_up: [1608, 63, -32],
            way: vec![
                [1620, 17, -36],
                [1609, 23, -38],
                [1602, 27, -38],
                [1596, 33, -38],
                [1590, 39, -38],
                [1596, 45, -38],
                [1602, 51, -38],
                [1608, 57, -38],
            ],
            up_walks: 3,
            logs: 14,
            log_reach: 48.0,
            log_rise: 16,
            log_walks: 24,
            look_walks: 24,
            wood_points: vec![
                [1530, -45],
                [1610, -130],
                [1610, 60],
                [1700, -35],
                [1420, -70],
            ],
            point_reach: 12.0,
            stone_pickaxes: 3,
            fed: 20,
            eat_tasks: 12,
            phase_ticks: 6000,
            phase_fails: 16,
            down_walks: 8,
            frame_reach: 4.5,
            eye_tries: 4,
            eye_every: 20,
            spawner_tries: 6,
            entry_walks: 4,
            entry_clicks: 9,
            entries: 3,
        }
    }
}

/// The kit's phases, in order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum KitPhase {
    #[default]
    Up,
    Wood,
    Row,
    Cook,
    Eat,
    Down,
    Over,
}

impl KitPhase {
    pub fn name(self) -> &'static str {
        match self {
            KitPhase::Up => "up",
            KitPhase::Wood => "wood",
            KitPhase::Row => "row",
            KitPhase::Cook => "cook",
            KitPhase::Eat => "eat",
            KitPhase::Down => "down",
            KitPhase::Over => "over",
        }
    }

    fn next(self) -> KitPhase {
        match self {
            KitPhase::Up => KitPhase::Wood,
            KitPhase::Wood => KitPhase::Row,
            KitPhase::Row => KitPhase::Cook,
            KitPhase::Cook => KitPhase::Eat,
            KitPhase::Eat => KitPhase::Down,
            KitPhase::Down | KitPhase::Over => KitPhase::Over,
        }
    }
}

/// The kit's and the portal's state, one field of `Plan` (`Plan::end_portal`).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EndPortalState {
    pub begun: bool,
    pub phase: KitPhase,
    /// When the phase in hand began, its tasks, and the planner's failure count last seen.
    pub since: u64,
    pub tasks: u32,
    pub fails: u32,
    pub failures_seen: u64,
    pub up_walks: u32,
    pub climb_noted: bool,
    /// The way point walked to next.
    pub way_at: Option<usize>,
    pub log_walks: u32,
    pub look_walks: u32,
    pub eat_tasks: u32,
    /// The wood point walked to next, and whether the overland's legs run for the wood now.
    pub wood_point: usize,
    #[serde(skip)]
    pub wood_legs: bool,
    pub down_walks: u32,
    pub down_noted: bool,
    /// The way point walked down to next.
    pub down_at: Option<usize>,
    /// The night a hut was begun for (the day's number).
    pub hut_night: Option<i64>,
    pub hut_digs: u32,
    // 20d
    pub spawner_done: bool,
    pub spawner_tries: u32,
    /// Per frame, the UseOns sent and when the last went.
    pub eye_tries: Vec<([i32; 3], u32, u64)>,
    pub eye_uses: u32,
    pub frames_noted: bool,
    pub lit_noted: bool,
    /// The way in chosen (`Entry`), the walks to its start, the blocks clicked, the steps in.
    pub entry: Option<Entry>,
    pub entry_walks: u32,
    pub entry_clicks: u32,
    pub entry_clicked_at: u64,
    pub entries: u32,
    /// Frames whose way in was given up.
    pub entry_refused: Vec<[i32; 3]>,
}

// ------------------------------------------------------------------ pure

/// Whether an `end_portal_frame` state holds an eye (its `eye` property), `None` for another
/// block's state. Read from the block report's property table, so no state number is written here.
pub fn frame_has_eye(state: u32) -> Option<bool> {
    if data::block_of_state(state) != Some(BlockKind::EndPortalFrame) {
        return None;
    }
    let info = BlockKind::EndPortalFrame.info()?;
    let idx = state.checked_sub(info.first_state)?;
    let pos = info.properties.iter().position(|p| p.name == "eye")?;
    // The state index is mixed-radix over the properties in order, the last fastest.
    let stride: u32 = info.properties[pos + 1..]
        .iter()
        .map(|p| u32::try_from(p.values.len()).unwrap_or(1))
        .product();
    let n = u32::try_from(info.properties[pos].values.len()).ok()?;
    let v = (idx / stride.max(1)) % n.max(1);
    info.properties[pos]
        .values
        .get(usize::try_from(v).ok()?)
        .map(|s| *s == "true")
}

/// The room's frames whose last sighting in memory holds no eye, in a fixed order (by position).
pub fn empty_frames(mem: &Memory, room: &Room) -> Vec<[i32; 3]> {
    let mut v: Vec<[i32; 3]> = room
        .frames
        .iter()
        .copied()
        .filter(|f| {
            mem.latest(OVERWORLD, (f[0], f[1], f[2]))
                .and_then(|s| frame_has_eye(s.state))
                == Some(false)
        })
        .collect();
    v.sort_unstable();
    v
}

/// The frames without an eye as the world has them now: the loaded state, else memory's last.
pub fn empty_frames_now(w: &World, room: &Room) -> Vec<[i32; 3]> {
    let mut v: Vec<[i32; 3]> = room
        .frames
        .iter()
        .copied()
        .filter(|f| {
            let c = (f[0], f[1], f[2]);
            let state = w
                .state_at(BlockPos::new(c.0, c.1, c.2))
                .or_else(|| w.memory.latest(OVERWORLD, c).map(|s| s.state));
            state.and_then(frame_has_eye) == Some(false)
        })
        .collect();
    v.sort_unstable();
    v
}

/// Stone-or-better pickaxes held (the hotbar, the pack, the off hand).
pub fn pickaxes_held(w: &World) -> i32 {
    [
        ItemKind::StonePickaxe,
        ItemKind::IronPickaxe,
        ItemKind::DiamondPickaxe,
        ItemKind::NetheritePickaxe,
    ]
    .iter()
    .map(|k| craft::count(w, *k))
    .sum()
}

/// The row's things are held: the pickaxes, an iron sword and iron boots (worn counts).
pub fn kit_held(w: &World, r: &EndKitRules) -> bool {
    pickaxes_held(w) >= r.stone_pickaxes
        && craft::count(w, ItemKind::IronSword) >= 1
        && craft::count(w, ItemKind::IronBoots) >= 1
}

/// The kit in one line, for the notes.
pub fn kit_line(w: &World) -> String {
    format!(
        "pickaxes {}, iron swords {}, iron boots {}, logs {}, planks {}, sticks {}, raw iron {}, ingots {}, food {:?}, health {:?}",
        pickaxes_held(w),
        craft::count(w, ItemKind::IronSword),
        craft::count(w, ItemKind::IronBoots),
        base::logs_held(w),
        base::planks_held(w),
        craft::count(w, ItemKind::Stick),
        craft::count(w, ItemKind::RawIron),
        craft::count(w, ItemKind::IronIngot),
        w.player.food,
        w.player.health
    )
}

/// The eye point of a body at `feet` (cell), standing on a full block.
fn eye_of(c: Cell) -> [f64; 3] {
    [
        f64::from(c.0) + 0.5,
        f64::from(c.1) + physics::EYE_HEIGHT,
        f64::from(c.2) + 0.5,
    ]
}

/// The distance from an eye point to the middle of a frame's top face (the frame is 13/16 high).
pub fn to_frame_top(eye: [f64; 3], f: [i32; 3]) -> f64 {
    let top = [
        f64::from(f[0]) + 0.5,
        f64::from(f[1]) + 0.8125,
        f64::from(f[2]) + 0.5,
    ];
    (0..3)
        .map(|i| (eye[i] - top[i]).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// Standing cells for putting an eye into `frame`: on a floor, out of the ring (the frames and the
/// interior, more than `stronghold::RING` from its centre on an axis), no lava in the feet cell,
/// under it or beside them, the eye within `reach` of the frame's top; nearest the feet first.
pub fn eye_stands(w: &World, room: &Room, frame: [i32; 3], from: Cell, reach: f64) -> Vec<Cell> {
    let mut cells = path::Cells::new(w);
    let [cx, _, cz] = room.centre;
    let r = reach.ceil() as i32;
    let mut v: Vec<(i32, Cell)> = Vec::new();
    for y in (frame[1] - 3)..=(frame[1] + 1) {
        for dx in -r..=r {
            for dz in -r..=r {
                let c = (frame[0] + dx, y, frame[2] + dz);
                let ring =
                    (c.0 - cx).abs() <= stronghold::RING && (c.2 - cz).abs() <= stronghold::RING;
                if ring || to_frame_top(eye_of(c), frame) > reach || !cells.standable(c) {
                    continue;
                }
                let hot = (-1..=1).any(|ax| {
                    (-1..=1).any(|az| {
                        (-1..=0)
                            .any(|ay| cells.at((c.0 + ax, c.1 + ay, c.2 + az)) == path::Class::Lava)
                    })
                });
                if hot {
                    continue;
                }
                let d = (c.0 - from.0).abs() + (c.2 - from.2).abs() + 2 * (c.1 - from.1).abs();
                v.push((d, c));
            }
        }
    }
    v.sort();
    v.into_iter().map(|(_, c)| c).collect()
}

/// The lit portal's blocks inside the ring (the 3x3 at the frames' height), as the world has them.
pub fn portal_cells(w: &World, room: &Room) -> Vec<Cell> {
    let [cx, y, cz] = room.centre;
    (-1..=1)
        .flat_map(|dx| (-1..=1).map(move |dz| (cx + dx, y, cz + dz)))
        .filter(|c| w.block_at(BlockPos::new(c.0, c.1, c.2)) == Some(BlockKind::EndPortal))
        .collect()
}

/// A way into the lit portal by a frame: two blocks laid on its outward side, `high` (two
/// high, its top level with the frame's foot) next to the frame and `low` (one high) beyond it;
/// the bot starts on the floor at `start`, beside `low`, steps up both and over the frame's
/// top, and falls into the portal cell `inside`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    pub frame: [i32; 3],
    pub high: Cell,
    pub low: Cell,
    pub start: Cell,
    pub inside: Cell,
}

impl Entry {
    /// The blocks to lay, in order, each with the cell whose top face is clicked.
    pub fn blocks(&self) -> Vec<(Cell, Cell)> {
        let below = |c: Cell| (c.0, c.1 - 1, c.2);
        let over = (self.high.0, self.high.1 + 1, self.high.2);
        vec![
            (self.high, below(self.high)),
            (over, self.high),
            (self.low, below(self.low)),
        ]
    }

    /// The forced route: onto `low`, onto `high`'s top, onto the frame, into the portal.
    pub fn way(&self) -> Vec<Cell> {
        vec![
            (self.low.0, self.low.1 + 1, self.low.2),
            (self.high.0, self.high.1 + 2, self.high.2),
            (self.frame[0], self.frame[1] + 1, self.frame[2]),
            self.inside,
            self.deeper(),
        ]
    }

    /// The cell past `inside` away from the frame: walked toward, so the body's box leaves the
    /// frame's top (race 45877404722 lane 1 stood "in end_portal" with its box still on the
    /// frame's edge at y 19.81, over the portal's 0.75 top, and never went through).
    pub fn deeper(&self) -> Cell {
        let (dx, dz) = (self.frame[0] - self.inside.0, self.frame[2] - self.inside.2);
        (self.inside.0 - dx, self.inside.1, self.inside.2 - dz)
    }
}

/// Every way in (`Entry`) the room's floor allows, by each frame of a side: `high` and `low`
/// on the floor outward of the frame, open, not lava, on a solid floor; a start beside `low`
/// across the axis (or beyond it) on the floor with head room; head room over the steps and the
/// frame's top; no lava within one of the cells walked. Pure over the world as loaded.
pub fn entries(w: &World, room: &Room) -> Vec<Entry> {
    let mut cells = path::Cells::new(w);
    let [cx, fy, cz] = room.centre;
    let floor = room.floor_y;
    let mut v = Vec::new();
    for f in &room.frames {
        let (dx, dz) = ((f[0] - cx).signum(), (f[2] - cz).signum());
        // A side's frame only: the outward axis is the one the frame is RING off on.
        let d = if (f[0] - cx).abs() == stronghold::RING && (f[2] - cz).abs() < stronghold::RING {
            (dx, 0)
        } else if (f[2] - cz).abs() == stronghold::RING && (f[0] - cx).abs() < stronghold::RING {
            (0, dz)
        } else {
            continue;
        };
        let high = (f[0] + d.0, floor, f[2] + d.1);
        let low = (f[0] + 2 * d.0, floor, f[2] + 2 * d.1);
        let inside = (f[0] - d.0, fy, f[2] - d.1);
        let clear = |cells: &mut path::Cells, c: Cell| matches!(cells.at(c), path::Class::Clear);
        let solid = |cells: &mut path::Cells, c: Cell| cells.at(c) == path::Class::Solid;
        let ok = [high, (high.0, high.1 + 1, high.2), low]
            .iter()
            .all(|c| clear(&mut cells, *c) || solid(&mut cells, *c))
            && solid(&mut cells, (high.0, high.1 - 1, high.2))
            && solid(&mut cells, (low.0, low.1 - 1, low.2))
            && [
                (low.0, low.1 + 1, low.2),
                (low.0, low.1 + 2, low.2),
                (high.0, high.1 + 2, high.2),
                (high.0, high.1 + 3, high.2),
                (f[0], f[1] + 1, f[2]),
                (f[0], f[1] + 2, f[2]),
            ]
            .iter()
            .all(|c| clear(&mut cells, *c));
        if !ok {
            continue;
        }
        let hot = [high, low].iter().any(|c| {
            (-1..=1).any(|ax| {
                (-1..=1).any(|az| {
                    (-1..=0).any(|ay| cells.at((c.0 + ax, c.1 + ay, c.2 + az)) == path::Class::Lava)
                })
            })
        });
        if hot {
            continue;
        }
        let across = (d.1, d.0);
        let starts = [
            (low.0 + across.0, floor, low.2 + across.1),
            (low.0 - across.0, floor, low.2 - across.1),
            (low.0 + d.0, floor, low.2 + d.1),
        ];
        if let Some(start) = starts.into_iter().find(|s| cells.standable(*s)) {
            v.push(Entry {
                frame: *f,
                high,
                low,
                start,
                inside,
            });
        }
    }
    v
}

/// The cells of a climb from `from` to `to` the way the bot's own stairs ran down it, reversed:
/// first across the minor axis, then level along the major axis for the run the rise does not
/// take, then a step up a cell for each block of rise (a downward leg cut its stairs first and its
/// level steps after). Each step is one cell on an axis, level or one up (`tunnel::opened`).
pub fn climb_route(from: Cell, to: Cell) -> Vec<Cell> {
    let (dx, dz) = (to.0 - from.0, to.2 - from.2);
    let x_major = dx.abs() >= dz.abs();
    let (major, minor) = if x_major { (dx, dz) } else { (dz, dx) };
    let rise = (to.1 - from.1).max(0);
    let mut c = from;
    let mut v = vec![c];
    let step = |c: Cell, along_x: bool, s: i32, up: i32| {
        if along_x {
            (c.0 + s, c.1 + up, c.2)
        } else {
            (c.0, c.1 + up, c.2 + s)
        }
    };
    for _ in 0..minor.abs() {
        c = step(c, !x_major, minor.signum(), 0);
        v.push(c);
    }
    let run = major.abs();
    for k in 0..run {
        let up = i32::from(k >= run - rise.min(run));
        c = step(c, x_major, major.signum(), up);
        v.push(c);
    }
    v
}

fn overworld(w: &World) -> bool {
    w.dimension.as_ref().is_some_and(|d| d.name == OVERWORLD)
}

fn flat(a: Cell, b: Cell) -> f64 {
    f64::from((a.0 - b.0).pow(2) + (a.2 - b.2).pow(2)).sqrt()
}

impl Plan {
    fn end_rules(&self) -> EndKitRules {
        self.table
            .as_ref()
            .map(|t| t.end_kit.clone())
            .unwrap_or_default()
    }

    fn end_kit_note(&mut self, tick: u64, what: String, w: &World) {
        let line = kit_line(w);
        self.note(tick, what, line);
    }

    /// A phase over: the next begins, noted with the kit held.
    fn end_kit_advance(&mut self, w: &World, tick: u64, why: &str) {
        let from = self.end_portal.phase;
        let to = from.next();
        self.end_portal.phase = to;
        self.end_portal.since = tick;
        self.end_portal.tasks = 0;
        self.end_portal.fails = 0;
        self.end_portal.failures_seen = self.failures.values().sum();
        let line = kit_line(w);
        self.note(
            tick,
            format!("end kit: {} over, {} next", from.name(), to.name()),
            format!("{why}; {line}"),
        );
    }

    // >>> E2 (issue 20c/20d): the planner's one entry, called first in `nether_task`.
    /// In the overworld under the race, once the portal room is reached (or this phase begun):
    /// the End kit, then the portal and the step in. `None` otherwise.
    pub fn end_portal_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if self.aim != crate::Aim::Race || !overworld(w) {
            return None;
        }
        if !self.end_portal.begun {
            let room = w
                .stage_reached
                .is_some_and(|s| s >= world::stage::Stage::PortalRoom);
            if !room {
                return None;
            }
            self.end_portal.begun = true;
            self.end_portal.since = tick;
            self.end_portal.failures_seen = self.failures.values().sum();
            let feet = w.body.map(|b| path::feet_cell(b.pos));
            let line = kit_line(w);
            self.note(
                tick,
                "end kit: begun".to_owned(),
                format!("at {feet:?}, the portal room reached; {line}"),
            );
        }
        // The walks' give-up never ends this (the way home's rule).
        if self.stuck {
            self.stuck = false;
            self.failed_in_row = 0;
            self.note(
                tick,
                "end kit: stuck cleared".to_owned(),
                "the walks' give-up does not end the End's kit".to_owned(),
            );
        }
        if self.end_portal.phase != KitPhase::Over
            && let Some(t) = self.end_kit_task(w, tick)
        {
            return Some(t);
        }
        self.end_frames_task(w, tick)
    }
    // <<< E2

    /// The kit's next task by phase; `None` once over.
    fn end_kit_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let r = self.end_rules();
        for _ in 0..8 {
            let phase = self.end_portal.phase;
            if phase == KitPhase::Over {
                return None;
            }
            // The progress invariant: a phase ends within its clock and its failed tasks.
            let failures: u64 = self.failures.values().sum();
            if failures > self.end_portal.failures_seen {
                self.end_portal.fails +=
                    u32::try_from(failures - self.end_portal.failures_seen).unwrap_or(1);
                self.end_portal.failures_seen = failures;
            }
            let spent = tick.saturating_sub(self.end_portal.since);
            if phase != KitPhase::Up && phase != KitPhase::Down {
                if spent > r.phase_ticks {
                    self.end_kit_advance(
                        w,
                        tick,
                        &format!("gave up: {spent} ticks of {}", r.phase_ticks),
                    );
                    continue;
                }
                if self.end_portal.fails >= r.phase_fails {
                    let fails = self.end_portal.fails;
                    self.end_kit_advance(w, tick, &format!("gave up: {fails} tasks failed"));
                    continue;
                }
            }
            let next = match phase {
                KitPhase::Up => self.end_kit_up(w, &r, tick),
                KitPhase::Wood | KitPhase::Row | KitPhase::Cook | KitPhase::Eat => {
                    self.end_kit_surface(w, &r, phase, tick)
                }
                KitPhase::Down => self.end_kit_down(w, &r, tick),
                KitPhase::Over => return None,
            };
            match next {
                Some(t) => {
                    self.end_portal.tasks += 1;
                    return Some(t);
                }
                None if self.end_portal.phase == phase => return Some(Task::Idle),
                None => {}
            }
        }
        Some(Task::Idle)
    }

    /// Up the bot's own stairs to their top (a walk through cells it dug), else the dry stairs.
    fn end_kit_up(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        if world::stage::open_sky(w) {
            let feet = w.body.map(|b| path::feet_cell(b.pos));
            self.end_kit_advance(w, tick, &format!("on the surface at {feet:?}"));
            return None;
        }
        // The way: the legs' ends from the room up, then the stairs' top; walked a leg at a time
        // from the point after the nearest one (a single route of 100 cells through the stairs
        // and the tunnel ran out of the search's budget).
        let way: Vec<Cell> = r
            .way
            .iter()
            .chain(std::iter::once(&r.way_up))
            .map(|p| (p[0], p[1], p[2]))
            .collect();
        let feet = path::stand_cell(w, w.body?.pos);
        let near = |a: Cell, b: Cell| path::octile(a, b) + f64::from((a.1 - b.1).abs());
        if self.end_portal.way_at.is_none() {
            let i = way
                .iter()
                .enumerate()
                .min_by(|a, b| near(feet, *a.1).total_cmp(&near(feet, *b.1)))
                .map_or(0, |(i, _)| i);
            self.end_portal.way_at = Some(i);
            self.note(
                tick,
                format!("end kit: up its own way from point {i} of {}", way.len()),
                format!("feet {feet:?}, the nearest point {:?}", way.get(i)),
            );
        }
        while let Some(i) = self.end_portal.way_at
            && let Some(p) = way.get(i).copied()
        {
            if near(feet, p) <= 2.0 {
                self.end_portal.way_at = Some(i + 1);
                self.end_portal.up_walks = 0;
                self.note(
                    tick,
                    format!("end kit: way point {i} reached at {p:?}"),
                    format!("feet {feet:?}"),
                );
                continue;
            }
            if self.end_portal.up_walks >= r.up_walks {
                self.end_portal.way_at = Some(i + 1);
                self.end_portal.up_walks = 0;
                self.note(
                    tick,
                    format!("end kit: way point {i} at {p:?} skipped"),
                    format!("{} walks to it from {feet:?}", r.up_walks),
                );
                continue;
            }
            let mut walk = Walk::new(p, Goal::Near { pos: p, range: 1.5 }, Arrive::Goal, tick);
            let routed = walk.route_now(w, tick);
            let found = walk.route;
            self.end_portal.up_walks += 1;
            if routed && matches!(found, Some(path::Status::Found | path::Status::Partial)) {
                self.walks += 1;
                let n = walk.waypoints.as_ref().map_or(0, Vec::len);
                self.note(
                    tick,
                    format!("end kit: up its own way to point {i} at {p:?}"),
                    format!(
                        "from {feet:?}, walk {} of {}, {n} cells on a {found:?} route",
                        self.end_portal.up_walks, r.up_walks
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            // The route's end under the point (race 45865135195's replan: from the room the
            // way's reach ends in the corridor at (1609, 19, -38), 4 under the stair it dropped
            // from): walked to, then stairs up toward the point from there.
            let end = walk.waypoints.as_ref().and_then(|v| v.last().copied());
            // Only a reach nearer the point than the feet (from a way point the reach can be back
            // down in the corridor).
            if let Some(e) =
                end.filter(|e| near(feet, *e) > 1.5 && near(*e, p) + 2.0 < near(feet, p))
            {
                let mut to_end = Walk::new(e, Goal::Block { pos: e }, Arrive::Goal, tick);
                if to_end.route_now(w, tick) && to_end.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("end kit: to the way's reach at {e:?} under point {i}"),
                        format!("from {feet:?}, route to {p:?} {found:?}"),
                    );
                    return Some(Task::Explore(to_end));
                }
                self.expanded += to_end.expanded;
            }
            // Right under it: pillared up to its height on held blocks (Q7's shaft, stopped there;
            // race 45865135195: the corridor 4 under the bot's own tunnel start).
            if p.1 > feet.1 + 1
                && flat(feet, p) <= 1.0
                && let Some(t) = self.arrival_rise_to(w, tick, p.1)
            {
                self.note(
                    tick,
                    format!("end kit: pillar up to point {i} at {p:?}"),
                    format!("from {feet:?}, no walk (route {found:?})"),
                );
                return Some(t);
            }
            // From a way point up to the next: the bot's own leg back up, cell by cell
            // (`climb_route`), only the head room its downward cut left out dug (race 45871438675:
            // the shaft straight up met an aquifer at y 30 beside the old stairs, which are dry).
            if i > 0 && p.1 > feet.1 && way.get(i - 1).is_some_and(|q| near(feet, *q) <= 2.0) {
                let route = climb_route(feet, p);
                self.note(
                    tick,
                    format!("end kit: up the old leg to point {i} at {p:?}"),
                    format!(
                        "from {feet:?}, {} steps, no walk (route {found:?})",
                        route.len()
                    ),
                );
                return Some(Task::Stairs(Box::new(crate::stairs::Stairs::along(
                    route, tick,
                ))));
            }
            // No walk up: a shaft straight up from here (Q7's, one dig a block by hand and a block
            // under the feet) to the sky. The bot's own stairs were cut going down and no walk
            // climbs them (race 45870075221: every point past the tunnel "route Some(None)";
            // stairs up by hand took 2,900 ticks for 5).
            if p.1 > feet.1 + 1
                // No height stop: out under the sky (`arrival::Step::Out`); a stop at the stairs'
                // top would end it in rock under a hill.
                && let Some(t) = self.arrival_rise_to(w, tick, i32::MAX)
            {
                self.note(
                    tick,
                    format!("end kit: shaft up from {feet:?} toward point {i} at {p:?}"),
                    format!("no walk (route {found:?}); out under the sky"),
                );
                return Some(t);
            }
            if p.1 > feet.1 + 1 {
                let why = format!("point {i}, no walk from {feet:?} (route {found:?})");
                if let Some(t) = self.dig_toward(w, p, true, ("the way's point", &why), tick) {
                    self.note(
                        tick,
                        format!("end kit: stairs up toward point {i} at {p:?}"),
                        why,
                    );
                    return Some(t);
                }
            }
            self.note(
                tick,
                format!("end kit: no walk to point {i} at {p:?}"),
                format!("from {feet:?}, route {found:?}"),
            );
            return Some(Task::Idle);
        }
        if !self.end_portal.climb_noted {
            self.end_portal.climb_noted = true;
            self.note(
                tick,
                "end kit: the dry stairs up".to_owned(),
                format!("{} walks up spent", self.end_portal.up_walks),
            );
        }
        Some(self.stairs_to_surface(w, tick).unwrap_or(Task::Idle))
    }

    /// The surface's phases: the night first (a hut, the cook in it), then the phase in hand.
    fn end_kit_surface(
        &mut self,
        w: &World,
        r: &EndKitRules,
        phase: KitPhase,
        tick: u64,
    ) -> Option<Task> {
        let rules = self.table.as_ref().map(|t| t.survival.clone())?;
        let night = self.dusk_near(w, tick) || w.is_night() == Some(true);
        if night {
            let day = w.day_time().unwrap_or(0) / 24_000;
            if self.end_portal.hut_night != Some(day) {
                self.end_portal.hut_night = Some(day);
                self.hut_done = false;
                self.hut_blocks = 0;
                self.cook_over = false;
                self.cook_tasks = 0;
                self.ender.home.cook_noted = false;
                self.end_kit_note(
                    tick,
                    format!("end kit: a hut for the night ({})", phase.name()),
                    w,
                );
            }
            if let Some(t) = self.hut_step(w, tick) {
                return Some(t);
            }
        }
        match phase {
            KitPhase::Wood => {
                let wood = base::logs_held(w) + base::planks_held(w) / 4;
                if wood >= r.logs || kit_held(w, r) {
                    self.end_kit_advance(w, tick, &format!("{wood} logs' worth held"));
                    return None;
                }
                if night {
                    self.retry_at = tick + crate::RETRY_TICKS * 10;
                    return Some(Task::Idle);
                }
                if let Some(t) = self.end_hut_exit(w, None, tick) {
                    return Some(t);
                }
                self.end_kit_wood(w, r, tick)
            }
            KitPhase::Row => {
                if kit_held(w, r) {
                    self.end_kit_advance(w, tick, "the row's things held");
                    return None;
                }
                self.end_kit_row(w, r, tick)
            }
            KitPhase::Cook => {
                let raw: i32 = eat::raw_held(&crate::held(w), &rules)
                    .iter()
                    .map(|x| x.1)
                    .sum();
                if raw == 0 && self.batch.is_none() {
                    self.end_kit_advance(w, tick, "no raw meat left");
                    return None;
                }
                // The row's table taken along first: at hand, the cook's planner presses the
                // cobblestone into furnaces (race 45877404722 lane 0: ten, 80 cobblestone).
                if let Some(t) = self.at_hand(w, BlockKind::CraftingTable, tick) {
                    self.note(
                        tick,
                        format!("end kit: the row's table at {t:?} taken along"),
                        "before the cook, so no stone is pressed into furnaces".to_owned(),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, t, tick)));
                }
                if let Some(t) = self.home_cook(w, tick, &rules, night) {
                    return Some(t);
                }
                self.end_kit_advance(w, tick, &format!("the cook over with {raw} raw left"));
                None
            }
            KitPhase::Eat => {
                let food = w.player.food.unwrap_or(0);
                if food >= r.fed || self.end_portal.eat_tasks >= r.eat_tasks {
                    let n = self.end_portal.eat_tasks;
                    self.end_kit_advance(w, tick, &format!("food {food} after {n} meals"));
                    return None;
                }
                let Some(f) = eat::best_food(w, &rules) else {
                    self.end_kit_advance(w, tick, &format!("nothing to eat at food {food}"));
                    return None;
                };
                self.end_portal.eat_tasks += 1;
                self.note(
                    tick,
                    format!("end kit: eat {}", f.name().unwrap_or_default()),
                    format!("food {food} under {}", r.fed),
                );
                Some(Task::Eat(eat::Eat::new(w, f, tick)))
            }
            KitPhase::Up | KitPhase::Down | KitPhase::Over => None,
        }
    }

    /// Out of the hut: a wall cell beside the feet dug (the head's first), toward `toward` when
    /// given; `None` once a side is open at the feet and the head, or no hut was built.
    fn end_hut_exit(&mut self, w: &World, toward: Option<Cell>, tick: u64) -> Option<Task> {
        if self.hut_blocks == 0 {
            return None;
        }
        let feet = path::feet_cell(w.body?.pos);
        // A hut is on the surface: in the bot's own stairs a wall is no hut (race 45877404722
        // lane 0 dug its stair's side at (1591, 39, -38)).
        if feet.1 < self.end_rules().way_up[1] - 8 {
            return None;
        }
        let mut cells = path::Cells::new(w);
        let sides = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        let open = |cells: &mut path::Cells, (dx, dz): (i32, i32)| {
            let c = (feet.0 + dx, feet.1, feet.2 + dz);
            cells.at(c) == path::Class::Clear && cells.at((c.0, c.1 + 1, c.2)) == path::Class::Clear
        };
        if sides.iter().any(|s| open(&mut cells, *s)) {
            return None;
        }
        let dir = toward.map_or((1, 0), |t| stronghold::axis_toward(feet, t));
        let mut order = vec![dir];
        order.extend(sides.iter().copied().filter(|s| *s != dir));
        let keep = |k: Option<BlockKind>| {
            matches!(
                k,
                Some(BlockKind::Furnace | BlockKind::CraftingTable | BlockKind::Lava)
            )
        };
        for (dx, dz) in order {
            let foot = (feet.0 + dx, feet.1, feet.2 + dz);
            let head = (foot.0, foot.1 + 1, foot.2);
            let kinds = [head, foot].map(|c| w.block_at(BlockPos::new(c.0, c.1, c.2)));
            if kinds.iter().any(|k| keep(*k))
                || cells.at((foot.0, foot.1 - 1, foot.2)) == path::Class::Lava
            {
                continue;
            }
            for c in [head, foot] {
                if cells.at(c) == path::Class::Solid {
                    self.end_portal.hut_digs += 1;
                    self.note(
                        tick,
                        format!("end kit: out of the hut, dig {c:?}"),
                        format!("from {feet:?}; dig {}", self.end_portal.hut_digs),
                    );
                    return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
                }
            }
        }
        None
    }

    /// The nearest log on record within `log_rise` of the feet's height (memory's sightings, an
    /// exposed face; race 45875661476: memory held logs at y -33 to -44 under the plain, and every
    /// leg went toward them), not refused, no other player at it.
    fn end_log(&self, w: &World, r: &EndKitRules, tick: u64) -> Option<Cell> {
        let body = w.body?;
        let dim = w.dimension.as_ref()?.name.clone();
        let logs = data::generated::tags::block_tag("logs")?;
        let feet = path::feet_cell(body.pos);
        let (cx, cz) = (feet.0 >> 4, feet.2 >> 4);
        let mut best: Option<(f64, Cell)> = None;
        for dx in -crate::SEARCH_COLUMNS..=crate::SEARCH_COLUMNS {
            for dz in -crate::SEARCH_COLUMNS..=crate::SEARCH_COLUMNS {
                for s in w.memory.in_column(&dim, cx + dx, cz + dz) {
                    let is_log = data::block_of_state(s.state).is_some_and(|k| logs.contains(&k));
                    if !is_log
                        || (s.pos.1 - feet.1).abs() > r.log_rise
                        || self.refused_now(s.pos, tick)
                        || crate::crowded(w, s.pos)
                    {
                        continue;
                    }
                    let d = flat(feet, s.pos);
                    if best.is_none_or(|(bd, bc)| d < bd || (d == bd && s.pos < bc)) {
                        best = Some((d, s.pos));
                    }
                }
            }
        }
        best.map(|(_, c)| c)
    }

    /// Wood by hand: the nearest log seen within `log_reach` with a found walk to it, dug by hand;
    /// with none, a walk to look for trees.
    fn end_kit_wood(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        if let Some(a) = base::bare_hand(w) {
            self.acts.push(a);
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        }
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        if self.end_portal.log_walks < r.log_walks {
            for _ in 0..crate::CANDIDATES_PER_TICK {
                let Some(log) = self.end_log(w, r, tick) else {
                    break;
                };
                let d = flat(feet, log);
                if d > r.log_reach {
                    break;
                }
                let mut walk = Walk::to_dig(log, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.end_portal.log_walks += 1;
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("end kit: wood by hand at {log:?}"),
                        format!(
                            "{} logs, {} planks held, {} wanted; {d:.0} off; walk {} of {}",
                            base::logs_held(w),
                            base::planks_held(w),
                            r.logs,
                            self.end_portal.log_walks,
                            r.log_walks
                        ),
                    );
                    return Some(Task::ToLog(walk));
                }
                self.expanded += walk.expanded;
                self.refuse(log, tick);
            }
        }
        // No log in walking reach: the overland's legs (seen ground, rivers bridged, a hut at
        // night) toward the nearest log on record, else the next of `wood_points` (race
        // 45873894148: plains and a river round the stairs' top, no log within 48, and eight
        // explore walks NoPath at once).
        let goal = self
            .end_log(w, r, tick)
            .map(|c| {
                (
                    [f64::from(c.0) + 0.5, f64::from(c.2) + 0.5],
                    format!("the log at {c:?}"),
                )
            })
            .or_else(|| {
                r.wood_points.get(self.end_portal.wood_point).map(|p| {
                    (
                        [f64::from(p[0]), f64::from(p[1])],
                        format!("wood point {}", self.end_portal.wood_point),
                    )
                })
            });
        if let Some((p, what)) = goal
            && self.end_portal.look_walks < r.look_walks
        {
            let here = [body.pos[0], body.pos[2]];
            let d = (p[0] - here[0]).hypot(p[1] - here[1]);
            if what.starts_with("wood point") && (d <= r.point_reach || self.overland.arrived) {
                self.end_portal.wood_point += 1;
                self.overland.arrived = false;
                self.note(
                    tick,
                    format!("end kit: {what} reached, no log seen"),
                    format!("{d:.0} off; on to the next"),
                );
                self.retry_at = tick + crate::RETRY_TICKS;
                return Some(Task::Idle);
            }
            if self
                .overland
                .fix
                .is_none_or(|(q, _)| (q[0] - p[0]).hypot(q[1] - p[1]) > 4.0)
            {
                self.overland.retarget((p, 0.0), true);
                self.end_portal.look_walks += 1;
                self.note(
                    tick,
                    format!("end kit: legs toward {what}"),
                    format!(
                        "({:.0}, {:.0}), {d:.0} off; look {} of {}; {} logs held",
                        p[0],
                        p[1],
                        self.end_portal.look_walks,
                        r.look_walks,
                        base::logs_held(w)
                    ),
                );
            }
            self.end_portal.wood_legs = true;
            let t = self.overland_task(w, tick);
            self.end_portal.wood_legs = false;
            if let Some(t) = t {
                return Some(t);
            }
            self.overland.arrived = true;
            return Some(Task::Idle);
        }
        let wood = base::logs_held(w) + base::planks_held(w) / 4;
        self.end_kit_advance(
            w,
            tick,
            &format!("no more wood found: {wood} logs' worth, on with it"),
        );
        None
    }

    /// One act of the `end-kit` row from the needs planner, where the bot stands: a craft (at the
    /// table at hand), a placing, a smelt at the furnace at hand, or a log gathered.
    fn end_kit_row(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        let table = self.table.clone()?;
        let Some(row) = table.until("end-kit") else {
            self.end_kit_advance(w, tick, "no end-kit row in the goal table");
            return None;
        };
        let table_cell = self.at_hand(w, BlockKind::CraftingTable, tick);
        let furnace_cell = self.at_hand(w, BlockKind::Furnace, tick);
        let (table_near, furnace_near) = (table_cell.is_some(), furnace_cell.is_some());
        let near = move |b: BlockKind| match b {
            BlockKind::CraftingTable => table_near,
            BlockKind::Furnace => furnace_near,
            _ => false,
        };
        let counts = crate::held(w);
        let choice =
            make::make_with_wear(&table, &counts, &near, true, &make::Wear::default(), row);
        let act = match choice {
            Choice::Done => {
                self.end_kit_advance(w, tick, "the end-kit row is met");
                return None;
            }
            Choice::Stuck => {
                self.end_kit_advance(w, tick, "gave up: the end-kit row is not makeable");
                return None;
            }
            Choice::Act(a) => a,
        };
        let id = act.id.clone();
        let why = format!("the end-kit row's next step ({id})");
        match act.act {
            Some(make::Act::Craft(rec)) if !make::fits_grid(rec) => {
                let Some(t) = table_cell else {
                    self.end_kit_advance(w, tick, &format!("gave up: {id} wants a table at hand"));
                    return None;
                };
                self.note(
                    tick,
                    format!("end kit: craft {} at the table {t:?}", rec.id),
                    why,
                );
                Some(Task::Craft(crate::craft::Craft::at_table(rec, t, tick)))
            }
            Some(make::Act::Craft(rec)) => {
                self.note(tick, format!("end kit: craft {}", rec.id), why);
                Some(Task::Craft(crate::craft::Craft::new(rec, tick)))
            }
            Some(make::Act::Place(b)) => {
                let p = crate::place::Place::new(b, tick)?;
                self.note(tick, format!("end kit: place {b:?}"), why);
                Some(Task::Place(p))
            }
            Some(make::Act::Smelt(s)) => {
                let Some(f) = furnace_cell else {
                    self.end_kit_advance(
                        w,
                        tick,
                        &format!("gave up: {id} wants a furnace at hand"),
                    );
                    return None;
                };
                let count = s
                    .input
                    .iter()
                    .map(|i| craft::count(w, *i))
                    .sum::<i32>()
                    .min(64);
                // The whole batch's fuel first, planks from the logs held (race 45877404722: the
                // smelt of 7 raw iron ran out of fuel, "failed Missing", 4 left in the furnace and
                // no boots).
                let need = count * i32::try_from(s.ticks).unwrap_or(200);
                let have: i32 = crate::smelt::fuels_held(w)
                    .iter()
                    .map(|(k, t)| craft::count(w, *k) * i32::try_from(*t).unwrap_or(0))
                    .sum();
                if have < need && base::logs_held(w) > 0 {
                    let plank = 300;
                    let more = (need - have + plank - 1) / plank;
                    let want = base::planks_held(w) + more;
                    if let Some(n) = make::Need::parse("tag:planks") {
                        let fuel_row = make::Row {
                            id: "end-kit-fuel".to_owned(),
                            needs: Vec::new(),
                            provides: vec![(n, want)],
                            holds: Vec::new(),
                            spares: Vec::new(),
                            wants: Vec::new(),
                            act: None,
                        };
                        if let Choice::Act(a) = make::make_with_wear(
                            &table,
                            &counts,
                            &near,
                            false,
                            &make::Wear::default(),
                            &fuel_row,
                        ) && let Some(make::Act::Craft(rec)) = a.act
                            && make::fits_grid(rec)
                        {
                            self.note(
                                tick,
                                format!("end kit: craft {} for the smelt's fuel", rec.id),
                                format!("{count} to smelt want {need} ticks, {have} held"),
                            );
                            return Some(Task::Craft(crate::craft::Craft::new(rec, tick)));
                        }
                    }
                }
                let sm = crate::smelt::Smelt::new(w, f, s, count, tick)?;
                self.note(
                    tick,
                    format!("end kit: smelt {count} for {} at {f:?}", s.id),
                    why,
                );
                Some(Task::Smelt(Box::new(sm)))
            }
            Some(make::Act::Gather) => {
                // Wood short for the row: one more log by hand.
                let before = self.end_portal.phase;
                let t = self.end_kit_wood(w, r, tick);
                if self.end_portal.phase != before {
                    // The wood gave out: the row is over with what is held.
                    self.end_portal.phase = before;
                    self.end_kit_advance(w, tick, "gave up: no wood for the row");
                }
                t
            }
            Some(make::Act::Mine { .. } | make::Act::Redig(_) | make::Act::Scoop) | None => {
                self.end_kit_advance(w, tick, &format!("gave up: {id} would be a trip"));
                None
            }
        }
    }

    /// Down the same stairs to the room's standing cell; with no walk, the stronghold's way in.
    fn end_kit_down(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        let rules = self.stronghold_rules();
        let feet = path::stand_cell(w, w.body?.pos);
        let frames = stronghold::frames_near(w, rules.frame_chunks);
        let Some(room) = stronghold::room_of(&frames, rules.frame_over_feet) else {
            self.end_kit_advance(
                w,
                tick,
                "no frame on record: the room's way is the stronghold's",
            );
            return None;
        };
        if stronghold::inside(&room, feet, rules.room_reach) {
            self.end_kit_advance(w, tick, &format!("back in the portal room at {feet:?}"));
            return None;
        }
        let target = stronghold::stand_target(w, &room, feet, rules.room_reach);
        if let Some(t) = self.end_hut_exit(w, target, tick) {
            return Some(t);
        }
        // A found walk straight to the room, else its own way down a point at a time.
        if let Some(target) = target {
            let mut walk = Walk::new(
                target,
                Goal::Near {
                    pos: target,
                    range: 1.0,
                },
                Arrive::Goal,
                tick,
            );
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.walks += 1;
                let n = walk.waypoints.as_ref().map_or(0, Vec::len);
                self.note(
                    tick,
                    format!("end kit: down to the room at {target:?}"),
                    format!("from {feet:?}, {n} cells on a found route"),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
        }
        if let Some(t) = self.end_way_down(w, r, feet, tick) {
            return Some(t);
        }
        if !self.end_portal.down_noted {
            self.end_portal.down_noted = true;
            self.note(
                tick,
                "end kit: down by the stronghold's way".to_owned(),
                format!(
                    "{} walks down spent; target {target:?}",
                    self.end_portal.down_walks
                ),
            );
        }
        // The stronghold's own approach (its stairs, its tunnel) knows the room; its inside
        // steps (the spawner, a nook) are this module's once in.
        Some(self.stronghold_task(w, tick).unwrap_or(Task::Idle))
    }

    /// Down the bot's own way, a point at a time from the nearest, top to bottom: far from the
    /// top, the overland's legs to it; a walk to each point (found or partial); a point right
    /// under the feet with no walk (the pillar the way up left in the corridor) dug down to, the
    /// cell under the feet at a time. `None` once the way is spent.
    fn end_way_down(&mut self, w: &World, r: &EndKitRules, feet: Cell, tick: u64) -> Option<Task> {
        let way: Vec<Cell> = r
            .way
            .iter()
            .chain(std::iter::once(&r.way_up))
            .map(|p| (p[0], p[1], p[2]))
            .collect();
        let near = |a: Cell, b: Cell| path::octile(a, b) + f64::from((a.1 - b.1).abs());
        if self.end_portal.down_at.is_none() {
            let i = way
                .iter()
                .enumerate()
                .min_by(|a, b| near(feet, *a.1).total_cmp(&near(feet, *b.1)))
                .map_or(0, |(i, _)| i);
            self.end_portal.down_at = Some(i);
            self.end_portal.down_walks = 0;
            self.note(
                tick,
                format!("end kit: down its own way from point {i}"),
                format!("feet {feet:?}, the nearest point {:?}", way.get(i)),
            );
        }
        while let Some(i) = self.end_portal.down_at {
            let p = *way.get(i)?;
            if near(feet, p) <= 2.0 {
                self.end_portal.down_walks = 0;
                self.note(
                    tick,
                    format!("end kit: down at point {i} {p:?}"),
                    format!("feet {feet:?}"),
                );
                if i == 0 {
                    self.end_portal.down_at = None;
                    return None;
                }
                self.end_portal.down_at = Some(i - 1);
                continue;
            }
            if self.end_portal.down_walks >= r.down_walks {
                self.end_portal.down_walks = 0;
                self.note(
                    tick,
                    format!("end kit: down point {i} at {p:?} skipped"),
                    format!("from {feet:?}"),
                );
                if i == 0 {
                    self.end_portal.down_at = None;
                    return None;
                }
                self.end_portal.down_at = Some(i - 1);
                continue;
            }
            self.end_portal.down_walks += 1;
            // Far from the way (a wood walk ended out on the plain): the overland's legs.
            if flat(feet, p) > 40.0 {
                let q = [f64::from(p.0) + 0.5, f64::from(p.2) + 0.5];
                if self
                    .overland
                    .fix
                    .is_none_or(|(o, _)| (o[0] - q[0]).hypot(o[1] - q[1]) > 4.0)
                {
                    self.overland.retarget((q, 0.0), true);
                    self.note(
                        tick,
                        format!("end kit: legs back to point {i} at {p:?}"),
                        format!("from {feet:?}"),
                    );
                }
                self.end_portal.wood_legs = true;
                let t = self.overland_task(w, tick);
                self.end_portal.wood_legs = false;
                if let Some(t) = t {
                    return Some(t);
                }
            }
            let mut walk = Walk::new(p, Goal::Near { pos: p, range: 1.5 }, Arrive::Goal, tick);
            let routed = walk.route_now(w, tick);
            let found = walk.route;
            if routed && matches!(found, Some(path::Status::Found | path::Status::Partial)) {
                self.walks += 1;
                self.note(
                    tick,
                    format!("end kit: down its own way to point {i} at {p:?}"),
                    format!(
                        "from {feet:?}, a {found:?} route, walk {}",
                        self.end_portal.down_walks
                    ),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            let under = (feet.0, feet.1 - 1, feet.2);
            let mut cells = path::Cells::new(w);
            let safe = cells.at((under.0, under.1 - 1, under.2)) != path::Class::Lava
                && cells.at(under) == path::Class::Solid;
            if p.1 < feet.1 && flat(feet, p) <= 1.0 && safe {
                self.note(
                    tick,
                    format!("end kit: dig down to point {i}, {under:?}"),
                    format!("from {feet:?}, no walk (route {found:?})"),
                );
                return Some(Task::Dig(crate::dig::Dig::new(w, under, tick)));
            }
            self.note(
                tick,
                format!("end kit: no walk down to point {i} at {p:?}"),
                format!("from {feet:?}, route {found:?}"),
            );
            return Some(Task::Idle);
        }
        None
    }

    /// 20d: in the room with the kit, the spawner broken, then an eye into each empty frame, then
    /// the step in. `Task::Idle` while it waits; `None` with no room known (the stronghold's).
    fn end_frames_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        let rules = self.stronghold_rules();
        let r = self.end_rules();
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let frames = stronghold::frames_near(w, rules.frame_chunks);
        let room = stronghold::room_of(&frames, rules.frame_over_feet)?;
        if !self.end_portal.frames_noted {
            self.end_portal.frames_noted = true;
            let empty = empty_frames_now(w, &room);
            self.note(
                tick,
                "end portal: the frames read".to_owned(),
                format!(
                    "ring centre {:?}, {} frames, {} empty {empty:?}, {} eyes held",
                    room.centre,
                    room.frames.len(),
                    empty.len(),
                    craft::count(w, ItemKind::EnderEye)
                ),
            );
        }
        if !self.end_portal.spawner_done
            && let Some(t) = self.end_spawner(w, &room, &r, feet, tick)
        {
            return Some(t);
        }
        let empty = empty_frames_now(w, &room);
        if let Some(t) = self.end_eyes(w, &room, &r, &empty, feet, tick) {
            return Some(t);
        }
        if !empty.is_empty() {
            self.retry_at = tick + crate::RETRY_TICKS * 5;
            return Some(Task::Idle);
        }
        let lit = portal_cells(w, &room);
        if lit.is_empty() {
            self.retry_at = tick + crate::RETRY_TICKS;
            return Some(Task::Idle);
        }
        if !self.end_portal.lit_noted {
            self.end_portal.lit_noted = true;
            self.note(
                tick,
                "end portal: lit".to_owned(),
                format!(
                    "{} portal blocks seen; {} eye uses sent",
                    lit.len(),
                    self.end_portal.eye_uses
                ),
            );
        }
        self.end_entry(w, &room, &r, feet, tick)
    }

    /// The step in: two steps of held blocks laid outside a frame (`entry_for`), then a forced
    /// walk up them, over the frame's top and into the lit portal (`Task::Enter`, done in the
    /// End). Each block, walk and step noted; a way whose blocks will not go in is given up for
    /// the next frame.
    fn end_entry(
        &mut self,
        w: &World,
        room: &Room,
        r: &EndKitRules,
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        let e = match self.end_portal.entry.clone() {
            Some(e) => e,
            None => {
                let refused = self.end_portal.entry_refused.clone();
                let Some(e) = entries(w, room)
                    .into_iter()
                    .filter(|e| !refused.contains(&e.frame))
                    .min_by_key(|e| (e.start.0 - feet.0).abs() + (e.start.2 - feet.2).abs())
                else {
                    if self.end_portal.entry_walks == 0 {
                        self.end_portal.entry_walks = 1;
                        self.note(
                            tick,
                            "end portal: no way in found".to_owned(),
                            format!("from {feet:?}; frames refused {refused:?}"),
                        );
                    }
                    self.retry_at = tick + crate::RETRY_TICKS * 10;
                    return Some(Task::Idle);
                };
                self.note(
                    tick,
                    format!("end portal: the way in by the frame at {:?}", e.frame),
                    format!(
                        "steps at {:?} and {:?}, from {:?}, into {:?}",
                        e.high, e.low, e.start, e.inside
                    ),
                );
                self.end_portal.entry = Some(e.clone());
                self.end_portal.entry_walks = 0;
                self.end_portal.entry_clicks = 0;
                e
            }
        };
        let give_up = |p: &mut Plan, why: String, tick: u64| {
            p.end_portal.entry_refused.push(e.frame);
            p.end_portal.entry = None;
            p.note(
                tick,
                format!("end portal: the way in by {:?} given up", e.frame),
                why,
            );
            Some(Task::Idle)
        };
        let mut cells = path::Cells::new(w);
        let todo: Vec<(Cell, Cell)> = e
            .blocks()
            .into_iter()
            .filter(|(c, _)| cells.at(*c) != path::Class::Solid)
            .collect();
        let at_start = feet == e.start;
        if !at_start && (feet.1 <= e.start.1 || !todo.is_empty()) {
            if self.end_portal.entry_walks >= r.entry_walks {
                return give_up(
                    self,
                    format!("{} walks to {:?}", r.entry_walks, e.start),
                    tick,
                );
            }
            let mut walk = Walk::new(e.start, Goal::Block { pos: e.start }, Arrive::Goal, tick);
            if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                self.end_portal.entry_walks += 1;
                self.walks += 1;
                self.note(
                    tick,
                    format!("end portal: walk to the way in's start {:?}", e.start),
                    format!("from {feet:?}; walk {}", self.end_portal.entry_walks),
                );
                return Some(Task::Explore(walk));
            }
            self.expanded += walk.expanded;
            return give_up(
                self,
                format!("no walk to {:?} from {feet:?}", e.start),
                tick,
            );
        }
        if let Some((cell, click)) = todo.first().copied() {
            if self.end_portal.entry_clicks >= r.entry_clicks {
                return give_up(
                    self,
                    format!("{} clicks and {cell:?} still open", r.entry_clicks),
                    tick,
                );
            }
            if tick < self.end_portal.entry_clicked_at + r.eye_every
                || w.window.is_some()
                || w.click.is_some()
                || !self.acts.is_empty()
            {
                return Some(Task::Idle);
            }
            let Some(item) = crate::SHELTER_BLOCKS
                .iter()
                .copied()
                .find(|k| craft::count(w, *k) > 0)
            else {
                return give_up(self, "no block held for the steps".to_owned(), tick);
            };
            return match crate::hold::toward(w, item) {
                crate::hold::Hold::Held => {
                    self.end_portal.entry_clicks += 1;
                    self.end_portal.entry_clicked_at = tick;
                    self.acts.push(Act::UseOn {
                        pos: BlockPos::new(click.0, click.1, click.2),
                        face: 1,
                    });
                    self.note(
                        tick,
                        format!("end portal: step block at {cell:?}"),
                        format!(
                            "{item:?} on {click:?} from {feet:?}; click {}",
                            self.end_portal.entry_clicks
                        ),
                    );
                    Some(Task::Idle)
                }
                crate::hold::Hold::Act(a) => {
                    self.acts.push(a);
                    Some(Task::Idle)
                }
                crate::hold::Hold::Wait => Some(Task::Idle),
                crate::hold::Hold::Missing => {
                    give_up(self, "the steps' block went missing".to_owned(), tick)
                }
            };
        }
        if self.end_portal.entries >= r.entries {
            return give_up(self, format!("{} steps in", r.entries), tick);
        }
        self.end_portal.entries += 1;
        let mut walk = Walk::new(e.inside, Goal::Block { pos: e.inside }, Arrive::Goal, tick);
        walk.waypoints = Some(e.way());
        walk.more.kinds = vec![
            path::MoveKind::JumpUp,
            path::MoveKind::JumpUp,
            path::MoveKind::JumpUp,
            path::MoveKind::Walk,
            path::MoveKind::Walk,
        ];
        walk.route = Some(path::Status::Found);
        self.walks += 1;
        self.note(
            tick,
            format!("end portal: step in by the frame at {:?}", e.frame),
            format!(
                "from {feet:?} up {:?}, {:?}, over the frame into {:?}; step in {}",
                e.low, e.high, e.inside, self.end_portal.entries
            ),
        );
        Some(Task::Enter(Box::new(crate::Enter {
            walk,
            cell: e.inside,
            inside_since: None,
            from: Some(OVERWORLD.to_owned()),
        })))
    }

    /// The room's spawner: dug with the pickaxe from a cell in reach not over lava, walked to on
    /// a found route first. Done once it is gone, or left after `spawner_tries` digs.
    fn end_spawner(
        &mut self,
        w: &World,
        room: &Room,
        r: &EndKitRules,
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        let centre = (room.centre[0], room.centre[1], room.centre[2]);
        let spawner = world::stage::kind_near(w, BlockKind::Spawner, 1)
            .into_iter()
            .find(|s| path::octile(*s, centre) <= 12.0 && (s.1 - centre.1).abs() <= 4)
            .filter(|s| w.block_at(BlockPos::new(s.0, s.1, s.2)) == Some(BlockKind::Spawner));
        let Some(s) = spawner else {
            self.end_portal.spawner_done = true;
            self.note(
                tick,
                "end portal: the room's spawner is gone".to_owned(),
                "broken, or none on record near the ring".to_owned(),
            );
            return None;
        };
        if self.end_portal.spawner_tries >= r.spawner_tries {
            self.end_portal.spawner_done = true;
            self.note(
                tick,
                format!("end portal: the room's spawner at {s:?} left"),
                format!("{} digs begun", self.end_portal.spawner_tries),
            );
            return None;
        }
        let mut cells = path::Cells::new(w);
        let over_lava = cells.at((feet.0, feet.1 - 1, feet.2)) == path::Class::Lava
            || cells.at(feet) == path::Class::Lava;
        let eye = [
            body_pos(w)[0],
            body_pos(w)[1] + physics::EYE_HEIGHT,
            body_pos(w)[2],
        ];
        let c = [
            f64::from(s.0) + 0.5,
            f64::from(s.1) + 0.5,
            f64::from(s.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        if d <= crate::walk::REACH && !over_lava {
            self.end_portal.spawner_tries += 1;
            self.note(
                tick,
                format!("end portal: break the spawner at {s:?}"),
                format!(
                    "from {feet:?}, {d:.1} off, pickaxes {}; dig {}",
                    pickaxes_held(w),
                    self.end_portal.spawner_tries
                ),
            );
            return Some(Task::Dig(crate::dig::Dig::new(w, s, tick)));
        }
        let mut walk = Walk::new(
            s,
            Goal::Near {
                pos: s,
                range: crate::walk::GOAL_RANGE,
            },
            Arrive::Reach,
            tick,
        );
        if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
            self.end_portal.spawner_tries += 1;
            self.walks += 1;
            self.note(
                tick,
                format!("end portal: walk to the spawner at {s:?}"),
                format!("from {feet:?}, {d:.1} off"),
            );
            return Some(Task::Explore(walk));
        }
        self.expanded += walk.expanded;
        self.end_portal.spawner_done = true;
        self.note(
            tick,
            format!("end portal: the room's spawner at {s:?} left"),
            "no route to within reach of it".to_owned(),
        );
        None
    }

    /// An eye into the first empty frame with a standing cell: from the feet when they reach, the
    /// eye held, `Act::UseOn` its top face; else a walk to the nearest standing cell.
    fn end_eyes(
        &mut self,
        w: &World,
        room: &Room,
        r: &EndKitRules,
        empty: &[[i32; 3]],
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        if empty.is_empty() || craft::count(w, ItemKind::EnderEye) == 0 {
            return None;
        }
        // A frame tried `eye_tries` times and still empty is skipped while others are left.
        let tried = |f: [i32; 3]| {
            self.end_portal
                .eye_tries
                .iter()
                .find(|(c, _, _)| *c == f)
                .map_or((0, 0), |(_, n, at)| (*n, *at))
        };
        let open: Vec<[i32; 3]> = empty
            .iter()
            .copied()
            .filter(|f| tried(*f).0 < r.eye_tries)
            .collect();
        let todo = if open.is_empty() {
            empty.to_vec()
        } else {
            open
        };
        let mut cells = path::Cells::new(w);
        let feet_ok = cells.at((feet.0, feet.1 - 1, feet.2)) != path::Class::Lava
            && cells.at(feet) != path::Class::Lava;
        let eye = [
            body_pos(w)[0],
            body_pos(w)[1] + physics::EYE_HEIGHT,
            body_pos(w)[2],
        ];
        // In reach from where the feet are.
        if let Some(f) = todo
            .iter()
            .copied()
            .find(|f| feet_ok && to_frame_top(eye, *f) <= r.frame_reach)
        {
            let (n, at) = tried(f);
            if n > 0 && tick < at + r.eye_every {
                return Some(Task::Idle);
            }
            if w.window.is_some() || w.click.is_some() || !self.acts.is_empty() {
                return Some(Task::Idle);
            }
            match crate::hold::toward(w, ItemKind::EnderEye) {
                crate::hold::Hold::Held => {
                    self.acts.push(Act::UseOn {
                        pos: BlockPos::new(f[0], f[1], f[2]),
                        face: 1,
                    });
                    match self
                        .end_portal
                        .eye_tries
                        .iter_mut()
                        .find(|(c, _, _)| *c == f)
                    {
                        Some(e) => {
                            e.1 += 1;
                            e.2 = tick;
                        }
                        None => self.end_portal.eye_tries.push((f, 1, tick)),
                    }
                    self.end_portal.eye_uses += 1;
                    self.note(
                        tick,
                        format!("end portal: eye into the frame at {f:?}"),
                        format!(
                            "from {feet:?}, {:.1} to its top; {} empty, {} eyes held; try {}",
                            to_frame_top(eye, f),
                            empty.len(),
                            craft::count(w, ItemKind::EnderEye),
                            n + 1
                        ),
                    );
                    self.retry_at = tick + r.eye_every;
                    return Some(Task::Idle);
                }
                crate::hold::Hold::Act(a) => {
                    self.acts.push(a);
                    return Some(Task::Idle);
                }
                crate::hold::Hold::Wait => return Some(Task::Idle),
                crate::hold::Hold::Missing => return None,
            }
        }
        // A walk to the nearest standing cell of the first frame that has one.
        for f in &todo {
            for stand in eye_stands(w, room, *f, feet, r.frame_reach)
                .into_iter()
                .take(crate::CANDIDATES_PER_TICK + 2)
            {
                if self.refused_now(stand, tick) {
                    continue;
                }
                let mut walk = Walk::new(stand, Goal::Block { pos: stand }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("end portal: walk to {stand:?} for the frame at {f:?}"),
                        format!("from {feet:?}; {} empty", empty.len()),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
                self.refuse(stand, tick);
            }
        }
        if !self.end_portal.lit_noted {
            self.note(
                tick,
                "end portal: no standing cell for the empty frames".to_owned(),
                format!("{todo:?} from {feet:?}"),
            );
        }
        None
    }
}

fn body_pos(w: &World) -> [f64; 3] {
    w.body.map_or([0.0; 3], |b| b.pos)
}
