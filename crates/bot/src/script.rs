//! Scripted controls: a test harness until the planner (issue 09). A script starts once the
//! bot's start has settled (its kit or place applied, 20 server ticks after), so a real run
//! and a model run of the same moment press the same keys from the same position. Since issue
//! 10b a script also acts (digs, clicks), for the parity table's dig and craft rows.

use physics::Controls;
use world::{Act, DigStep, World};

/// What a craft script makes in window 0, from what its moment gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Craft {
    /// One log (given into hotbar slot 36) into the grid; shift-click the planks out.
    Planks,
    /// Two planks, one each into grid slots 1 and 3 (a column); shift-click the sticks out.
    Sticks,
    /// Four planks, one into each grid slot; shift-click the table out.
    Table,
    /// At a placed crafting table (issue 11a): the table placed on the block east of the feet,
    /// opened, three planks and two sticks laid as a wooden pickaxe, taken, the table closed.
    /// Given: the table (hotbar 0), 3 planks, 2 sticks.
    WoodenPickaxe,
    /// The same with 5 cobblestone and 3 sticks: a stone pickaxe, then a stone sword.
    StoneTools,
}

impl Craft {
    /// The clicks, in order: (slot, button, mode). Each changes the window, so each is
    /// answered (a click that changes nothing draws no reply).
    fn clicks(self) -> &'static [(i16, i8, i32)] {
        match self {
            Craft::Planks => &[(36, 0, 0), (1, 0, 0), (0, 0, 1)],
            Craft::Sticks => &[(36, 0, 0), (1, 1, 0), (3, 1, 0), (0, 0, 1)],
            Craft::Table => &[
                (36, 0, 0),
                (1, 1, 0),
                (2, 1, 0),
                (3, 1, 0),
                (4, 1, 0),
                (0, 0, 1),
            ],
            // The table's menu: the inventory at 10-45, so window 0's 37 and 38 are 38 and 39.
            Craft::WoodenPickaxe => &[
                (38, 0, 0),
                (1, 1, 0),
                (2, 1, 0),
                (3, 1, 0),
                (39, 0, 0),
                (5, 1, 0),
                (8, 1, 0),
                (0, 0, 1),
            ],
            Craft::StoneTools => &[
                (38, 0, 0),
                (1, 1, 0),
                (2, 1, 0),
                (3, 1, 0),
                (38, 0, 0),
                (39, 0, 0),
                (5, 1, 0),
                (8, 1, 0),
                (39, 0, 0),
                (0, 0, 1),
                // The sword: the last two cobblestone and the last stick, so the cursor ends
                // empty and nothing is put back (a click that changes nothing draws no reply).
                (38, 0, 0),
                (2, 1, 0),
                (5, 1, 0),
                (39, 0, 0),
                (8, 1, 0),
                (0, 0, 1),
            ],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Craft::Planks => "planks",
            Craft::Sticks => "sticks",
            Craft::Table => "table",
            Craft::WoodenPickaxe => "wooden_pickaxe",
            Craft::StoneTools => "stone_tools",
        }
    }

    /// Whether it is made at a placed crafting table.
    fn at_table(self) -> bool {
        matches!(self, Craft::WoodenPickaxe | Craft::StoneTools)
    }
}

/// What a burrow script does (issue 18's parity rows e04 to e08, docs/problems/pearls-model.md).
/// The burrow runs east: the back wall west of B, the mouth M east of B, O east of M. The phase
/// is read from the world each tick, so a script needs no memory: the feet are in B when the
/// block west of them is solid (the back wall), else in M.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum BurrowMode {
    /// From M, stare at the nearest enderman's eye until one is creepy (entity data 17), then
    /// walk west into B; in B, look at the nearest enderman's torso (feet + 1.2), never its eye.
    Hold,
    /// As `Hold`, and in B strike the nearest enderman on a full charge (an iron sword's 1.6
    /// attacks a second: 12.5 ticks; every 14th tick) while its box is within 3.0 of the eye:
    /// the attack, then the swing.
    Strike,
    /// Look at the nearest enderman's eye, wherever the feet are; never step.
    Eye,
    /// From M, stare until one is creepy, then stay in M looking at its torso.
    Near,
}

impl BurrowMode {
    fn name(self) -> &'static str {
        match self {
            BurrowMode::Hold => "hold",
            BurrowMode::Strike => "strike",
            BurrowMode::Eye => "eye",
            BurrowMode::Near => "near",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "hold" => Some(BurrowMode::Hold),
            "strike" => Some(BurrowMode::Strike),
            "eye" => Some(BurrowMode::Eye),
            "near" => Some(BurrowMode::Near),
            _ => None,
        }
    }
}

/// An enderman's eye and torso over its feet (`EntityType.ENDERMAN`: eye 2.55; the torso the
/// plan's feet + 1.2), its half width and height (0.3, 2.9).
const ENDER_EYE: f64 = 2.55;
const ENDER_TORSO: f64 = 1.2;
const ENDER_HALF: f64 = 0.3;
const ENDER_HEIGHT: f64 = 2.9;
/// The player's strike reach, eye to box (`Player.entityInteractionRange` 3.0).
const STRIKE_REACH: f64 = 3.0;
/// Ticks between strikes: an iron sword's attack speed 1.6 charges in 12.5 ticks; 14 is full.
const STRIKE_EVERY: u64 = 14;
/// Endermen farther than this are not the script's (48: the look-out rows place one 33 beyond a
/// spot 10 out).
const ENDER_NEAR: f64 = 48.0;
/// `EnderMan.DATA_CREEPY`.
const CREEPY: u8 = 17;

/// Whether nothing solid the world knows lies on the line from `a` to `b` (sampled every 0.2).
fn clear(w: &World, a: [f64; 3], b: [f64; 3]) -> bool {
    let len = (0..3).map(|i| (b[i] - a[i]).powi(2)).sum::<f64>().sqrt();
    let steps = (len / 0.2).ceil().max(1.0) as u32;
    (1..steps).all(|s| {
        let t = f64::from(s) / f64::from(steps);
        let p = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t);
        w.block_at(world::BlockPos::new(
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        ))
        .is_none_or(|k| k == data::BlockKind::Air || k == data::BlockKind::CaveAir)
    })
}

/// The nearest live enderman within `ENDER_NEAR` of the feet that the eye sees (its eye or its
/// torso in a clear line: another row's enderman behind a wall is not this row's): its id and
/// feet.
fn nearest_ender(w: &World, feet: [f64; 3]) -> Option<(i32, [f64; 3])> {
    let d = |p: [f64; 3]| (0..3).map(|i| (p[i] - feet[i]).powi(2)).sum::<f64>();
    let eye = [feet[0], feet[1] + physics::EYE_HEIGHT, feet[2]];
    w.entities
        .iter()
        .filter(|(_, e)| !e.dead && e.kind == data::EntityKind::Enderman)
        .map(|(id, e)| (*id, e.pos))
        .filter(|(_, p)| d(*p) <= ENDER_NEAR * ENDER_NEAR)
        .filter(|(_, p)| {
            [ENDER_EYE, ENDER_TORSO]
                .iter()
                .any(|h| clear(w, eye, [p[0], p[1] + h, p[2]]))
        })
        .min_by(|a, b| d(a.1).total_cmp(&d(b.1)))
}

/// Whether the enderman `id` is creepy.
fn creepy_of(w: &World, id: i32) -> bool {
    w.entities.get(&id).is_some_and(|e| {
        matches!(
            e.metadata.get(&CREEPY),
            Some(protocol::metadata::MetaValue::Bool(true))
        )
    })
}

/// The yaw and pitch from `eye` to `at` (yaw 0 south, 90 west; pitch down positive).
fn look_at(eye: [f64; 3], at: [f64; 3]) -> (f64, f32) {
    let (dx, dy, dz) = (at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]);
    let yaw = (-dx).atan2(dz).to_degrees();
    let pitch = -dy.atan2(dx.hypot(dz)).to_degrees();
    (yaw, pitch as f32)
}

/// The distance from `eye` to an enderman's box with feet at `at`.
fn eye_to_box(eye: [f64; 3], at: [f64; 3]) -> f64 {
    let lo = [at[0] - ENDER_HALF, at[1], at[2] - ENDER_HALF];
    let hi = [at[0] + ENDER_HALF, at[1] + ENDER_HEIGHT, at[2] + ENDER_HALF];
    (0..3)
        .map(|i| (eye[i] - eye[i].clamp(lo[i], hi[i])).powi(2))
        .sum::<f64>()
        .sqrt()
}

/// The feet are in B: the block west of the feet cell is solid (the back wall).
fn in_back(w: &World, feet: [f64; 3]) -> bool {
    let pos = world::BlockPos::new(
        feet[0].floor() as i32 - 1,
        (feet[1] + 0.01).floor() as i32,
        feet[2].floor() as i32,
    );
    w.block_at(pos)
        .is_some_and(|k| k != data::BlockKind::Air && k != data::BlockKind::CaveAir)
}

/// A burrow script's controls this tick.
fn burrow_controls(mode: BurrowMode, w: &World) -> Controls {
    let Some(body) = w.body else {
        return Controls::default();
    };
    let feet = body.pos;
    let eye = [feet[0], feet[1] + physics::EYE_HEIGHT, feet[2]];
    let target = nearest_ender(w, feet);
    let look = |h: f64| {
        let (yaw, pitch) =
            target.map_or((270.0, 0.0), |(_, p)| look_at(eye, [p[0], p[1] + h, p[2]]));
        Controls {
            yaw,
            pitch: Some(pitch),
            ..Controls::default()
        }
    };
    let creepy = target.is_some_and(|(id, _)| creepy_of(w, id));
    match mode {
        BurrowMode::Eye => look(ENDER_EYE),
        BurrowMode::Near if creepy => look(ENDER_TORSO),
        BurrowMode::Hold | BurrowMode::Strike if in_back(w, feet) => look(ENDER_TORSO),
        BurrowMode::Hold | BurrowMode::Strike if creepy => Controls {
            forward: true,
            yaw: 90.0,
            pitch: Some(0.0),
            ..Controls::default()
        },
        BurrowMode::Hold | BurrowMode::Strike | BurrowMode::Near => look(ENDER_EYE),
    }
}

/// A burrow script's acts this tick: only `Strike` acts, from B.
fn burrow_acts(mode: BurrowMode, tick: u64, w: &World) -> Vec<Act> {
    let Some(body) = w.body else {
        return Vec::new();
    };
    if mode != BurrowMode::Strike
        || tick % STRIKE_EVERY != STRIKE_EVERY - 1
        || !in_back(w, body.pos)
    {
        return Vec::new();
    }
    let eye = [body.pos[0], body.pos[1] + physics::EYE_HEIGHT, body.pos[2]];
    match nearest_ender(w, body.pos) {
        Some((id, at)) if eye_to_box(eye, at) <= STRIKE_REACH => {
            vec![Act::Attack { entity: id }, Act::Swing]
        }
        _ => Vec::new(),
    }
}

/// What a scripted bot presses.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Script {
    /// Issue 06's staircase: 40 ticks south then 40 west, over and over, jumping for the first
    /// 6 ticks of each west leg.
    WalkSquare,
    /// Forward along one yaw, never jumping.
    Straight { yaw: f64 },
    /// Forward along one yaw, holding jump: ledges and steps are climbed, water swum.
    JumpForward { yaw: f64 },
    /// Nothing pressed.
    Still,
    /// Forward along one yaw, sneaking: the walk stops at an edge (`sneak:<yaw>`).
    SneakForward { yaw: f64 },
    /// Standing still, sneaking: the server holds the shift key (`crouch`; magma does not burn).
    Crouch,
    /// Standing still, dig the block under the feet: START at tick 0, FINISH at `finish`.
    DigDown { finish: u64 },
    /// Standing still, craft in window 0: one click per round trip (the next only once the
    /// last moved the window's state id).
    Craft(Craft),
    /// Standing still facing `yaw`, attack the nearest living entity within 5 blocks every
    /// `every` ticks (issue 12a): `attack:<every>:<yaw>`.
    Attack { every: u64, yaw: f64 },
    /// Standing still, start using the held item at tick `at` (eating): `eat:<tick>`.
    Eat { at: u64 },
    /// Standing still, smelt in a furnace (issue 13a): `smelt:<take>`. The given furnace (hotbar
    /// 0) placed east of the feet and opened; the fuel (hotbar 2) then the input (hotbar 1) put
    /// in whole; at script tick `take` the result shift-clicked out, then the window closed.
    Smelt { take: u64 },
    /// Standing still looking along `yaw` and `pitch` from the start, use the held item at tick
    /// `at` (issue 14a: a bucket): `use:<tick>:<yaw>:<pitch>`.
    Use { at: u64, yaw: f64, pitch: f32 },
    /// Standing still, use the held item on a face of the block `dx dy dz` from the feet cell at
    /// tick `at` (issue 14a: flint and steel on a frame): `light:<tick>:<dx>:<dy>:<dz>:<face>`.
    Light {
        at: u64,
        dx: i32,
        dy: i32,
        dz: i32,
        face: i8,
    },
    /// Issue 18's burrow rows (`burrow:<mode>`): the look and the step read from the world each
    /// tick (`steer`).
    Burrow(BurrowMode),
}

impl Script {
    /// The controls this tick, read from the world where a script needs it (a burrow script's
    /// look follows the enderman); every other script's are `controls(tick)`.
    pub fn steer(self, tick: u64, w: &World) -> Controls {
        match self {
            Script::Burrow(mode) => burrow_controls(mode, w),
            other => other.controls(tick),
        }
    }

    pub fn controls(self, tick: u64) -> Controls {
        match self {
            Script::WalkSquare => {
                let side = (tick / 40) % 4;
                let into_side = tick % 40;
                Controls {
                    forward: true,
                    jump: side % 2 == 1 && into_side < 6,
                    sneak: false,
                    yaw: [0.0, 90.0, 0.0, 90.0][side as usize],
                    pitch: None,
                }
            }
            Script::Straight { yaw } => Controls {
                forward: true,
                jump: false,
                sneak: false,
                yaw,
                pitch: None,
            },
            Script::JumpForward { yaw } => Controls {
                forward: true,
                jump: true,
                sneak: false,
                yaw,
                pitch: None,
            },
            Script::SneakForward { yaw } => Controls {
                forward: true,
                jump: false,
                sneak: true,
                yaw,
                pitch: None,
            },
            Script::Crouch => Controls {
                sneak: true,
                ..Controls::default()
            },
            Script::Still
            | Script::DigDown { .. }
            | Script::Craft(_)
            | Script::Eat { .. }
            | Script::Smelt { .. }
            | Script::Light { .. }
            | Script::Burrow(_) => Controls::default(),
            Script::Use { yaw, pitch, .. } => Controls {
                yaw,
                pitch: Some(pitch),
                ..Controls::default()
            },
            Script::Attack { yaw, .. } => Controls {
                forward: false,
                jump: false,
                sneak: false,
                yaw,
                pitch: None,
            },
        }
    }

    /// What the script does to the world this tick, read from what the world holds.
    pub fn acts(self, tick: u64, w: &World) -> Vec<Act> {
        match self {
            Script::Burrow(mode) => burrow_acts(mode, tick, w),
            Script::DigDown { finish } => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                // The block under the feet (y a hair under the top of the block stood on).
                let pos = world::BlockPos::new(
                    body.pos[0].floor() as i32,
                    (body.pos[1] - 0.01).floor() as i32,
                    body.pos[2].floor() as i32,
                );
                let step = match tick {
                    0 => DigStep::Start,
                    t if t == finish => DigStep::Finish,
                    _ => return Vec::new(),
                };
                vec![Act::Dig { step, pos, face: 1 }]
            }
            Script::Smelt { take } => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                let (fx, fy, fz) = (
                    body.pos[0].floor() as i32,
                    (body.pos[1] + 0.01).floor() as i32,
                    body.pos[2].floor() as i32,
                );
                // The furnace menu: the hotbar at 30-38 (window 0's 36-44).
                const LOAD: [(i16, i8, i32); 4] = [(32, 0, 0), (1, 0, 0), (31, 0, 0), (0, 0, 0)];
                let sent = usize::try_from(w.clicks_sent).unwrap_or(usize::MAX);
                match (w.sequence_sent, &w.window, w.click.is_none()) {
                    (0, None, _) => vec![Act::UseOn {
                        pos: world::BlockPos::new(fx + 1, fy - 1, fz),
                        face: 1,
                    }],
                    (1, None, _) if sent == 0 && tick >= 5 => vec![Act::UseOn {
                        pos: world::BlockPos::new(fx + 1, fy, fz),
                        face: 4,
                    }],
                    (_, Some(_), true) => match LOAD.get(sent) {
                        Some(&(slot, button, mode)) => vec![Act::Click { slot, button, mode }],
                        None if sent == LOAD.len() && tick >= take => vec![Act::Click {
                            slot: 2,
                            button: 0,
                            mode: 1,
                        }],
                        None if sent > LOAD.len() => vec![Act::CloseWindow],
                        None => Vec::new(),
                    },
                    (_, _, _) => Vec::new(),
                }
            }
            Script::Craft(c) if c.at_table() => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                let (fx, fy, fz) = (
                    body.pos[0].floor() as i32,
                    (body.pos[1] + 0.01).floor() as i32,
                    body.pos[2].floor() as i32,
                );
                let sent = usize::try_from(w.clicks_sent).unwrap_or(usize::MAX);
                let clicks = c.clicks();
                match (w.sequence_sent, &w.window, w.click.is_none()) {
                    // Place the held table on the block east of the feet, then open it.
                    (0, None, _) => vec![Act::UseOn {
                        pos: world::BlockPos::new(fx + 1, fy - 1, fz),
                        face: 1,
                    }],
                    (1, None, _) if sent == 0 && tick >= 5 => vec![Act::UseOn {
                        pos: world::BlockPos::new(fx + 1, fy, fz),
                        face: 4,
                    }],
                    (_, Some(_), true) => match clicks.get(sent) {
                        Some(&(slot, button, mode)) => vec![Act::Click { slot, button, mode }],
                        None => vec![Act::CloseWindow],
                    },
                    (_, _, _) => Vec::new(),
                }
            }
            Script::Craft(c) => {
                let sent = usize::try_from(w.clicks_sent).unwrap_or(usize::MAX);
                match (w.click.is_none(), c.clicks().get(sent)) {
                    (true, Some(&(slot, button, mode))) => vec![Act::Click { slot, button, mode }],
                    (_, _) => Vec::new(),
                }
            }
            Script::Attack { every, .. } => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                if tick % every.max(1) != every.max(1) - 1 {
                    return Vec::new();
                }
                let near = w
                    .entities
                    .iter()
                    // Mobs only, never a player: the server kicks a client that attacks an
                    // item or an experience orb (`invalid_entity_attacked`), and a kill drops one.
                    .filter(|(_, e)| {
                        !e.dead
                            && e.kind != data::EntityKind::Player
                            && data::generated::mobs::MOBS
                                .iter()
                                .any(|m| m.entity == e.kind)
                    })
                    .map(|(id, e)| {
                        let d = (0..3)
                            .map(|i| (e.pos[i] - body.pos[i]).powi(2))
                            .sum::<f64>();
                        (d, *id)
                    })
                    // Only at the summon spot (two blocks east of the feet): a passive animal that
                    // wanders by later is not the row's (a runner's cow row struck three).
                    .filter(|(d, _)| *d <= 6.25)
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                near.map(|(_, id)| vec![Act::Attack { entity: id }])
                    .unwrap_or_default()
            }
            Script::Eat { at } | Script::Use { at, .. } => {
                if tick == at {
                    vec![Act::UseItem]
                } else {
                    Vec::new()
                }
            }
            Script::Light {
                at,
                dx,
                dy,
                dz,
                face,
            } => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                if tick != at {
                    return Vec::new();
                }
                let (fx, fy, fz) = (
                    body.pos[0].floor() as i32,
                    (body.pos[1] + 0.01).floor() as i32,
                    body.pos[2].floor() as i32,
                );
                vec![Act::UseOn {
                    pos: world::BlockPos::new(fx + dx, fy + dy, fz + dz),
                    face,
                }]
            }
            Script::WalkSquare
            | Script::Straight { .. }
            | Script::JumpForward { .. }
            | Script::SneakForward { .. }
            | Script::Crouch
            | Script::Still => Vec::new(),
        }
    }

    /// The name a case and a flag use: `square`, `straight:90`, `jump:180`, `still`.
    pub fn name(self) -> String {
        match self {
            Script::WalkSquare => "square".to_owned(),
            Script::Straight { yaw } => format!("straight:{yaw}"),
            Script::JumpForward { yaw } => format!("jump:{yaw}"),
            Script::Still => "still".to_owned(),
            Script::SneakForward { yaw } => format!("sneak:{yaw}"),
            Script::Crouch => "crouch".to_owned(),
            Script::DigDown { finish } => format!("dig:{finish}"),
            Script::Craft(c) => format!("craft:{}", c.name()),
            Script::Attack { every, yaw } => format!("attack:{every}:{yaw}"),
            Script::Eat { at } => format!("eat:{at}"),
            Script::Smelt { take } => format!("smelt:{take}"),
            Script::Use { at, yaw, pitch } => format!("use:{at}:{yaw}:{pitch}"),
            Script::Light {
                at,
                dx,
                dy,
                dz,
                face,
            } => format!("light:{at}:{dx}:{dy}:{dz}:{face}"),
            Script::Burrow(mode) => format!("burrow:{}", mode.name()),
        }
    }

    pub fn parse(s: &str) -> Option<Script> {
        let (head, yaw) = match s.split_once(':') {
            Some((h, y)) => (h, y.parse::<f64>().ok()),
            None => (s, None),
        };
        match (head, yaw) {
            ("square", None) => Some(Script::WalkSquare),
            ("still", None) => Some(Script::Still),
            ("straight", Some(yaw)) => Some(Script::Straight { yaw }),
            ("jump", Some(yaw)) => Some(Script::JumpForward { yaw }),
            ("sneak", Some(yaw)) => Some(Script::SneakForward { yaw }),
            ("crouch", None) => Some(Script::Crouch),
            ("dig", Some(f)) if f >= 0.0 && f.fract() == 0.0 => {
                Some(Script::DigDown { finish: f as u64 })
            }
            _ => Self::parse_use(s).or_else(|| Self::parse_craft(s)),
        }
    }

    fn parse_use(s: &str) -> Option<Script> {
        if let Some(rest) = s.strip_prefix("use:") {
            let mut p = rest.split(':');
            let at = p.next()?.parse().ok()?;
            let yaw = p.next()?.parse().ok()?;
            let pitch = p.next()?.parse().ok()?;
            return p.next().is_none().then_some(Script::Use { at, yaw, pitch });
        }
        let rest = s.strip_prefix("light:")?;
        let v: Vec<&str> = rest.split(':').collect();
        let [at, dx, dy, dz, face] = v.as_slice() else {
            return None;
        };
        Some(Script::Light {
            at: at.parse().ok()?,
            dx: dx.parse().ok()?,
            dy: dy.parse().ok()?,
            dz: dz.parse().ok()?,
            face: face.parse().ok()?,
        })
    }

    fn parse_craft(s: &str) -> Option<Script> {
        if let Some(mode) = s.strip_prefix("burrow:") {
            return BurrowMode::parse(mode).map(Script::Burrow);
        }
        if let Some(rest) = s.strip_prefix("attack:") {
            let (every, yaw) = rest.split_once(':')?;
            return Some(Script::Attack {
                every: every.parse().ok()?,
                yaw: yaw.parse().ok()?,
            });
        }
        if let Some(take) = s.strip_prefix("smelt:") {
            return Some(Script::Smelt {
                take: take.parse().ok()?,
            });
        }
        if let Some(at) = s.strip_prefix("eat:") {
            return Some(Script::Eat {
                at: at.parse().ok()?,
            });
        }
        match s {
            "craft:planks" => Some(Script::Craft(Craft::Planks)),
            "craft:sticks" => Some(Script::Craft(Craft::Sticks)),
            "craft:table" => Some(Script::Craft(Craft::Table)),
            "craft:wooden_pickaxe" => Some(Script::Craft(Craft::WoodenPickaxe)),
            "craft:stone_tools" => Some(Script::Craft(Craft::StoneTools)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_back() {
        for s in [
            Script::WalkSquare,
            Script::Still,
            Script::Straight { yaw: 90.0 },
            Script::JumpForward { yaw: -45.0 },
            Script::SneakForward { yaw: 90.0 },
            Script::Crouch,
            Script::DigDown { finish: 30 },
            Script::Craft(Craft::Planks),
            Script::Craft(Craft::Sticks),
            Script::Craft(Craft::Table),
            Script::Craft(Craft::WoodenPickaxe),
            Script::Craft(Craft::StoneTools),
            Script::Attack {
                every: 13,
                yaw: 270.0,
            },
            Script::Eat { at: 220 },
            Script::Smelt { take: 460 },
            Script::Burrow(BurrowMode::Hold),
            Script::Burrow(BurrowMode::Strike),
            Script::Burrow(BurrowMode::Eye),
            Script::Burrow(BurrowMode::Near),
        ] {
            assert_eq!(Script::parse(&s.name()), Some(s));
        }
        assert_eq!(Script::parse("straight"), None);
    }
}
