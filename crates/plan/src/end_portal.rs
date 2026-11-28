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

use data::{BlockKind, EntityKind, ItemKind};
use memory::Memory;
use world::{Act, BlockPos, World};

use crate::make::{self, Choice};
use crate::path::{self, Cell, Goal};
use crate::stronghold::{self, Room};
use crate::walk::{Arrive, Walk};
use crate::{Plan, Task, base, craft, eat};

/// The overworld's dimension name.
pub const OVERWORLD: &str = "minecraft:overworld";

/// A standing cell for an eye is chosen this far inside the reach: the body stands up to a
/// third of a block off its cell's centre.
pub const STAND_MARGIN: f64 = 0.4;

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
    /// The cook's own clock.
    pub cook_ticks: u64,
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
    // >>> E8 (issue 20, the bow line): the bow kit's targets (`[end_kit]` in config/goals.toml).
    /// The bow phase runs (the bow line); false skips it.
    pub bow: bool,
    /// Arrows wanted, and the least that counts as armed with a bow (`world::stage::BOW_ARROWS`).
    pub arrows: i32,
    /// Sticks and stone pickaxes held when the bow phase is over.
    pub bow_sticks: i32,
    pub bow_pickaxes: i32,
    /// The bow phase's own clock (a night is in it) and its failed tasks.
    pub bow_ticks: u64,
    pub bow_fails: u32,
    /// Chickens and spiders hunted within this of the feet.
    pub bow_radius: f64,
    /// The look points (x, z) for chickens and spiders, in turn (`wood_points` when empty).
    pub bow_points: Vec<[i32; 2]>,
    // <<< E8
    // >>> pumpkin (issue 80): the pumpkin phase's numbers (`[end_kit]` in config/goals.toml).
    /// The pumpkin phase runs, and the pumpkin is worn from the way down; false skips both.
    pub pumpkin: bool,
    /// The end-kit row makes the shears ahead of the iron boots (the 2 ingots kept for them).
    pub shears_first: bool,
    /// The phase's own clock and its failed tasks.
    pub pumpkin_ticks: u64,
    pub pumpkin_fails: u32,
    /// Pumpkins are looked for within this flat distance of the stairs' top, and this far over or
    /// under its height.
    pub pumpkin_radius: f64,
    pub pumpkin_rise: i32,
    /// Shears' UseOns on one pumpkin, and walks to it, before it is given up.
    pub pumpkin_tries: u32,
    pub pumpkin_walks: u32,
    /// The look points (x, z) walked to with no pumpkin seen (`bow_points` when empty).
    pub pumpkin_points: Vec<[i32; 2]>,
    /// In the dragon fight (`Task::Dragon`) the iron helmet goes back on over the pumpkin's place.
    pub dragon_helmet: bool,
    // <<< pumpkin
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
            cook_ticks: 9600,
            phase_fails: 16,
            down_walks: 8,
            frame_reach: 4.5,
            eye_tries: 4,
            eye_every: 20,
            spawner_tries: 6,
            entry_walks: 4,
            entry_clicks: 9,
            entries: 3,
            bow: false,
            arrows: 32,
            bow_sticks: 16,
            bow_pickaxes: 4,
            bow_ticks: 24_000,
            bow_fails: 24,
            bow_radius: 48.0,
            bow_points: Vec::new(),
            pumpkin: true,
            shears_first: true,
            pumpkin_ticks: 6000,
            pumpkin_fails: 16,
            pumpkin_radius: 150.0,
            pumpkin_rise: 16,
            pumpkin_tries: 4,
            pumpkin_walks: 4,
            pumpkin_points: Vec::new(),
            dragon_helmet: false,
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
    // >>> E8 (issue 20, the bow line): after the eat, before the way down.
    Bow,
    // <<< E8
    // >>> pumpkin (issue 80): after the bow, before the way down.
    Pumpkin,
    // <<< pumpkin
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
            KitPhase::Bow => "bow",
            KitPhase::Pumpkin => "pumpkin",
        }
    }

    fn next(self) -> KitPhase {
        match self {
            KitPhase::Up => KitPhase::Wood,
            KitPhase::Wood => KitPhase::Row,
            KitPhase::Row => KitPhase::Cook,
            KitPhase::Cook => KitPhase::Eat,
            KitPhase::Eat => KitPhase::Bow,
            KitPhase::Bow => KitPhase::Pumpkin,
            KitPhase::Pumpkin => KitPhase::Down,
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
    /// The way point walked to next, and whether a surface phase is climbing back up.
    pub way_at: Option<usize>,
    pub reclimb: bool,
    pub log_walks: u32,
    pub look_walks: u32,
    pub eat_tasks: u32,
    /// The wood point walked to next, and whether the overland's legs run for the wood now.
    pub wood_point: usize,
    #[serde(skip)]
    pub wood_legs: bool,
    pub down_walks: u32,
    pub down_noted: bool,
    /// The way point walked down to next, and plain walks back to the way from far off.
    pub down_at: Option<usize>,
    pub far_walks: u32,
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
    /// Pushes from a frame's top into the portal.
    pub pushes: u32,
    /// Frames whose way in was given up.
    pub entry_refused: Vec<[i32; 3]>,
    // >>> E8 (issue 20, the bow line)
    /// The look point walked to next for chickens and spiders, the legs retargeted, and the
    /// bow row's last give-up noted.
    pub bow_point: usize,
    pub bow_looks: u32,
    pub bow_row_said: Option<String>,
    /// In the portal room: the cobweb walks begun, and whether the room's bow step is over.
    pub web_tries: u32,
    pub room_bow_over: bool,
    // <<< E8
    // >>> pumpkin (issue 80)
    /// Pumpkins on record (memory's sightings: an exposed face), each noted once; the one gone
    /// for, the shears' UseOns and the walks spent on it.
    pub pumpkins_seen: Vec<Cell>,
    pub pumpkin_at: Option<Cell>,
    pub pumpkin_uses: u32,
    pub pumpkin_used_at: u64,
    pub pumpkin_walks: u32,
    pub pumpkin_point: usize,
    pub pumpkin_looks: u32,
    /// Noted once each: the shears made, the pumpkin carved, worn, and the iron short.
    pub shears_noted: bool,
    pub carved_noted: bool,
    pub worn_noted: bool,
    pub pumpkin_row_said: Option<String>,
    // <<< pumpkin
}

// >>> E8 (issue 20, the bow line)
/// The bow kit's goal row's id (built where the bot stands from what it holds).
pub const BOW_ROW: &str = "bow-kit";
/// The portal room's row: the bow alone.
pub const BOW_ROOM_ROW: &str = "bow-room";

/// A goal row's step: a task (or a wait), or the row over with why.
#[allow(clippy::large_enum_variant)]
pub enum RowStep {
    Task(Option<Task>),
    Over(String),
}

/// The bow kit's row for what is held now: the bow with 3 string held, as many arrows as the
/// feathers held make (one feather, flint and stick each 4, up to `arrows`), the stone pickaxes,
/// the sticks. Pure.
pub fn bow_row(
    r: &EndKitRules,
    have_bow: bool,
    arrows: i32,
    string: i32,
    feathers: i32,
) -> make::Row {
    let sets_short = ((r.arrows - arrows).max(0) + 3) / 4;
    let arrow_target = arrows + 4 * feathers.min(sets_short);
    let mut provides: Vec<(make::Need, i32)> = Vec::new();
    let mut push = |name: &str, k: i32| {
        if let Some(need) = make::Need::parse(name) {
            provides.push((need, k));
        }
    };
    if !have_bow && string >= 3 {
        push("item:bow", 1);
    }
    if arrow_target > arrows {
        push("item:arrow", arrow_target);
    }
    push("item:stone_pickaxe", r.bow_pickaxes);
    push("item:stick", r.bow_sticks);
    // Planks for a table where the bow is made (race 46072716511 lane 2 had 3 string in the room
    // and 2 planks: "no wood underground" for the table, no bow).
    push("tag:planks", 4);
    make::Row {
        id: BOW_ROW.to_owned(),
        needs: Vec::new(),
        provides,
        holds: Vec::new(),
        spares: Vec::new(),
        wants: Vec::new(),
        act: None,
    }
}

/// The bow kit in one line, for the notes.
pub fn bow_line(w: &World) -> String {
    format!(
        "bow {}, arrows {}, string {}, feathers {}, flint {}, gravel {}, sticks {}, stone pickaxes {}, logs {}, planks {}",
        craft::count(w, ItemKind::Bow),
        craft::count(w, ItemKind::Arrow),
        craft::count(w, ItemKind::String),
        craft::count(w, ItemKind::Feather),
        craft::count(w, ItemKind::Flint),
        craft::count(w, ItemKind::Gravel),
        craft::count(w, ItemKind::Stick),
        craft::count(w, ItemKind::StonePickaxe),
        base::logs_held(w),
        base::planks_held(w)
    )
}
// <<< E8

// >>> pumpkin (issue 80): the carved pumpkin (docs/problems/pumpkin.md).
/// The goal row the shears are made by, where the bot stands.
pub const SHEARS_ROW: &str = "pumpkin-shears";

/// The pumpkin's part of the kit line: the carved pumpkins and shears held, what the head wears.
pub fn pumpkin_line(w: &World) -> String {
    format!(
        "carved pumpkins {}, shears {}, head {:?}",
        craft::count(w, ItemKind::CarvedPumpkin),
        craft::count(w, ItemKind::Shears),
        w.inventory.slots.get(&5).map(|(k, _)| *k)
    )
}

/// The end-kit row with the shears put ahead of the iron boots, so the iron for them is kept
/// (the armed pack's 7 raw iron make the sword's 2 and the boots' 4 and leave 1). Pure.
pub fn shears_first(row: &make::Row) -> make::Row {
    let Some(shears) = make::Need::parse("item:shears") else {
        return row.clone();
    };
    let mut out = row.clone();
    if out.provides.iter().any(|(n, _)| *n == shears) {
        return out;
    }
    let boots = make::Need::parse("item:iron_boots");
    let at = out
        .provides
        .iter()
        .position(|(n, _)| Some(n) == boots.as_ref())
        .unwrap_or(out.provides.len());
    out.provides.insert(at, (shears, 1));
    out
}

/// The shears' row: one pair, from the iron held.
pub fn shears_row() -> make::Row {
    make::Row {
        id: SHEARS_ROW.to_owned(),
        needs: Vec::new(),
        provides: make::Need::parse("item:shears")
            .map(|n| vec![(n, 1)])
            .unwrap_or_default(),
        holds: Vec::new(),
        spares: Vec::new(),
        wants: Vec::new(),
        act: None,
    }
}

/// The six faces of a cell with the protocol's face numbers (0 down, 1 up, 2 north, 3 south,
/// 4 west, 5 east) and the neighbour each looks onto.
const FACES: [(i8, (i32, i32, i32)); 6] = [
    (0, (0, -1, 0)),
    (1, (0, 1, 0)),
    (2, (0, 0, -1)),
    (3, (0, 0, 1)),
    (4, (-1, 0, 0)),
    (5, (1, 0, 0)),
];

/// The faces of `c` open to sight (a loaded neighbour that is not a full block), nearest the
/// eye first, with the face's middle's distance from the eye.
pub fn open_faces(w: &World, c: Cell, eye: [f64; 3]) -> Vec<(i8, f64)> {
    let mut v: Vec<(i8, f64)> = FACES
        .iter()
        .filter(|(_, d)| {
            w.block_at(BlockPos::new(c.0 + d.0, c.1 + d.1, c.2 + d.2))
                .is_some_and(|k| {
                    data::generated::shapes::collision(k) != Some(data::types::Collision::Full)
                })
        })
        .map(|(f, d)| {
            let m = [
                f64::from(c.0) + 0.5 + 0.5 * f64::from(d.0),
                f64::from(c.1) + 0.5 + 0.5 * f64::from(d.1),
                f64::from(c.2) + 0.5 + 0.5 * f64::from(d.2),
            ];
            let dist = (0..3).map(|i| (m[i] - eye[i]).powi(2)).sum::<f64>().sqrt();
            (*f, dist)
        })
        .collect();
    v.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    v
}

// <<< pumpkin

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
        "pickaxes {}, iron swords {}, iron boots {}, logs {}, planks {}, sticks {}, raw iron {}, ingots {}, food {:?}, health {:?}, {}",
        pickaxes_held(w),
        craft::count(w, ItemKind::IronSword),
        craft::count(w, ItemKind::IronBoots),
        base::logs_held(w),
        base::planks_held(w),
        craft::count(w, ItemKind::Stick),
        craft::count(w, ItemKind::RawIron),
        craft::count(w, ItemKind::IronIngot),
        w.player.food,
        w.player.health,
        pumpkin_line(w)
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

/// The body stands on a frame's top (feet 0.6 to 1.0 over the frames' y) with a lit portal
/// block at the frames' height within one cell of the feet: the last step is a push in.
pub fn on_frame_by_portal(w: &World, room: &Room, pos: [f64; 3]) -> bool {
    let fy = room.centre[1];
    let up = pos[1] - f64::from(fy);
    if !(0.6..=1.0).contains(&up) {
        return false;
    }
    let (x, z) = (pos[0].floor() as i32, pos[2].floor() as i32);
    let under = |c: (i32, i32)| w.block_at(BlockPos::new(c.0, fy, c.1));
    let on_frame = (-1..=1).any(|dx| {
        (-1..=1).any(|dz| {
            let (cx, cz) = (x + dx, z + dz);
            under((cx, cz)) == Some(BlockKind::EndPortalFrame)
                && (pos[0] - f64::from(cx) - 0.5).abs() < 0.5 + physics::HALF_WIDTH
                && (pos[2] - f64::from(cz) - 0.5).abs() < 0.5 + physics::HALF_WIDTH
        })
    });
    let portal =
        (-1..=1).any(|dx| (-1..=1).any(|dz| under((x + dx, z + dz)) == Some(BlockKind::EndPortal)));
    on_frame && portal
}

/// The body over the lit end portal: an end portal block within one cell across of the feet and
/// up to three under them or one over. The pool under the portal's cells is lava two under the
/// body, never "at the door" (race 45887747475 lane 1: an escape began a tick before the body
/// went through).
pub fn over_portal(w: &World, pos: [f64; 3]) -> bool {
    let f = path::feet_cell(pos);
    (-1..=1).any(|dx| {
        (-1..=1).any(|dz| {
            (-3..=1).any(|dy| {
                w.block_at(BlockPos::new(f.0 + dx, f.1 + dy, f.2 + dz))
                    == Some(BlockKind::EndPortal)
            })
        })
    })
}

fn overworld(w: &World) -> bool {
    w.dimension.as_ref().is_some_and(|d| d.name == OVERWORLD)
}

fn flat(a: Cell, b: Cell) -> f64 {
    f64::from((a.0 - b.0).pow(2) + (a.2 - b.2).pow(2)).sqrt()
}

impl Plan {
    /// A dimension change: the task in hand ended and the traps' belief cleared, noted
    /// "dimension changed (task ended)". The first tick only records the dimension.
    pub(crate) fn end_dimension_watch(&mut self, w: &World, tick: u64) {
        let Some(now) = w.dimension.as_ref().map(|d| d.name.clone()) else {
            return;
        };
        let before = self.last_dimension.replace(now.clone());
        let Some(before) = before.filter(|b| *b != now) else {
            return;
        };
        let task = self.task.name();
        // An Enter ends itself in the new dimension (its own done check, and its after-step).
        if !matches!(self.task, Task::Enter(_)) {
            self.task = Task::Idle;
        }
        self.traps = crate::escape::Traps::default();
        self.retry_at = tick;
        self.note(
            tick,
            "dimension changed (task ended)".to_owned(),
            format!("{before} -> {now}; the task in hand was {task}"),
        );
    }

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
        // >>> E8 (issue 20, the bow line): in the stronghold with no bow, its cobwebs for string.
        if let Some(t) = self.bow_room_task(w, tick) {
            return Some(t);
        }
        // <<< E8
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
            // The cook's clock is its own: a furnace cooks a piece in 200 ticks, 30 pieces 6,000
            // (race 45880017656 lane 1 gave the cook up at 6,182 ticks with its last batch done).
            let clock = match phase {
                KitPhase::Cook => r.cook_ticks,
                KitPhase::Bow => r.bow_ticks,
                // >>> pumpkin (issue 80)
                KitPhase::Pumpkin => r.pumpkin_ticks,
                // <<< pumpkin
                _ => r.phase_ticks,
            };
            let fails_max = match phase {
                KitPhase::Bow => r.bow_fails,
                KitPhase::Pumpkin => r.pumpkin_fails,
                _ => r.phase_fails,
            };
            if phase != KitPhase::Up && phase != KitPhase::Down {
                if spent > clock {
                    let why = format!("gave up: {spent} ticks of {clock}");
                    self.pumpkin_given_up(w, phase, &why, tick);
                    self.end_kit_advance(w, tick, &why);
                    continue;
                }
                if self.end_portal.fails >= fails_max {
                    let fails = self.end_portal.fails;
                    let why = format!("gave up: {fails} tasks failed");
                    self.pumpkin_given_up(w, phase, &why, tick);
                    self.end_kit_advance(w, tick, &why);
                    continue;
                }
            }
            let next = match phase {
                KitPhase::Up => self.end_kit_up(w, &r, tick),
                KitPhase::Wood
                | KitPhase::Row
                | KitPhase::Cook
                | KitPhase::Eat
                | KitPhase::Bow
                | KitPhase::Pumpkin => self.end_kit_surface(w, &r, phase, tick),
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
        self.end_kit_climb(w, r, tick)
    }

    /// The climb up the bot's own way from where the feet are (the up phase's, and the surface
    /// phases' when a fight's retreat took the bot back down its stairs).
    fn end_kit_climb(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
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
            if near(feet, p) <= 3.0 {
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
            if i > 0 && p.1 > feet.1 && way.get(i - 1).is_some_and(|q| near(feet, *q) <= 3.0) {
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
        // Back down its own stairs (race 45880017656 lane 0: a creeper's retreat took the bot
        // from the trees to (1597, 47, -38), and the wood's legs then tunnelled by hand): up again
        // first, by the same way.
        let feet = path::feet_cell(w.body?.pos);
        if feet.1 < r.way_up[1] - 4 && !world::stage::open_sky(w) {
            if !self.end_portal.reclimb {
                self.end_portal.reclimb = true;
                self.end_portal.way_at = None;
                self.end_portal.up_walks = 0;
                self.note(
                    tick,
                    format!(
                        "end kit: under the surface at {feet:?} in the {} phase",
                        phase.name()
                    ),
                    "up its own way again first".to_owned(),
                );
            }
            return self.end_kit_climb(w, r, tick);
        }
        self.end_portal.reclimb = false;
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
            KitPhase::Bow => self.end_kit_bow(w, r, night, tick),
            // >>> pumpkin (issue 80)
            KitPhase::Pumpkin => self.end_kit_pumpkin(w, r, night, tick),
            // <<< pumpkin
            KitPhase::Up | KitPhase::Down | KitPhase::Over => None,
        }
    }

    // >>> E8 (issue 20, the bow line)
    /// In the portal room with the kit's phases over and no bow (string is scarce by day: race
    /// 46053975527 lane 0 had 24 arrows and 1 string after a whole day): the stronghold's
    /// cobwebs with a face open to the air, within `WEB_REACH`, walked to and dug (string, the
    /// jar's loot); with 3 string the bow made through the bow row. Over at `WEB_TRIES` walks,
    /// none in reach, or the bow held.
    fn bow_room_task(&mut self, w: &World, tick: u64) -> Option<Task> {
        const WEB_REACH: i32 = 40;
        const WEB_RISE: i32 = 12;
        const WEB_TRIES: u32 = 48;
        let r = self.end_rules();
        if !r.bow || self.end_portal.room_bow_over || !overworld(w) {
            return None;
        }
        let n = |k: ItemKind| craft::count(w, k);
        if n(ItemKind::Bow) >= 1 {
            self.room_bow_done(w, tick, "the bow held");
            return None;
        }
        if n(ItemKind::String) >= 3 {
            // The bow alone: no pickaxe or sticks wanted down here, so no trip for wood (race
            // 46067009450 lane 2 walked off for logs from the stronghold with 3 string).
            let row = make::Row {
                id: BOW_ROOM_ROW.to_owned(),
                needs: Vec::new(),
                provides: make::Need::parse("item:bow")
                    .map(|need| vec![(need, 1)])
                    .unwrap_or_default(),
                holds: Vec::new(),
                spares: Vec::new(),
                wants: Vec::new(),
                act: None,
            };
            return match self.kit_row_step(w, &r, &row, tick) {
                RowStep::Task(t) => Some(t.unwrap_or(Task::Idle)),
                RowStep::Over(why) => {
                    self.room_bow_done(w, tick, &format!("the room's bow over ({why})"));
                    None
                }
            };
        }
        if self.end_portal.web_tries >= WEB_TRIES {
            self.room_bow_done(w, tick, "no more cobwebs tried");
            return None;
        }
        let feet = path::feet_cell(w.body?.pos);
        let mut best: Option<(i32, Cell)> = None;
        for dx in -WEB_REACH..=WEB_REACH {
            for dz in -WEB_REACH..=WEB_REACH {
                for dy in -WEB_RISE..=WEB_RISE {
                    let c = (feet.0 + dx, feet.1 + dy, feet.2 + dz);
                    if w.block_at(BlockPos::new(c.0, c.1, c.2)) != Some(BlockKind::Cobweb) {
                        continue;
                    }
                    // No X-ray: a face open to the air.
                    let open = [
                        (1, 0, 0),
                        (-1, 0, 0),
                        (0, 1, 0),
                        (0, -1, 0),
                        (0, 0, 1),
                        (0, 0, -1),
                    ]
                    .iter()
                    .any(|(a, b, d)| {
                        matches!(
                            w.block_at(BlockPos::new(c.0 + a, c.1 + b, c.2 + d)),
                            Some(BlockKind::Air | BlockKind::CaveAir)
                        )
                    });
                    if !open || self.refused_now(c, tick) {
                        continue;
                    }
                    let d = dx.abs() + dz.abs() + 2 * dy.abs();
                    if best.is_none_or(|(bd, bc)| d < bd || (d == bd && c < bc)) {
                        best = Some((d, c));
                    }
                }
            }
        }
        let Some((_, c)) = best else {
            self.room_bow_done(
                w,
                tick,
                &format!("no cobweb within {WEB_REACH} of {feet:?}"),
            );
            return None;
        };
        self.end_portal.web_tries += 1;
        let mut walk = Walk::to_dig(c, tick);
        if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
            self.walks += 1;
            let line = bow_line(w);
            self.note(
                tick,
                format!("end kit: cobweb at {c:?} for string"),
                format!("try {} of {WEB_TRIES}; {line}", self.end_portal.web_tries),
            );
            return Some(Task::ToLog(walk));
        }
        self.expanded += walk.expanded;
        // No walk (race 46060386538 lane 1: sixteen webs in reach, none walked to): dug toward
        // through the stronghold's walls, a step at a time, the walk asked again after each.
        let why = format!("no walk to it (route {:?})", walk.route);
        if let Some(t) = self.dig_toward(w, c, true, ("the cobweb", &why), tick) {
            let line = bow_line(w);
            self.note(
                tick,
                format!("end kit: dig toward the cobweb at {c:?}"),
                format!(
                    "{why}; try {} of {WEB_TRIES}; {line}",
                    self.end_portal.web_tries
                ),
            );
            return Some(t);
        }
        self.note(
            tick,
            format!("end kit: cobweb at {c:?} refused"),
            format!(
                "{why}, no dig toward it; try {} of {WEB_TRIES}",
                self.end_portal.web_tries
            ),
        );
        self.refuse(c, tick);
        Some(Task::Idle)
    }

    /// The room's bow step over, noted; after a trip to the cobwebs the way down runs again to
    /// the room's standing cell (race 46067009450 lane 0 made its bow 16 off in its tunnel, and
    /// the frames had no standing cell from there).
    fn room_bow_done(&mut self, w: &World, tick: u64, why: &str) {
        self.end_portal.room_bow_over = true;
        let line = bow_line(w);
        self.note(tick, format!("end kit: room bow step over ({why})"), line);
        if self.end_portal.web_tries > 0 {
            self.end_portal.phase = KitPhase::Down;
            self.end_portal.since = tick;
            self.end_portal.down_at = None;
            self.end_portal.down_noted = false;
            self.note(
                tick,
                "end kit: back to the room after the cobwebs".to_owned(),
                format!("{} cobweb tries", self.end_portal.web_tries),
            );
        }
    }

    /// The bow kit (docs/problems/end-bow.md): by day, what can be made is made (the bow from 3
    /// string, arrows from the feathers held with flint redug from the gravel held, the spare
    /// pickaxe, sticks); then chickens hunted for feathers and spiders for string, seen within
    /// `bow_radius` (the jar's loot: 0 to 2 each); with none in sight, the overland's legs to the
    /// look points (`wood_points`) in turn. Over once held, or on its clock.
    fn end_kit_bow(&mut self, w: &World, r: &EndKitRules, night: bool, tick: u64) -> Option<Task> {
        if !r.bow {
            self.end_kit_advance(w, tick, "no bow on this line");
            return None;
        }
        let n = |k: ItemKind| craft::count(w, k);
        let have_bow = n(ItemKind::Bow) >= 1;
        let arrows = n(ItemKind::Arrow);
        let pickaxes = n(ItemKind::StonePickaxe);
        let sticks = n(ItemKind::Stick);
        if have_bow && arrows >= r.arrows && sticks >= r.bow_sticks && pickaxes >= r.bow_pickaxes {
            let line = bow_line(w);
            self.end_kit_advance(w, tick, &format!("the bow kit held: {line}"));
            return None;
        }
        if night {
            self.retry_at = tick + crate::RETRY_TICKS * 10;
            return Some(Task::Idle);
        }
        if let Some(t) = self.end_hut_exit(w, None, tick) {
            return Some(t);
        }
        // What can be made now, from what is held.
        let sets_short = ((r.arrows - arrows).max(0) + 3) / 4;
        let feathers = n(ItemKind::Feather);
        let string = n(ItemKind::String);
        let row = bow_row(r, have_bow, arrows, string, feathers);
        match self.kit_row_step(w, r, &row, tick) {
            RowStep::Task(Some(t)) => return Some(t),
            RowStep::Task(None) => {
                self.retry_at = tick + crate::RETRY_TICKS;
                return Some(Task::Idle);
            }
            RowStep::Over(why) => {
                if self.end_portal.bow_row_said.as_deref() != Some(why.as_str()) {
                    let line = bow_line(w);
                    self.note(tick, format!("end kit: bow row over ({why})"), line);
                    self.end_portal.bow_row_said = Some(why);
                }
            }
        }
        let want_feathers = feathers < sets_short;
        let want_string = !have_bow && string < 3;
        if !want_feathers && !want_string {
            let line = bow_line(w);
            self.end_kit_advance(w, tick, &format!("the bow kit as far as it goes: {line}"));
            return None;
        }
        let body = w.body?;
        // Feathers or string lying in sight (a defend's kill drops them and moves on: race
        // 46047707279 lane 2 killed a spider in a defend and walked off): picked up first.
        let wanted: Vec<ItemKind> = [
            (want_feathers, ItemKind::Feather),
            (want_string, ItemKind::String),
        ]
        .into_iter()
        .filter(|x| x.0)
        .map(|x| x.1)
        .collect();
        let lying = w
            .entities
            .values()
            .filter(|e| !e.dead && e.item().is_some_and(|(k, _)| wanted.contains(&k)))
            .map(|e| e.pos)
            .filter(|p| {
                (0..3)
                    .map(|i| (p[i] - body.pos[i]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    <= 24.0
            })
            .min_by(|a, b| {
                let d = |p: &[f64; 3]| (0..3).map(|i| (p[i] - body.pos[i]).powi(2)).sum::<f64>();
                d(a).total_cmp(&d(b))
            });
        if let Some(p) = lying {
            let c = path::feet_cell(p);
            if !self.refused_now(c, tick) {
                let mut walk = Walk::new(c, Goal::Near { pos: c, range: 0.8 }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    let line = bow_line(w);
                    self.note(
                        tick,
                        format!("end kit: pick up the bow's drop at {c:?}"),
                        line,
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
                self.refuse(c, tick);
            }
        }
        // The nearest chicken or spider seen, with a walk to it.
        let kinds: Vec<(EntityKind, &str)> = [
            (want_feathers, EntityKind::Chicken, "feathers"),
            (want_string, EntityKind::Spider, "string"),
            (want_string, EntityKind::CaveSpider, "string"),
        ]
        .into_iter()
        .filter(|x| x.0)
        .map(|x| (x.1, x.2))
        .collect();
        let d = |p: [f64; 3]| {
            (0..3)
                .map(|i| (p[i] - body.pos[i]).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let mut prey: Vec<(f64, i32, EntityKind, &str)> = w
            .entities
            .iter()
            .filter(|(id, e)| !e.dead && !self.refused_prey.contains(id))
            .filter_map(|(id, e)| {
                kinds
                    .iter()
                    .find(|k| k.0 == e.kind)
                    .map(|k| (d(e.pos), *id, e.kind, k.1))
            })
            .filter(|x| x.0 <= r.bow_radius)
            .collect();
        prey.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (dist, id, kind, item) in prey.into_iter().take(crate::hunt::CANDIDATES) {
            let mut h = crate::hunt::Hunt::new(w, id, kind, tick);
            h.extra = vec![ItemKind::Feather, ItemKind::String];
            if h.reachable(w, tick) {
                let line = bow_line(w);
                self.note(
                    tick,
                    format!("end kit: hunt {kind:?} {id} for {item}"),
                    format!("{dist:.0} off; {line}"),
                );
                return Some(Task::Hunt(Box::new(h)));
            }
            self.refused_prey.push(id);
        }
        // None in sight: the overland's legs to the next look point.
        let points = if r.bow_points.is_empty() {
            &r.wood_points
        } else {
            &r.bow_points
        };
        let Some(p) = points.get(self.end_portal.bow_point).copied() else {
            let line = bow_line(w);
            self.end_kit_advance(
                w,
                tick,
                &format!("gave up: no chicken or spider found at the look points; {line}"),
            );
            return None;
        };
        let p = [f64::from(p[0]), f64::from(p[1])];
        let here = [body.pos[0], body.pos[2]];
        let dd = (p[0] - here[0]).hypot(p[1] - here[1]);
        if dd <= r.point_reach || self.overland.arrived || self.end_portal.bow_looks >= r.look_walks
        {
            self.end_portal.bow_point += 1;
            self.end_portal.bow_looks = 0;
            self.overland.arrived = false;
            self.note(
                tick,
                format!(
                    "end kit: bow look point {} reached, nothing to hunt",
                    self.end_portal.bow_point - 1
                ),
                format!("{dd:.0} off; on to the next"),
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
            self.end_portal.bow_looks += 1;
            let line = bow_line(w);
            self.note(
                tick,
                format!(
                    "end kit: legs toward bow look point {}",
                    self.end_portal.bow_point
                ),
                format!("({:.0}, {:.0}), {dd:.0} off; {line}", p[0], p[1]),
            );
        }
        self.end_portal.wood_legs = true;
        let t = self.overland_task(w, tick);
        self.end_portal.wood_legs = false;
        if let Some(t) = t {
            return Some(t);
        }
        self.overland.arrived = true;
        Some(Task::Idle)
    }
    // <<< E8

    // >>> pumpkin (issue 80): the carved pumpkin (docs/problems/pumpkin.md).
    /// The pumpkin leg: from the kit's way down (the pumpkin never goes on before `armed`: the
    /// pearl hunt stares on purpose) through the stronghold and the End.
    pub fn pumpkin_leg(&self, w: &World) -> bool {
        crate::end::in_end(w)
            || (overworld(w)
                && self.end_portal.begun
                && matches!(self.end_portal.phase, KitPhase::Down | KitPhase::Over))
    }

    /// The pumpkin wanted on the head now: on the leg with one held, and not the dragon fight
    /// when `dragon_helmet` puts the helmet back for it.
    fn pumpkin_wanted(&self, w: &World, r: &EndKitRules) -> bool {
        r.pumpkin
            && self.pumpkin_leg(w)
            && craft::count(w, ItemKind::CarvedPumpkin) >= 1
            && !(r.dragon_helmet && matches!(self.task, Task::Dragon(_)))
    }

    /// The one head-slot rule: with the pumpkin wanted, the iron helmet stays off (held), so
    /// `equip` and the Nether's armour branch never put it back over the pumpkin's place.
    pub fn helmet_off(&self, w: &World) -> bool {
        self.pumpkin_wanted(w, &self.end_rules())
    }

    /// The head's next click, on `equip`'s clock (true when one was sent): the helmet off and
    /// the pumpkin on, two shift-clicks: a carved pumpkin's quick move goes into slot 5 only when
    /// it is empty, and with the helmet there falls through to a main-hotbar move (its
    /// `equippable` is head, not swappable; probe 46159976076, brain/game/mc-pumpkin.md). Or the
    /// pumpkin off for the dragon fight when the data says so.
    pub fn pumpkin_head(&mut self, w: &World, tick: u64) -> bool {
        let r = self.end_rules();
        if !r.pumpkin || w.click.is_some() || w.inventory.carried.is_some() {
            return false;
        }
        let head = w.inventory.slots.get(&5).map(|(k, _)| *k);
        let wanted = self.pumpkin_wanted(w, &r);
        let click = |slot: i16| Act::Click {
            slot,
            button: 0,
            mode: 1,
        };
        let free = (9i16..=44).any(|s| !w.inventory.slots.contains_key(&s));
        if !wanted {
            // The dragon fight with `dragon_helmet`: the pumpkin comes off; `equip` then puts
            // the helmet on.
            if head == Some(ItemKind::CarvedPumpkin)
                && r.dragon_helmet
                && matches!(self.task, Task::Dragon(_))
                && free
            {
                self.equip_at = tick + crate::EQUIP_TICKS;
                self.acts.push(click(5));
                self.end_portal.worn_noted = false;
                let line = pumpkin_line(w);
                self.note(
                    tick,
                    "pumpkin off for the dragon fight (helmet next)".to_owned(),
                    format!("[end_kit] dragon_helmet; {line}"),
                );
                return true;
            }
            return false;
        }
        match head {
            Some(ItemKind::CarvedPumpkin) => {
                if !self.end_portal.worn_noted {
                    self.end_portal.worn_noted = true;
                    let line = pumpkin_line(w);
                    self.note(
                        tick,
                        "pumpkin worn (helmet held)".to_owned(),
                        format!(
                            "{} iron helmets held; no stare provokes an enderman now; {line}",
                            craft::count(w, ItemKind::IronHelmet)
                        ),
                    );
                }
                false
            }
            Some(k) => {
                if !free {
                    return false;
                }
                self.equip_at = tick + crate::EQUIP_TICKS;
                self.acts.push(click(5));
                let line = pumpkin_line(w);
                self.note(
                    tick,
                    format!("{k:?} off the head for the pumpkin"),
                    format!("the pumpkin leg begun; {line}"),
                );
                true
            }
            None => {
                let Some(slot) = w
                    .inventory
                    .slots
                    .iter()
                    .find(|(s, (k, _))| **s >= 9 && *k == ItemKind::CarvedPumpkin)
                    .map(|(s, _)| *s)
                else {
                    return false;
                };
                self.equip_at = tick + crate::EQUIP_TICKS;
                self.acts.push(click(slot));
                let line = pumpkin_line(w);
                self.note(
                    tick,
                    format!("pumpkin on from slot {slot}"),
                    format!("the head empty, the pumpkin leg begun; {line}"),
                );
                true
            }
        }
    }

    /// The pumpkin phase given up on its clock or its failures: noted as the pumpkin's.
    fn pumpkin_given_up(&mut self, w: &World, phase: KitPhase, why: &str, tick: u64) {
        if phase != KitPhase::Pumpkin {
            return;
        }
        let line = pumpkin_line(w);
        self.note(
            tick,
            format!("pumpkin given up: {why}"),
            format!("the kit goes on without one; {line}"),
        );
    }

    /// The pumpkins on record in the columns round the feet (`SEARCH_COLUMNS`), their last
    /// sighting a pumpkin, within `pumpkin_radius` flat and `pumpkin_rise` of the stairs' top.
    fn pumpkins_on_record(&self, w: &World, r: &EndKitRules) -> Vec<Cell> {
        let (Some(body), Some(dim)) = (w.body, w.dimension.as_ref()) else {
            return Vec::new();
        };
        let feet = path::feet_cell(body.pos);
        let top = (r.way_up[0], r.way_up[1], r.way_up[2]);
        let (cx, cz) = (feet.0 >> 4, feet.2 >> 4);
        let mut v: Vec<Cell> = Vec::new();
        for dx in -crate::SEARCH_COLUMNS..=crate::SEARCH_COLUMNS {
            for dz in -crate::SEARCH_COLUMNS..=crate::SEARCH_COLUMNS {
                for s in w.memory.in_column(&dim.name, cx + dx, cz + dz) {
                    if data::block_of_state(s.state) == Some(BlockKind::Pumpkin)
                        && flat(s.pos, top) <= r.pumpkin_radius
                        && (s.pos.1 - top.1).abs() <= r.pumpkin_rise
                    {
                        v.push(s.pos);
                    }
                }
            }
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// The phase over without a carved pumpkin, noted, the kit going on.
    fn pumpkin_give_up(&mut self, w: &World, why: &str, tick: u64) {
        self.pumpkin_given_up(w, KitPhase::Pumpkin, why, tick);
        self.end_kit_advance(w, tick, &format!("no pumpkin: {why}"));
    }

    /// The pumpkin phase: a pumpkin seen with an exposed face within `pumpkin_radius` of the
    /// stairs' top, walked to, the shears used on it where it stands (made from the iron held when
    /// none are, never a trip for iron), the carved pumpkin dug and picked up. With none seen,
    /// the look points; over at the carved pumpkin held, or given up (noted) on its clock.
    fn end_kit_pumpkin(
        &mut self,
        w: &World,
        r: &EndKitRules,
        night: bool,
        tick: u64,
    ) -> Option<Task> {
        if !r.pumpkin {
            self.end_kit_advance(w, tick, "no pumpkin on this line");
            return None;
        }
        if craft::count(w, ItemKind::Shears) >= 1 && !self.end_portal.shears_noted {
            self.end_portal.shears_noted = true;
            let line = pumpkin_line(w);
            self.note(tick, "shears made".to_owned(), line);
        }
        if craft::count(w, ItemKind::CarvedPumpkin) >= 1 {
            let line = pumpkin_line(w);
            self.note(tick, "carved pumpkin picked up".to_owned(), line.clone());
            self.end_kit_advance(w, tick, &format!("the carved pumpkin held: {line}"));
            return None;
        }
        if night {
            self.retry_at = tick + crate::RETRY_TICKS * 10;
            return Some(Task::Idle);
        }
        if let Some(t) = self.end_hut_exit(w, None, tick) {
            return Some(t);
        }
        let body = w.body?;
        let feet = path::feet_cell(body.pos);
        let top = (r.way_up[0], r.way_up[1], r.way_up[2]);
        // Memory's pumpkins (sightings need an exposed face: strict no X-ray) within the radius
        // of the stairs' top, each noted once.
        for c in self.pumpkins_on_record(w, r) {
            if self.end_portal.pumpkins_seen.contains(&c) {
                continue;
            }
            self.end_portal.pumpkins_seen.push(c);
            self.note(
                tick,
                format!("pumpkin seen at {c:?}"),
                format!(
                    "{:.0} off the feet {feet:?}, {:.0} off the stairs' top; {} seen",
                    flat(c, feet),
                    flat(c, top),
                    self.end_portal.pumpkins_seen.len()
                ),
            );
        }
        // A carved pumpkin lying in sight (a dig's drop not yet taken): picked up.
        let lying = w
            .entities
            .values()
            .filter(|e| !e.dead && e.item().is_some_and(|(k, _)| k == ItemKind::CarvedPumpkin))
            .map(|e| e.pos)
            .filter(|p| flat(path::feet_cell(*p), feet) <= 24.0)
            .min_by(|a, b| {
                flat(path::feet_cell(*a), feet).total_cmp(&flat(path::feet_cell(*b), feet))
            });
        if let Some(p) = lying {
            let c = path::feet_cell(p);
            if !self.refused_now(c, tick) {
                let mut walk = Walk::new(c, Goal::Near { pos: c, range: 0.8 }, Arrive::Goal, tick);
                if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("end kit: pick up the carved pumpkin at {c:?}"),
                        pumpkin_line(w),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
                self.refuse(c, tick);
            }
        }
        // The pumpkin gone for: kept while it stands, else the nearest seen and not refused.
        if let Some(c) = self.end_portal.pumpkin_at {
            let k = w.block_at(BlockPos::new(c.0, c.1, c.2));
            let gone = !matches!(
                k,
                Some(BlockKind::Pumpkin | BlockKind::CarvedPumpkin) | None
            );
            if gone || self.refused_now(c, tick) {
                self.end_portal.pumpkin_at = None;
                self.end_portal.pumpkins_seen.retain(|x| *x != c);
                self.note(
                    tick,
                    format!("end kit: the pumpkin at {c:?} left"),
                    format!("now {k:?}"),
                );
            }
        }
        if self.end_portal.pumpkin_at.is_none() {
            let seen = self.end_portal.pumpkins_seen.clone();
            let best = seen
                .into_iter()
                .filter(|c| !self.refused_now(*c, tick))
                .min_by(|a, b| flat(*a, feet).total_cmp(&flat(*b, feet)).then(a.cmp(b)));
            if let Some(c) = best {
                self.end_portal.pumpkin_at = Some(c);
                self.end_portal.pumpkin_uses = 0;
                self.end_portal.pumpkin_walks = 0;
                self.end_portal.carved_noted = false;
                self.note(
                    tick,
                    format!("end kit: for the pumpkin at {c:?}"),
                    format!(
                        "{:.0} off the feet {feet:?}; {}",
                        flat(c, feet),
                        pumpkin_line(w)
                    ),
                );
            }
        }
        let Some(c) = self.end_portal.pumpkin_at else {
            return self.pumpkin_look(w, r, tick);
        };
        if w.block_at(BlockPos::new(c.0, c.1, c.2)) == Some(BlockKind::CarvedPumpkin) {
            if !self.end_portal.carved_noted {
                self.end_portal.carved_noted = true;
                self.note(
                    tick,
                    format!("pumpkin carved at {c:?}"),
                    format!(
                        "after {} shears' uses; {}",
                        self.end_portal.pumpkin_uses,
                        pumpkin_line(w)
                    ),
                );
            }
            self.walks += 1;
            self.note(
                tick,
                format!("end kit: dig the carved pumpkin at {c:?}"),
                format!("from {feet:?}; {}", pumpkin_line(w)),
            );
            return Some(Task::ToLog(Walk::to_dig(c, tick)));
        }
        // Shears first, from the iron held: never a trip for iron.
        if craft::count(w, ItemKind::Shears) == 0 {
            let iron = craft::count(w, ItemKind::IronIngot) + craft::count(w, ItemKind::RawIron);
            if iron < 2 {
                self.pumpkin_give_up(w, &format!("{iron} iron held, the shears want 2"), tick);
                return None;
            }
            return match self.kit_row_step(w, r, &shears_row(), tick) {
                RowStep::Task(t) => Some(t.unwrap_or(Task::Idle)),
                RowStep::Over(why) => {
                    if craft::count(w, ItemKind::Shears) == 0 {
                        self.pumpkin_give_up(w, &format!("no shears ({why})"), tick);
                        return None;
                    }
                    Some(Task::Idle)
                }
            };
        }
        if self.end_portal.pumpkin_uses >= r.pumpkin_tries
            || self.end_portal.pumpkin_walks >= r.pumpkin_walks
        {
            self.note(
                tick,
                format!("end kit: the pumpkin at {c:?} refused"),
                format!(
                    "{} uses, {} walks",
                    self.end_portal.pumpkin_uses, self.end_portal.pumpkin_walks
                ),
            );
            self.refuse(c, tick);
            self.end_portal.pumpkin_at = None;
            return Some(Task::Idle);
        }
        let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
        // In reach: the shears on its open face nearest the eye (the server takes a UseOn within
        // the block interaction range, `frame_reach`).
        if let Some((face, d)) = open_faces(w, c, eye).first().copied()
            && d <= r.frame_reach
        {
            if self.end_portal.pumpkin_uses > 0
                && tick < self.end_portal.pumpkin_used_at + r.eye_every
            {
                return Some(Task::Idle);
            }
            if w.window.is_some() || w.click.is_some() || !self.acts.is_empty() {
                return Some(Task::Idle);
            }
            return match crate::hold::toward(w, ItemKind::Shears) {
                crate::hold::Hold::Held => {
                    self.acts.push(Act::UseOn {
                        pos: BlockPos::new(c.0, c.1, c.2),
                        face,
                    });
                    self.end_portal.pumpkin_uses += 1;
                    self.end_portal.pumpkin_used_at = tick;
                    self.note(
                        tick,
                        format!("end kit: shears on the pumpkin at {c:?}"),
                        format!(
                            "face {face}, {d:.1} from the eye; use {} of {}",
                            self.end_portal.pumpkin_uses, r.pumpkin_tries
                        ),
                    );
                    self.retry_at = tick + r.eye_every;
                    Some(Task::Idle)
                }
                crate::hold::Hold::Act(a) => {
                    self.acts.push(a);
                    Some(Task::Idle)
                }
                crate::hold::Hold::Wait => Some(Task::Idle),
                crate::hold::Hold::Missing => {
                    self.pumpkin_give_up(w, "the shears not in the inventory", tick);
                    None
                }
            };
        }
        // Out of reach: a walk to beside it.
        self.end_portal.pumpkin_walks += 1;
        let mut walk = Walk::to_dig(c, tick);
        if walk.route_now(w, tick) && walk.route == Some(path::Status::Found) {
            self.walks += 1;
            self.note(
                tick,
                format!("end kit: walk to the pumpkin at {c:?}"),
                format!(
                    "from {feet:?}, {:.0} off; walk {} of {}",
                    flat(c, feet),
                    self.end_portal.pumpkin_walks,
                    r.pumpkin_walks
                ),
            );
            return Some(Task::Explore(walk));
        }
        self.expanded += walk.expanded;
        self.note(
            tick,
            format!("end kit: no walk to the pumpkin at {c:?}"),
            format!("from {feet:?}, route {:?}", walk.route),
        );
        self.refuse(c, tick);
        self.end_portal.pumpkin_at = None;
        Some(Task::Idle)
    }

    /// No pumpkin seen: the overland's legs to the next look point (`pumpkin_points`, else the
    /// bow's); over (noted) when the points are spent.
    fn pumpkin_look(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        let points = if r.pumpkin_points.is_empty() {
            &r.bow_points
        } else {
            &r.pumpkin_points
        };
        let Some(p) = points.get(self.end_portal.pumpkin_point).copied() else {
            let n = points.len();
            self.pumpkin_give_up(w, &format!("none seen at {n} look points"), tick);
            return None;
        };
        let body = w.body?;
        let p = [f64::from(p[0]), f64::from(p[1])];
        let dd = (p[0] - body.pos[0]).hypot(p[1] - body.pos[2]);
        if dd <= r.point_reach
            || self.overland.arrived
            || self.end_portal.pumpkin_looks >= r.look_walks
        {
            self.end_portal.pumpkin_point += 1;
            self.end_portal.pumpkin_looks = 0;
            self.overland.arrived = false;
            self.note(
                tick,
                format!(
                    "end kit: pumpkin look point {} reached, none seen",
                    self.end_portal.pumpkin_point - 1
                ),
                format!("{dd:.0} off; on to the next"),
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
            self.end_portal.pumpkin_looks += 1;
            self.note(
                tick,
                format!(
                    "end kit: legs toward pumpkin look point {}",
                    self.end_portal.pumpkin_point
                ),
                format!("({:.0}, {:.0}), {dd:.0} off", p[0], p[1]),
            );
        }
        self.end_portal.wood_legs = true;
        let t = self.overland_task(w, tick);
        self.end_portal.wood_legs = false;
        if let Some(t) = t {
            return Some(t);
        }
        self.overland.arrived = true;
        Some(Task::Idle)
    }
    // <<< pumpkin

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
        let Some(mut row) = table.until("end-kit").cloned() else {
            self.end_kit_advance(w, tick, "no end-kit row in the goal table");
            return None;
        };
        // >>> pumpkin (issue 80): the shears' 2 ingots kept ahead of the boots.
        if r.pumpkin && r.shears_first {
            row = shears_first(&row);
        }
        // <<< pumpkin
        match self.kit_row_step(w, r, &row, tick) {
            RowStep::Task(t) => t,
            RowStep::Over(why) => {
                self.end_kit_advance(w, tick, &why);
                None
            }
        }
    }

    // >>> E8 (issue 20, the bow line): the row's step, shared by the end-kit row and the bow's.
    /// One act of a goal row from the needs planner, where the bot stands: a craft (at the table
    /// at hand), a placing, a smelt at the furnace at hand, or a log gathered; `Over` with why
    /// when the row is met or cannot go on here.
    fn kit_row_step(&mut self, w: &World, r: &EndKitRules, row: &make::Row, tick: u64) -> RowStep {
        let Some(table) = self.table.clone() else {
            return RowStep::Over("no goal table".to_owned());
        };
        let label = row.id.clone();
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
            Choice::Done => return RowStep::Over(format!("the {label} row is met")),
            Choice::Stuck => {
                return RowStep::Over(format!("gave up: the {label} row is not makeable"));
            }
            Choice::Act(a) => a,
        };
        let id = act.id.clone();
        let why = format!("the {label} row's next step ({id})");
        let t = match act.act {
            Some(make::Act::Craft(rec)) if !make::fits_grid(rec) => {
                let Some(t) = table_cell else {
                    return RowStep::Over(format!("gave up: {id} wants a table at hand"));
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
                let p = crate::place::Place::new(b, tick);
                if p.is_some() {
                    self.note(tick, format!("end kit: place {b:?}"), why);
                }
                p.map(Task::Place)
            }
            Some(make::Act::Smelt(s)) => {
                let Some(f) = furnace_cell else {
                    return RowStep::Over(format!("gave up: {id} wants a furnace at hand"));
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
                            return RowStep::Task(Some(Task::Craft(crate::craft::Craft::new(
                                rec, tick,
                            ))));
                        }
                    }
                }
                let sm = crate::smelt::Smelt::new(w, f, s, count, tick);
                if sm.is_some() {
                    self.note(
                        tick,
                        format!("end kit: smelt {count} for {} at {f:?}", s.id),
                        why,
                    );
                }
                sm.map(|sm| Task::Smelt(Box::new(sm)))
            }
            Some(make::Act::Gather) if label == BOW_ROOM_ROW => {
                return RowStep::Over("no wood underground".to_owned());
            }
            Some(make::Act::Gather) => {
                // Wood short for the row: one more log by hand.
                let before = self.end_portal.phase;
                let t = self.end_kit_wood(w, r, tick);
                if self.end_portal.phase != before {
                    // The wood gave out: the row is over with what is held.
                    self.end_portal.phase = before;
                    return RowStep::Over("gave up: no wood for the row".to_owned());
                }
                t
            }
            // The bow's flint: a held gravel placed and dug again until it drops flint (the
            // `flint-from-gravel` row; the jar's gravel loot, one time in ten).
            Some(make::Act::Redig(b)) if label == BOW_ROW => {
                let rd = crate::redig::Redig::new(w, b, tick);
                if rd.is_some() {
                    self.note(tick, format!("end kit: redig {b:?} for flint"), why);
                }
                rd.map(|x| Task::Redig(Box::new(x)))
            }
            Some(make::Act::Mine { .. } | make::Act::Redig(_) | make::Act::Scoop) | None => {
                return RowStep::Over(format!("gave up: {id} would be a trip"));
            }
        };
        RowStep::Task(t)
    }
    // <<< E8

    /// Down the same stairs to the room's standing cell; with no walk, the stronghold's way in.
    fn end_kit_down(&mut self, w: &World, r: &EndKitRules, tick: u64) -> Option<Task> {
        let rules = self.stronghold_rules();
        let feet = path::stand_cell(w, w.body?.pos);
        let frames = stronghold::frames_near(w, rules.frame_chunks);
        let Some(room) = stronghold::room_of(&frames, rules.frame_over_feet) else {
            // >>> E8 (issue 20, the bow line): far off its stairs (the bow's look points, race
            // 46053975527 lane 1 at (1522, 65, -32), 86 off), the frames are out of the search:
            // the overland's legs back to the stairs' top first.
            let top = [f64::from(r.way_up[0]) + 0.5, f64::from(r.way_up[2]) + 0.5];
            let pos = w.body?.pos;
            let off = (pos[0] - top[0]).hypot(pos[2] - top[1]);
            if off > 48.0 {
                if self
                    .overland
                    .fix
                    .is_none_or(|(q, _)| (q[0] - top[0]).hypot(q[1] - top[1]) > 4.0)
                {
                    self.overland.retarget((top, 0.0), true);
                    self.overland.arrived = false;
                    self.note(
                        tick,
                        "end kit: legs back to the stairs' top".to_owned(),
                        format!("{off:.0} off it and no frame in the search"),
                    );
                }
                self.end_portal.wood_legs = true;
                let t = self.overland_task(w, tick);
                self.end_portal.wood_legs = false;
                if let Some(t) = t {
                    return Some(t);
                }
            }
            // <<< E8
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
            // Far from the way (a wood walk ended out on the plain): the overland's legs, not
            // counted as the point's walks (race 45880017656 lane 1: its night hut's blocks, a
            // task a call, spent every point's walks in 60 ticks).
            // A plain walk first, found or partial: the overland's legs build a hut at night and
            // wait it out on the surface (race 45891071513: all three lanes huted 60 off the
            // stairs' top from tick 1,020 to the cap), and the way down is underground.
            if flat(feet, p) > 40.0 && self.end_portal.far_walks < r.down_walks {
                let mut walk = Walk::new(p, Goal::Near { pos: p, range: 1.5 }, Arrive::Goal, tick);
                let routed = walk.route_now(w, tick);
                self.end_portal.far_walks += 1;
                if routed
                    && matches!(
                        walk.route,
                        Some(path::Status::Found | path::Status::Partial)
                    )
                {
                    self.walks += 1;
                    self.note(
                        tick,
                        format!("end kit: walk back to point {i} at {p:?}"),
                        format!(
                            "from {feet:?}, a {:?} route, walk {}",
                            walk.route, self.end_portal.far_walks
                        ),
                    );
                    return Some(Task::Explore(walk));
                }
                self.expanded += walk.expanded;
            }
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
            self.end_portal.down_walks += 1;
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
        // On a frame's top beside the lit portal: plain forward toward the ring's centre, no
        // path and no goal to meet on the way (the coordinator, after two bots stood at the edge
        // with their boxes on the frame: races 45877404722 and 45884543903).
        if let Some(body) = w.body
            && on_frame_by_portal(w, room, body.pos)
            && self.end_portal.pushes < 4 * r.entries
        {
            self.end_portal.pushes += 1;
            let c = (room.centre[0], room.centre[1], room.centre[2]);
            let mut walk = Walk::new(c, Goal::Block { pos: c }, Arrive::Goal, tick);
            walk.waypoints = Some(Vec::new());
            walk.route = Some(path::Status::Found);
            self.walks += 1;
            self.note(
                tick,
                format!(
                    "end portal: into the portal by push ({})",
                    self.end_portal.pushes
                ),
                format!(
                    "from ({:.2}, {:.2}, {:.2}) on a frame's top, forward toward {c:?}",
                    body.pos[0], body.pos[1], body.pos[2]
                ),
            );
            return Some(Task::Enter(Box::new(crate::Enter {
                walk,
                cell: c,
                inside_since: None,
                from: Some(OVERWORLD.to_owned()),
            })));
        }
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
        // The goal is the cell past the portal cell (race 45884543903 lane 0: a goal of the portal
        // cell itself was met by the stand cell with the body at z -32.0 still on the frame's
        // top, the walk done and the step in "TargetLost", three times). Already up at the
        // frame's height, only the last step.
        let deeper = e.deeper();
        let mut walk = Walk::new(deeper, Goal::Block { pos: deeper }, Arrive::Goal, tick);
        let up_already = feet.1 >= e.frame[1] && flat(feet, e.inside) <= 1.5;
        if up_already {
            walk.waypoints = Some(vec![e.inside, deeper]);
            walk.more.kinds = vec![path::MoveKind::Walk, path::MoveKind::Walk];
        } else {
            walk.waypoints = Some(e.way());
            walk.more.kinds = vec![
                path::MoveKind::JumpUp,
                path::MoveKind::JumpUp,
                path::MoveKind::JumpUp,
                path::MoveKind::Walk,
                path::MoveKind::Walk,
            ];
        }
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
            cell: deeper,
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
            // Stands judged from the cell's centre a margin inside the reach, and never the feet's
            // own cell (race 45886255606 lane 1: (1629, 17, -37) was in reach of (1626, 19, -34)
            // from its centre, not from the body off it, and the walk there was begun 3,267 times
            // standing on it).
            for stand in eye_stands(w, room, *f, feet, r.frame_reach - STAND_MARGIN)
                .into_iter()
                .take(crate::CANDIDATES_PER_TICK + 2)
            {
                if self.refused_now(stand, tick) || stand == feet {
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
