//! Into the stronghold and its portal room (issue 19f, docs/problems/eyes-stronghold.md).
//!
//! The room is found by its frames: `end_portal_frame` blocks face the room's air, so memory
//! records them from chunk data within the view distance, as it did the fortress's nether bricks
//! 110 blocks off. From the frames' cells the ring's centre and the room's floor are computed
//! (`room`, pure), and the target is a standing cell on the floor inside the room, outside the
//! ring and away from the lava under it and in the room's corners (`stand_target`).
//!
//! The way in is dug, never walked through open caves: dry stairs down from the surface (the
//! staircase that turns from water beside, overworld-leg wall 7), out of the room's footprint,
//! to the floor's height; then the travel tunnel at the floor's level (`Plan::tunnel_toward`),
//! which breaks in through the room's wall; a short plain walk only at the floor's height within
//! `WALK_NEAR` of the target. No block a silverfish hides in is ever opened (`stairs::infested`:
//! the staircase refuses it and the tunnel's route takes it as a wall). In the room the spawner is
//! broken and the bot seals itself in a nook at the ring's side (issue 20 starts there).
//!
//! With no frame seen at the fix (the overland's arrival), dry stairs go down at the fix to the
//! stronghold's depth (`search_y`), then a square spiral of tunnel legs, toward stone bricks seen
//! exposed where there are any; memory is read again at every look.

use data::BlockKind;
use memory::Memory;
use world::{BlockPos, World};

use crate::path::{self, Cell, Goal};
use crate::stairs::{self, Stairs};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task};

/// The overworld's dimension name: frames are looked for there only.
pub const OVERWORLD: &str = "minecraft:overworld";

/// A plain walk to the target is taken only within this on the plane, at the floor's height: the
/// last steps through the hole the tunnel broke, never a walk across caves to the room.
pub const WALK_NEAR: f64 = 24.0;

/// The ring's shape (`EndPortalFrameBlock`'s portal pattern, the room's `PortalRoom` piece): three
/// frames a side, each side two from the centre on one axis.
pub const RING: i32 = 2;

/// The portal room as memory knows it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Room {
    /// The ring's centre, at the frames' height.
    pub centre: [i32; 3],
    /// The frame cells seen.
    pub frames: Vec<[i32; 3]>,
    /// The feet's height standing on the room's floor.
    pub floor_y: i32,
}

/// `[stronghold]` in `config/goals.toml`: every number has its source there.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StrongholdRules {
    pub frame_over_feet: i32,
    pub room_reach: i32,
    pub approach: i32,
    pub frame_chunks: i32,
    pub look_every: u64,
    pub search_y: i32,
    pub spiral_first: i32,
    pub spiral_step: i32,
    pub spiral_legs: u32,
    pub nook_max: u32,
    pub descent_stalls: u32,
    pub wall_breaks: u32,
    pub pick_tasks: u32,
}

impl Default for StrongholdRules {
    fn default() -> Self {
        Self {
            frame_over_feet: 2,
            room_reach: 7,
            approach: 10,
            frame_chunks: 8,
            look_every: 100,
            search_y: 30,
            spiral_first: 16,
            spiral_step: 8,
            spiral_legs: 12,
            nook_max: 24,
            descent_stalls: 4,
            wall_breaks: 4,
            pick_tasks: 8,
        }
    }
}

/// What the stronghold task holds between looks. Every field defaults.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StrongholdState {
    /// The room computed from the frames seen, and when memory was last read for them.
    pub room: Option<Room>,
    pub looked: Option<u64>,
    /// The overland's arrival with no frame seen: the fix's point on the plane, for the search.
    pub at_fix: Option<[i32; 2]>,
    /// Stairs legs begun, and the feet cell the last began from with legs begun from it since.
    pub legs: u32,
    pub leg_from: Option<(Cell, u32)>,
    /// Turns of the descent's heading after legs that went nowhere.
    pub turns: u32,
    /// The search's spiral legs begun (no frame seen).
    pub spiral: u32,
    pub spiral_target: Option<Cell>,
    /// The search reached its depth once (the stairs down are never begun again after it).
    pub descended: bool,
    /// Stone bricks broken in reach of the tunnel's end.
    pub wall_breaks: u32,
    /// Stone pickaxes being made (`stronghold_pickaxes`), tasks begun for them in this round,
    /// and the tick before which no round begins after one ended short.
    pub picking: bool,
    pub pick_tasks: u32,
    pub picks_retry_at: u64,
    /// In the room: the spawner broken (or none), the nook sealed, and blocks placed for it.
    pub spawner_done: bool,
    pub sealed: bool,
    pub nook_blocks: u32,
    pub nook_at: u64,
    /// Said once: the room found, the room entered, the room reached.
    pub found_noted: bool,
    pub entered_noted: bool,
    pub waited_noted: bool,
}

/// The ring's centre from frame cells seen (some or all of its twelve): the one cell every frame
/// is a ring member of (three frames a side, `RING` from the centre). With frames from more than
/// one ring or none consistent, the middle of their bounds. Pure.
pub fn ring_centre(frames: &[[i32; 3]]) -> Option<[i32; 3]> {
    let first = frames.first()?;
    let y = first[1];
    let members = |c: [i32; 2], f: [i32; 3]| {
        let (dx, dz) = ((f[0] - c[0]).abs(), (f[2] - c[1]).abs());
        (dx == RING && dz <= 1) || (dz == RING && dx <= 1)
    };
    let candidates: Vec<[i32; 2]> = (-RING..=RING)
        .flat_map(|dx| (-RING..=RING).map(move |dz| [first[0] + dx, first[2] + dz]))
        .filter(|c| members(*c, *first))
        .collect();
    let fit: Vec<[i32; 2]> = candidates
        .iter()
        .copied()
        .filter(|c| frames.iter().all(|f| members(*c, *f)))
        .collect();
    if let [c] = fit.as_slice() {
        return Some([c[0], y, c[1]]);
    }
    // Ambiguous (a few frames of one side) or inconsistent: the middle of the bounds, which is
    // the centre once two opposite sides are seen.
    let (x0, x1) = frames
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), f| (a.min(f[0]), b.max(f[0])));
    let (z0, z1) = frames
        .iter()
        .fold((i32::MAX, i32::MIN), |(a, b), f| (a.min(f[2]), b.max(f[2])));
    Some([(x0 + x1).div_euclid(2), y, (z0 + z1).div_euclid(2)])
}

/// The room from frame cells (the nearest ring's): its centre, frames and floor. Pure.
pub fn room_of(frames: &[[i32; 3]], frame_over_feet: i32) -> Option<Room> {
    let first = *frames.first()?;
    // One ring: the frames within reach of the first.
    let ring: Vec<[i32; 3]> = frames
        .iter()
        .copied()
        .filter(|f| {
            (f[0] - first[0]).abs() <= 2 * RING
                && (f[2] - first[2]).abs() <= 2 * RING
                && f[1] == first[1]
        })
        .collect();
    let centre = ring_centre(&ring)?;
    Some(Room {
        centre,
        frames: ring,
        floor_y: centre[1] - frame_over_feet,
    })
}

/// The portal room from every frame on record in the overworld (the agreed signature; a scan of
/// memory's latest sightings: the planner reads the frames near the feet instead, `frames_near`).
pub fn room(mem: &Memory) -> Option<Room> {
    let mut frames: Vec<[i32; 3]> = mem
        .latest_index()
        .iter()
        .filter(|((dim, _), _)| dim == OVERWORLD)
        .filter_map(|(_, i)| mem.log().get(*i))
        .filter(|s| data::block_of_state(s.state) == Some(BlockKind::EndPortalFrame))
        .map(|s| [s.pos.0, s.pos.1, s.pos.2])
        .collect();
    frames.sort_unstable();
    room_of(&frames, StrongholdRules::default().frame_over_feet)
}

/// Frames on record within `chunks` columns of the feet, nearest first.
pub fn frames_near(w: &World, chunks: i32) -> Vec<[i32; 3]> {
    if !in_overworld(w) {
        return Vec::new();
    }
    world::stage::kind_near(w, BlockKind::EndPortalFrame, chunks)
        .into_iter()
        .map(|c| [c.0, c.1, c.2])
        .collect()
}

fn in_overworld(w: &World) -> bool {
    w.dimension.as_ref().is_some_and(|d| d.name == OVERWORLD)
}

fn kind(w: &World, (x, y, z): Cell) -> Option<BlockKind> {
    w.block_at(BlockPos::new(x, y, z))
}

fn lava(k: Option<BlockKind>) -> bool {
    k == Some(BlockKind::Lava)
}

/// Is `feet` inside the room: within `reach` of the ring's centre on the plane, at the floor's
/// height up to the frames' and two over.
pub fn inside(room: &Room, feet: Cell, reach: i32) -> bool {
    let (dx, dz) = (feet.0 - room.centre[0], feet.2 - room.centre[2]);
    f64::from(dx).hypot(f64::from(dz)) <= f64::from(reach)
        && feet.1 >= room.floor_y
        && feet.1 <= room.centre[1] + 2
}

/// A standing cell in the room for the bot: on a floor, inside `reach` of the ring's centre and
/// outside the ring itself, with no lava within two cells at the feet's and the floor's height
/// (the pool under the ring, the room's corner pools), at the floor's height up to the frames';
/// the nearest `from` first. Only cells of the room's air and the blocks facing it are read.
pub fn stand_target(w: &World, room: &Room, from: Cell, reach: i32) -> Option<Cell> {
    let [cx, fy, cz] = room.centre;
    let mut cells = path::Cells::new(w);
    let mut best: Option<(i64, Cell)> = None;
    for y in room.floor_y..=fy {
        for dx in -reach..=reach {
            for dz in -reach..=reach {
                if dx.abs() <= RING + 1 && dz.abs() <= RING + 1 {
                    continue;
                }
                if f64::from(dx).hypot(f64::from(dz)) > f64::from(reach) {
                    continue;
                }
                let c = (cx + dx, y, cz + dz);
                if !cells.standable(c) {
                    continue;
                }
                let hot = (-2..=2).any(|ax| {
                    (-2..=2)
                        .any(|az| (-1..=0).any(|ay| lava(kind(w, (c.0 + ax, c.1 + ay, c.2 + az)))))
                });
                if hot {
                    continue;
                }
                // The floor's height first (the room's floor is lowest), then the nearest.
                let d = i64::from((c.0 - from.0).abs() + (c.2 - from.2).abs())
                    + 1000 * i64::from(y - room.floor_y);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, c));
                }
            }
        }
    }
    best.map(|(_, c)| c)
}

/// The compass axis nearest the direction from `a` to `b` on the plane (east on a tie).
pub fn axis_toward(a: Cell, b: Cell) -> (i32, i32) {
    let (dx, dz) = (b.0 - a.0, b.2 - a.2);
    if dx == 0 && dz == 0 {
        (1, 0)
    } else if dx.abs() >= dz.abs() {
        (dx.signum(), 0)
    } else {
        (0, dz.signum())
    }
}

fn quarter((dx, dz): (i32, i32)) -> (i32, i32) {
    (-dz, dx)
}

/// The descent's heading from `feet` toward a target at `target`, the room's centre `centre`:
/// toward the target while the stairs still end `approach` or more out from the centre (a stair
/// a block down is a block along), else away from the centre (never down through the room's
/// roof or over its lava). Pure.
pub fn descent_dir(feet: Cell, target: Cell, centre: Cell, approach: i32) -> (i32, i32) {
    let height = (feet.1 - target.1).max(0);
    let off = f64::from(feet.0 - centre.0).hypot(f64::from(feet.2 - centre.2));
    if off > f64::from(height + approach) {
        axis_toward(feet, target)
    } else {
        axis_toward(centre, feet)
    }
}

/// The search spiral's leg `n` from `centre` at height `y`: a square spiral east, south, west,
/// north, each pair of legs `step` longer. Pure.
pub fn spiral_point(centre: Cell, n: u32, first: i32, step: i32) -> Cell {
    let (mut x, mut z) = (centre.0, centre.2);
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    for k in 0..=n {
        let (dx, dz) = dirs[(k % 4) as usize];
        let len = first + step * i32::try_from(k / 2).unwrap_or(0);
        x += dx * len;
        z += dz * len;
    }
    (x, centre.1, z)
}

impl Plan {
    fn stronghold_rules(&self) -> StrongholdRules {
        self.table
            .as_ref()
            .map(|t| t.stronghold.clone())
            .unwrap_or_default()
    }

    /// The room known from memory, read again every `look_every` ticks (frames come into view as
    /// the bot nears); a new or moved ring is noted.
    fn stronghold_room(&mut self, w: &World, rules: &StrongholdRules, tick: u64) -> Option<Room> {
        let due = self
            .stronghold
            .looked
            .is_none_or(|t| tick.saturating_sub(t) >= rules.look_every);
        if due {
            self.stronghold.looked = Some(tick);
            let frames = frames_near(w, rules.frame_chunks);
            if let Some(room) = room_of(&frames, rules.frame_over_feet) {
                let moved = self
                    .stronghold
                    .room
                    .as_ref()
                    .is_none_or(|r| r.centre != room.centre);
                if moved {
                    self.note(
                        tick,
                        format!("portal room found: ring centre {:?}", room.centre),
                        format!(
                            "{} frames seen, floor feet y {}; {}",
                            room.frames.len(),
                            room.floor_y,
                            self.stronghold_state(w)
                        ),
                    );
                    self.stronghold.found_noted = true;
                    self.stronghold.room = Some(room);
                }
            }
        }
        self.stronghold.room.clone()
    }

    fn stronghold_state(&self, w: &World) -> String {
        let feet = w.body.map(|b| path::stand_cell(w, b.pos));
        format!(
            "feet {feet:?}, health {:?}, food {:?}, eyes {}, pickaxe uses {}",
            w.player.health,
            w.player.food,
            crate::craft::count(w, data::ItemKind::EnderEye),
            crate::endermen::pickaxe_uses_left(w)
        )
    }

    // >>> stronghold (Q4, issue 19f): the planner's one entry to the stronghold.
    /// The next step into the portal room: dry stairs down out of the room's footprint, the
    /// tunnel at the floor's level, a short walk in, then the spawner broken and a nook sealed at
    /// the ring's side (then `Task::Idle`: issue 20 starts there). With no frame seen and the
    /// overland arrived at the fix, the search: stairs down to `search_y` and a spiral of tunnel
    /// legs. `None` when neither a room nor an arrival is known (the overland's turn), or outside
    /// the overworld.
    pub fn stronghold_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        if !in_overworld(w) {
            return None;
        }
        let rules = self.stronghold_rules();
        let body = w.body?;
        let feet = path::stand_cell(w, body.pos);
        let Some(room) = self.stronghold_room(w, &rules, tick) else {
            return self.stronghold_search(w, &rules, feet, tick);
        };
        if inside(&room, feet, rules.room_reach) {
            return self.stronghold_inside(w, &room, &rules, feet, tick);
        }
        let centre = (room.centre[0], room.centre[1], room.centre[2]);
        let target = stand_target(w, &room, feet, rules.room_reach).unwrap_or_else(|| {
            // No standing cell seen yet: the floor beside the ring, on the bot's side.
            let (dx, dz) = axis_toward(centre, feet);
            (
                centre.0 + dx * (RING + 2),
                room.floor_y,
                centre.2 + dz * (RING + 2),
            )
        });
        // At the floor's height and near: the last steps by a plain walk on a found route.
        if (feet.1 - target.1).abs() <= 1 && path::octile(feet, target) <= WALK_NEAR {
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
                self.note(
                    tick,
                    format!("stronghold walk to {target:?}"),
                    format!(
                        "the room's floor, {} cells on a found route",
                        walk.waypoints.as_ref().map_or(0, Vec::len)
                    ),
                );
                return Some(Task::Explore(walk));
            }
        }
        // Over the floor's height: dry stairs down, out of the room's footprint.
        if feet.1 > target.1 + 1 {
            return Some(self.stronghold_stairs(w, feet, target, centre, &rules, tick, false));
        }
        // Under it (a stair overshot): stairs up.
        if feet.1 < target.1 - 1 {
            return Some(self.stronghold_stairs(w, feet, target, centre, &rules, tick, true));
        }
        // At the floor's level: the tunnel in, through the room's wall.
        let why = format!(
            "the portal room's floor at {target:?}, ring centre {:?}",
            room.centre
        );
        if let Some(t) = self.tunnel_toward(w, target, ("the portal room", &why), tick) {
            return Some(t);
        }
        self.note(
            tick,
            "stronghold tunnel has no way".to_owned(),
            format!("toward {target:?} from {feet:?}; waits a look"),
        );
        self.retry_at = tick + crate::RETRY_TICKS;
        Some(Task::Idle)
    }
    // <<< stronghold

    /// A leg of dry stairs (down, or up after an overshoot) toward the room: the heading out of
    /// its footprint (`descent_dir`), turned a quarter after a leg from the same cell went
    /// nowhere (`descent_stalls` of them in all end the stairs: the tunnel takes over).
    #[allow(clippy::too_many_arguments)]
    fn stronghold_stairs(
        &mut self,
        w: &World,
        feet: Cell,
        target: Cell,
        centre: Cell,
        rules: &StrongholdRules,
        tick: u64,
        up: bool,
    ) -> Task {
        let again = match self.stronghold.leg_from {
            Some((c, n)) if c == feet => n + 1,
            Some(_) | None => 1,
        };
        self.stronghold.leg_from = Some((feet, again));
        let mut dir = descent_dir(feet, target, centre, rules.approach);
        if again > 1 {
            self.stronghold.turns += 1;
            for _ in 1..again {
                dir = quarter(dir);
            }
            self.note(
                tick,
                format!("stronghold stairs turned to ({}, {})", dir.0, dir.1),
                format!("{} legs from {feet:?} went nowhere", again - 1),
            );
        }
        if again > rules.descent_stalls {
            let why = format!("stairs boxed at {feet:?}: the tunnel toward {target:?}");
            if let Some(t) = self.tunnel_toward(w, target, ("the portal room", &why), tick) {
                return t;
            }
            self.retry_at = tick + crate::RETRY_TICKS;
            return Task::Idle;
        }
        self.stronghold.legs += 1;
        let mut st = if up {
            Stairs::up(dir, tick)
        } else {
            Stairs::new(dir, tick)
        };
        st.dry = true;
        self.note(
            tick,
            format!(
                "stronghold stairs {} leg {} on ({}, {})",
                if up { "up" } else { "down" },
                self.stronghold.legs,
                dir.0,
                dir.1
            ),
            format!(
                "from {feet:?} to the floor's height {} at {target:?}, {:.0} off the ring's centre \
                 on the plane; {}",
                target.1,
                f64::from(feet.0 - centre.0).hypot(f64::from(feet.2 - centre.2)),
                self.stronghold_state(w)
            ),
        );
        Task::Stairs(Box::new(st))
    }

    /// In the room: the spawner broken, then the nook sealed, then the wait.
    fn stronghold_inside(
        &mut self,
        w: &World,
        room: &Room,
        rules: &StrongholdRules,
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        if !self.stronghold.entered_noted {
            self.stronghold.entered_noted = true;
            self.note(
                tick,
                "in the portal room".to_owned(),
                format!(
                    "ring centre {:?}, floor feet y {}; {}",
                    room.centre,
                    room.floor_y,
                    self.stronghold_state(w)
                ),
            );
        }
        let centre = (room.centre[0], room.centre[1], room.centre[2]);
        // The spawner: broken with the pickaxe once inside (it wakes silverfish within 16).
        if !self.stronghold.spawner_done {
            let spawner = world::stage::kind_near(w, BlockKind::Spawner, 1)
                .into_iter()
                .find(|s| path::octile(*s, centre) <= 12.0 && (s.1 - centre.1).abs() <= 4);
            match spawner {
                Some(s) if kind(w, s) == Some(BlockKind::Spawner) => {
                    let eye = [body_x(w), body_y(w) + physics::EYE_HEIGHT, body_z(w)];
                    let c = [
                        f64::from(s.0) + 0.5,
                        f64::from(s.1) + 0.5,
                        f64::from(s.2) + 0.5,
                    ];
                    let d = ((eye[0] - c[0]).powi(2)
                        + (eye[1] - c[1]).powi(2)
                        + (eye[2] - c[2]).powi(2))
                    .sqrt();
                    if d <= crate::walk::REACH {
                        self.note(
                            tick,
                            format!("break the room's spawner at {s:?}"),
                            "it wakes silverfish within 16; issue 20 lights the portal beside it"
                                .to_owned(),
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
                        self.walks += 1;
                        self.note(
                            tick,
                            format!("walk to the room's spawner at {s:?}"),
                            "to break it".to_owned(),
                        );
                        return Some(Task::Explore(walk));
                    }
                    self.stronghold.spawner_done = true;
                    self.note(
                        tick,
                        format!("the room's spawner at {s:?} left"),
                        "no route to within reach of it".to_owned(),
                    );
                }
                Some(_) | None => {
                    self.stronghold.spawner_done = true;
                    self.note(
                        tick,
                        "the room's spawner is gone".to_owned(),
                        "broken, or none on record near the ring".to_owned(),
                    );
                }
            }
        }
        // To the standing cell at the ring's side, then the nook.
        if !self.stronghold.sealed {
            if let Some(t) = stand_target(w, room, feet, rules.room_reach).filter(|t| *t != feet) {
                let mut walk = Walk::new(t, Goal::Block { pos: t }, Arrive::Goal, tick);
                if path::octile(feet, t) > 1.5
                    && walk.route_now(w, tick)
                    && walk.route == Some(path::Status::Found)
                {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("stand at the ring's side at {t:?}"),
                        "the floor clear of the lava, beside the ring".to_owned(),
                    );
                    return Some(Task::Explore(walk));
                }
            }
            return Some(self.stronghold_seal(w, rules, tick));
        }
        if !self.stronghold.waited_noted {
            self.stronghold.waited_noted = true;
            self.note(
                tick,
                "waits sealed in the portal room".to_owned(),
                format!("at {feet:?}; {}", self.stronghold_state(w)),
            );
        }
        Some(Task::Idle)
    }

    /// One block of the nook round the body (`cover::hut_block`), or its end noted.
    fn stronghold_seal(&mut self, w: &World, rules: &StrongholdRules, tick: u64) -> Task {
        let Some(body) = w.body else {
            return Task::Idle;
        };
        let next = crate::cover::hut_block(w, body.pos);
        let item = crate::SHELTER_BLOCKS
            .iter()
            .copied()
            .find(|k| crate::craft::count(w, *k) > 0);
        let end = match (item, next) {
            (_, None) => Some("nook sealed"),
            (None, Some(_)) => Some("no block held for the nook"),
            _ if self.stronghold.nook_blocks >= rules.nook_max => Some("nook given up"),
            _ => None,
        };
        if let Some(why) = end {
            self.stronghold.sealed = true;
            self.note(
                tick,
                format!("{why} ({} blocks placed)", self.stronghold.nook_blocks),
                "at the ring's side in the portal room".to_owned(),
            );
            return Task::Idle;
        }
        let Some((item, (cell, click, face))) = item.zip(next) else {
            return Task::Idle;
        };
        if w.window.is_some()
            || w.click.is_some()
            || !self.acts.is_empty()
            || tick < self.stronghold.nook_at
        {
            return Task::Idle;
        }
        match crate::hold::toward(w, item) {
            crate::hold::Hold::Held => {
                self.stronghold.nook_at = tick + crate::SHELTER_EVERY_TICKS;
                self.stronghold.nook_blocks += 1;
                self.acts.push(world::Act::UseOn {
                    pos: BlockPos::new(click.0, click.1, click.2),
                    face,
                });
                self.note(
                    tick,
                    format!("nook block {} at {cell:?}", self.stronghold.nook_blocks),
                    format!("on {click:?} face {face}"),
                );
            }
            crate::hold::Hold::Act(a) => self.acts.push(a),
            crate::hold::Hold::Wait => {}
            crate::hold::Hold::Missing => self.stronghold.sealed = true,
        }
        Task::Idle
    }

    /// No frame seen, the overland arrived at the fix: stairs down at the fix to `search_y`, then
    /// a spiral of tunnel legs toward stone bricks seen exposed, else the spiral's next point.
    fn stronghold_search(
        &mut self,
        w: &World,
        rules: &StrongholdRules,
        feet: Cell,
        tick: u64,
    ) -> Option<Task> {
        let [fx, fz] = self.stronghold.at_fix?;
        let centre = (fx, rules.search_y, fz);
        // Down once: a spiral leg's tunnel may climb a few blocks over the search's depth, and
        // stairs down again from there went down and up at the wall (race 45689919380 lane 0).
        if feet.1 <= rules.search_y + 1 && !self.stronghold.descended {
            self.stronghold.descended = true;
            self.note(
                tick,
                format!("stronghold search at depth y {}", feet.1),
                format!(
                    "the spiral and the bricks from here; {}",
                    self.stronghold_state(w)
                ),
            );
        }
        if !self.stronghold.descended {
            // A spiral down near the fix: each leg turns a quarter.
            let again = match self.stronghold.leg_from {
                Some((c, n)) if c == feet => n + 1,
                Some(_) | None => 1,
            };
            self.stronghold.leg_from = Some((feet, again));
            self.stronghold.legs += 1;
            let mut dir = axis_toward(feet, centre);
            for _ in 0..(self.stronghold.legs % 4) {
                dir = quarter(dir);
            }
            let mut st = Stairs::new(dir, tick);
            st.dry = true;
            self.note(
                tick,
                format!(
                    "stronghold search stairs down leg {} on ({}, {})",
                    self.stronghold.legs, dir.0, dir.1
                ),
                format!(
                    "no frame seen at the fix ({fx}, {fz}); down to y {}; {}",
                    rules.search_y,
                    self.stronghold_state(w)
                ),
            );
            return Some(Task::Stairs(Box::new(st)));
        }
        // Stone bricks seen exposed: the stronghold's walls; tunnel to the nearest.
        let bricks = crate::exposed_near(
            w,
            &[
                BlockKind::StoneBricks,
                BlockKind::MossyStoneBricks,
                BlockKind::CrackedStoneBricks,
            ],
        );
        let seen = bricks.len();
        if let Some(b) = bricks.into_iter().find(|b| !self.refused_now(*b, tick)) {
            let why = format!("stone bricks seen at {b:?}: the stronghold's wall");
            if let Some(t) = self.tunnel_toward(w, b, ("stone bricks", &why), tick) {
                return Some(t);
            }
            // The tunnel ends within reach of the wall and goes no further: the brick is broken
            // and the corridor behind it opened to the eye (race 45689919380 lane 0 stood at the
            // wall and turned away). At most `wall_breaks` of them, then the spiral.
            if self.stronghold.wall_breaks < rules.wall_breaks
                && !within_reach(w, b)
                && let Some(t) = self.wall_step(w, feet, b, tick)
            {
                return Some(t);
            }
            if self.stronghold.wall_breaks < rules.wall_breaks && within_reach(w, b) {
                self.stronghold.wall_breaks += 1;
                self.note(
                    tick,
                    format!(
                        "stronghold wall broken at {b:?} ({} of {})",
                        self.stronghold.wall_breaks, rules.wall_breaks
                    ),
                    format!(
                        "stone bricks in reach, the tunnel at its end; {}",
                        self.stronghold_state(w)
                    ),
                );
                return Some(Task::Dig(crate::dig::Dig::new(w, b, tick)));
            }
        }
        if self.stronghold.spiral >= rules.spiral_legs {
            self.note(
                tick,
                "stronghold search over".to_owned(),
                format!(
                    "{} spiral legs round ({fx}, {fz}) with no frame seen",
                    self.stronghold.spiral
                ),
            );
            self.stronghold.at_fix = None;
            return None;
        }
        let target = match self.stronghold.spiral_target {
            Some(t) if path::octile(feet, t) > 4.0 && !self.refused_now(t, tick) => t,
            Some(_) | None => {
                let n = self.stronghold.spiral;
                self.stronghold.spiral += 1;
                let t = spiral_point(centre, n, rules.spiral_first, rules.spiral_step);
                self.stronghold.spiral_target = Some(t);
                self.note(
                    tick,
                    format!("stronghold search leg {} toward {t:?}", n + 1),
                    format!(
                        "no frame seen; {seen} stone bricks seen exposed, none with a way or in \
                         reach ({} broken); {}",
                        self.stronghold.wall_breaks,
                        self.stronghold_state(w)
                    ),
                );
                t
            }
        };
        let why = "the stronghold search's spiral".to_owned();
        if let Some(t) = self.tunnel_toward(w, target, ("the search", &why), tick) {
            return Some(t);
        }
        self.stronghold.spiral_target = None;
        self.retry_at = tick + crate::RETRY_TICKS;
        Some(Task::Idle)
    }
}

impl Plan {
    /// Stone pickaxes made from the stairs' cobblestone, the pack's sticks and its table, where
    /// the bot stands, between the overland's and the stronghold's tasks: `spare_pickaxes` held
    /// with `spare_uses` or more each (overworld-leg wall 2's row, which `spares_wanted` closes
    /// once a portal is known). Race 45689919380 lane 0 came down the stairs to 13 uses on its one
    /// stone pickaxe with 67 cobblestone in the pack. The table is placed beside the feet and dug
    /// again once the round is done. `None` with nothing to do or nothing to make it from.
    pub fn stronghold_pickaxes(&mut self, w: &World, tick: u64) -> Option<Task> {
        let table = self.table.clone()?;
        let rules = &table.survival;
        let shrules = self.stronghold_rules();
        if rules.spare_pickaxes <= 0 || !in_overworld(w) {
            return None;
        }
        let fresh = crate::fresh_pickaxes(w, rules.spare_uses);
        let table_cell = self.at_hand(w, BlockKind::CraftingTable, tick);
        if fresh >= rules.spare_pickaxes {
            if !self.stronghold.picking {
                return None;
            }
            self.stronghold.picking = false;
            self.stronghold.pick_tasks = 0;
            self.note(
                tick,
                "stronghold pickaxes held".to_owned(),
                format!(
                    "{fresh} stone pickaxes with {} uses or more",
                    rules.spare_uses
                ),
            );
            return self.carry_table(w, tick);
        }
        if tick < self.stronghold.picks_retry_at {
            return None;
        }
        let end = |p: &mut Self, why: String| {
            p.stronghold.picking = false;
            p.stronghold.pick_tasks = 0;
            p.stronghold.picks_retry_at = tick + crate::SPARES_RETRY_TICKS;
            p.note(tick, "stronghold pickaxes not made".to_owned(), why);
        };
        if !crate::pickaxe_makeable(w, table_cell.is_some()) {
            if self.stronghold.picking {
                end(self, format!("{fresh} fresh held; no makings left"));
            }
            return None;
        }
        if self.stronghold.pick_tasks >= shrules.pick_tasks {
            end(
                self,
                format!("{} tasks begun this round", self.stronghold.pick_tasks),
            );
            return self.carry_table(w, tick);
        }
        if !self.stronghold.picking {
            self.stronghold.picking = true;
            self.note(
                tick,
                "stronghold pickaxes".to_owned(),
                format!(
                    "{fresh} stone pickaxes with {} uses or more, under {}: made here; {}",
                    rules.spare_uses,
                    rules.spare_pickaxes,
                    self.stronghold_state(w)
                ),
            );
        }
        // One more wanted: the row's one pickaxe counted from none held.
        let mut counts = crate::held(w);
        counts.insert(data::ItemKind::StonePickaxe, 0);
        let near_table = table_cell.is_some();
        let near = move |b: BlockKind| b == BlockKind::CraftingTable && near_table;
        let Some(goal) = table.until("stone-pickaxe") else {
            end(self, "no stone-pickaxe row".to_owned());
            return None;
        };
        let task = match crate::make::make(&table, &counts, &near, goal) {
            crate::make::Choice::Act(row) => match row.act {
                Some(crate::make::Act::Craft(r)) if !crate::make::fits_grid(r) => {
                    table_cell.map(|t| Task::Craft(crate::craft::Craft::at_table(r, t, tick)))
                }
                Some(crate::make::Act::Craft(r)) => {
                    Some(Task::Craft(crate::craft::Craft::new(r, tick)))
                }
                Some(crate::make::Act::Place(b)) => {
                    crate::place::Place::new(b, tick).map(Task::Place)
                }
                Some(_) | None => None,
            },
            crate::make::Choice::Done | crate::make::Choice::Stuck => None,
        };
        match task {
            Some(t) => {
                self.stronghold.pick_tasks += 1;
                self.note(
                    tick,
                    format!("stronghold pickaxes step {}", self.stronghold.pick_tasks),
                    match &t {
                        Task::Place(_) => "the table placed beside the feet".to_owned(),
                        _ => format!("a craft toward a stone pickaxe ({fresh} fresh held)"),
                    },
                );
                Some(t)
            }
            None => {
                end(
                    self,
                    format!("{fresh} fresh held; the row's next step is no craft here"),
                );
                self.carry_table(w, tick)
            }
        }
    }

    /// One step nearer a brick the tunnel ended short of (its reach is 5 to the cell, a dig's 4
    /// from the eye): the head's cell then the feet's on the axis toward it dug, then a walk into
    /// them. `None` at a liquid or a block a silverfish hides in (never opened).
    fn wall_step(&mut self, w: &World, feet: Cell, brick: Cell, tick: u64) -> Option<Task> {
        // Bricks under the feet (race 45689919380 lane 1: 5 under, at (1660, 22, -18), in the
        // tunnel's reach and out of a dig's): the block under the feet dug while the one under it
        // holds, a drop of one each time.
        let flat = (brick.0 - feet.0).abs().max((brick.2 - feet.2).abs());
        if brick.1 < feet.1 - 1 && flat <= 1 {
            let under = (feet.0, feet.1 - 1, feet.2);
            let below = (feet.0, feet.1 - 2, feet.2);
            let firm = |p: &Self, c: Cell| {
                let k = kind(w, c);
                k.is_some()
                    && !open(k)
                    && !lava(k)
                    && k != Some(BlockKind::Water)
                    && !stairs::infested(k)
                    && !p.refused_now(c, tick)
            };
            if !firm(self, under) || !(firm(self, below) || below == brick) {
                return None;
            }
            self.note(
                tick,
                format!("stronghold wall step down: dig {under:?}"),
                format!("stone bricks at {brick:?} under the feet {feet:?}, out of a dig's reach"),
            );
            return Some(Task::Dig(crate::dig::Dig::new(w, under, tick)));
        }
        let (dx, dz) = axis_toward(feet, brick);
        let next = (feet.0 + dx, feet.1, feet.2 + dz);
        for c in [(next.0, next.1 + 1, next.2), next] {
            let k = kind(w, c);
            if open(k) {
                continue;
            }
            if stairs::infested(k)
                || lava(k)
                || k == Some(BlockKind::Water)
                || k.is_none()
                || self.refused_now(c, tick)
            {
                return None;
            }
            self.note(
                tick,
                format!("stronghold wall step: dig {c:?}"),
                format!("toward stone bricks at {brick:?} out of a dig's reach from {feet:?}"),
            );
            return Some(Task::Dig(crate::dig::Dig::new(w, c, tick)));
        }
        let mut walk = Walk::new(next, Goal::Block { pos: next }, Arrive::Goal, tick);
        if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
            self.walks += 1;
            self.note(
                tick,
                format!("stronghold wall step: walk to {next:?}"),
                format!("toward stone bricks at {brick:?}"),
            );
            return Some(Task::Explore(walk));
        }
        None
    }
}

/// Air of any kind: nothing to dig.
fn open(k: Option<BlockKind>) -> bool {
    matches!(
        k,
        Some(BlockKind::Air | BlockKind::CaveAir | BlockKind::VoidAir)
    )
}

/// A block within a dig's reach of the eye.
fn within_reach(w: &World, (x, y, z): Cell) -> bool {
    let eye = [body_x(w), body_y(w) + physics::EYE_HEIGHT, body_z(w)];
    let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
    let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
    d <= crate::walk::REACH
}

fn body_x(w: &World) -> f64 {
    w.body.map_or(0.0, |b| b.pos[0])
}
fn body_y(w: &World) -> f64 {
    w.body.map_or(0.0, |b| b.pos[1])
}
fn body_z(w: &World) -> f64 {
    w.body.map_or(0.0, |b| b.pos[2])
}

/// Never dug: an infested block (`stairs::infested`), for the tests and the ledger.
pub fn never_dug(w: &World, c: Cell) -> bool {
    stairs::infested(kind(w, c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(c: [i32; 3]) -> Vec<[i32; 3]> {
        let mut v = Vec::new();
        for k in -1..=1 {
            v.push([c[0] + k, c[1], c[2] - 2]);
            v.push([c[0] + k, c[1], c[2] + 2]);
            v.push([c[0] - 2, c[1], c[2] + k]);
            v.push([c[0] + 2, c[1], c[2] + k]);
        }
        v
    }

    #[test]
    fn the_whole_ring_gives_its_centre() {
        let c = [100, 30, -40];
        assert_eq!(ring_centre(&ring(c)), Some(c));
        let r = room_of(&ring(c), 2).unwrap();
        assert_eq!(r.floor_y, 28);
        assert_eq!(r.frames.len(), 12);
    }

    #[test]
    fn two_sides_seen_give_the_centre() {
        let c = [7, 20, 9];
        let some: Vec<[i32; 3]> = ring(c)
            .into_iter()
            .filter(|f| f[2] == c[2] - 2 || f[0] == c[0] + 2)
            .collect();
        assert_eq!(ring_centre(&some), Some(c));
    }

    #[test]
    fn a_side_and_one_more_frame_fit_one_centre() {
        // The north side whole and one frame of the east side fit one centre.
        let c = [0, 10, 0];
        let some = vec![[-1, 10, -2], [0, 10, -2], [1, 10, -2], [2, 10, 0]];
        assert_eq!(ring_centre(&some), Some(c));
    }

    #[test]
    fn the_descent_heads_out_of_the_footprint_when_near() {
        // Far out: toward the target.
        assert_eq!(
            descent_dir((100, 70, 0), (5, 28, 0), (0, 30, 0), 10),
            (-1, 0)
        );
        // Over the room: away from the centre.
        assert_eq!(descent_dir((20, 70, 3), (5, 28, 0), (0, 30, 0), 10), (1, 0));
        assert_eq!(
            descent_dir((2, 70, -9), (5, 28, 0), (0, 30, 0), 10),
            (0, -1)
        );
    }

    #[test]
    fn the_spiral_widens() {
        let c = (0, 30, 0);
        assert_eq!(spiral_point(c, 0, 16, 8), (16, 30, 0));
        assert_eq!(spiral_point(c, 1, 16, 8), (16, 30, 16));
        assert_eq!(spiral_point(c, 2, 16, 8), (-8, 30, 16));
        assert_eq!(spiral_point(c, 3, 16, 8), (-8, 30, -8));
    }

    #[test]
    fn infested_kinds_are_never_dug() {
        assert!(stairs::infested(Some(BlockKind::InfestedStoneBricks)));
        assert!(stairs::infested(Some(BlockKind::InfestedDeepslate)));
        assert!(!stairs::infested(Some(BlockKind::StoneBricks)));
        assert!(!stairs::infested(None));
    }
}
