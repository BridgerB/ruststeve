//! Vanilla land and liquid movement as a pure function (brain/ruststeve/design/physics.md): a
//! port of clojurecraft's `physics.clj` at be85a92, function for function and operation for
//! operation, checked against it tick by tick (`tests/differential.rs`).
//!
//! Full-cube collision and the flat boxes of carpets, which the body stands on and steps onto
//! (`rise_onto`); a one-block ledge needs a jump. In a liquid: no buoyancy to speak of, a swim-up
//! of 0.04 a tick while jumping, and the one real lift, an impulse of 0.3 when pressing into a
//! bank whose top is free (brain/game/mc-water.md).
//!
//! The world is read through two oracles: `solid(x, y, z)` gives `None` for nothing, `Some(1.0)`
//! for a full cube and `Some(top)` for a flat box under 1 (a carpet's 0.0625); `fluid(x, y, z)`
//! gives the fluid there and its level. `step_on` takes a third, `surface(x, y, z)`: what the
//! block does to a body moving on it (soul sand's speed factor, honey's jump factor).

#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde::{Deserialize, Serialize};

/// One game tick: the server runs twenty a second; "per tick" below means per one of these.
pub const TICK_MS: u64 = 50;
/// Blocks per tick², subtracted from vy each tick.
pub const GRAVITY: f64 = 0.08;
/// vy is multiplied by this each tick.
pub const VERTICAL_DRAG: f64 = 0.98;
/// The player box is 0.6 wide.
pub const HALF_WIDTH: f64 = 0.3;
/// The player box height.
pub const HEIGHT: f64 = 1.8;
/// Eye above the feet.
pub const EYE_HEIGHT: f64 = 1.62;
/// vy given by a jump.
pub const JUMP_VELOCITY: f64 = 0.42;
/// Blocks a body on the ground walking into a low box rises onto it (vanilla's maxUpStep): a
/// carpet, never a block.
pub const STEP_HEIGHT: f64 = 0.6;
/// Slipperiness × 0.91.
pub const GROUND_INERTIA: f64 = 0.6 * 0.91;
/// Horizontal velocity kept per tick in the air.
pub const AIR_INERTIA: f64 = 0.91;
/// Horizontal input acceleration in the air.
pub const AIR_ACCELERATION: f64 = 0.02;
/// Velocities below this snap to zero.
pub const NEGLIGIBLE: f64 = 0.003;
/// Horizontal input acceleration in a liquid.
pub const LIQUID_ACCELERATION: f64 = 0.02;
/// Velocity kept per tick in water.
pub const WATER_INERTIA: f64 = 0.8;
/// Velocity kept per tick in lava.
pub const LAVA_INERTIA: f64 = 0.5;
/// Gravity in water (1/16 of land, 1.13+).
pub const WATER_GRAVITY: f64 = 0.005;
/// Gravity in lava.
pub const LAVA_GRAVITY: f64 = 0.02;
/// vy added per tick while jumping in a liquid.
pub const SWIM_UP: f64 = 0.04;
/// vy taken per tick while sneaking in a liquid.
pub const SWIM_DOWN: f64 = 0.04;
/// The lift when pressing into a bank whose top is free.
pub const OUT_OF_LIQUID_IMPULSE: f64 = 0.3;
/// How far up the box is moved to ask whether a bank's top is free, less what it rose this tick.
pub const BANK_PROBE: f64 = 0.6;
/// The movement input a held forward key gives (vanilla scales input by 0.98).
pub const FORWARD_INPUT: f64 = 0.98;
/// The movement input is scaled by this while sneaking: the default of the `sneaking_speed`
/// attribute (`Attributes.SNEAKING_SPEED`, 0.3), which the client applies to its input vector
/// while it is crouching (`LocalPlayer.modifyInput`; javap of the 26.1.2 jars, probe 45339873719).
pub const SNEAKING_SPEED: f64 = 0.3;
/// The step a sneaking body's move is cut back by until it no longer leaves the edge
/// (`Player.maybeBackOffFromEdge`, 0.05).
pub const EDGE_BACK_OFF: f64 = 0.05;
/// The largest move the edge back-off is asked about, in blocks a tick: the server calls a
/// player's move of more than this "moved too quickly" (`ServerGamePacketListenerImpl`, a
/// squared distance over 100), so nothing real is left out.
pub const MAX_BACK_OFF: f64 = 10.0;
/// Ticks after a jump before the next one.
pub const JUMP_COOLDOWN: i64 = 10;
/// The box shrinks by this before asking "in water?".
pub const WATER_CONTRACT: [f64; 3] = [0.001, 0.401, 0.001];
/// And by this for "in lava?".
pub const LAVA_CONTRACT: [f64; 3] = [0.1, 0.4, 0.1];
/// And by this for "touching a fluid at all?": vanilla's own test since 1.13, which the bank
/// impulse is asked under.
pub const TOUCH_CONTRACT: [f64; 3] = [0.001, 0.001, 0.001];

/// Horizontal input acceleration on the ground: `0.1 × (0.16277136 / ground-inertia³)`, about 0.1.
pub fn ground_acceleration() -> f64 {
    0.1 * (0.16277136 / GROUND_INERTIA.powi(3))
}

// The sprint's numbers are config/sprint.toml's (S1's javap of the 26.1.2 jars, probe
// 46115253148), mirrored as constants because the core reads no files; the test
// `the_constants_are_config_sprint_toml` holds them equal.

/// The `movement_speed` attribute is multiplied by `1 + this` while sprinting: the sprint
/// modifier, an `add_multiplied_total` of 0.3 (`LivingEntity.SPEED_MODIFIER_SPRINTING`,
/// 0.30000001192092896, the float 0.3), so the ground acceleration is ×1.3 and a steady sprint
/// is 5.612 blocks a second against 4.317. The walk's 0.1 is the round value, not the float, as
/// clojurecraft's is; the sprint's 0.3 likewise.
pub const SPRINT_SPEED_BOOST: f64 = 0.3;
/// Horizontal input acceleration in the air while sprinting (`Player.getFlyingSpeed`:
/// `isSprinting ? 0.025999999f : 0.02f`, the float widened).
pub const SPRINT_AIR_ACCELERATION: f64 = 0.025_999_999_046_325_684;
/// The horizontal push a jump gets along the yaw while sprinting
/// (`LivingEntity.jumpFromGround`: `addDeltaMovement(-sin yaw × 0.2, 0, cos yaw × 0.2)`).
pub const SPRINT_JUMP_BOOST: f64 = 0.2;
/// Food at or under this stops a sprint and refuses one (`FoodData.hasEnoughFood`: food > 6,
/// `FoodConstants.SPRINT_LEVEL`).
pub const SPRINT_FOOD_FLOOR: i32 = 6;
/// A collision whose move still points within this of the facing, in radians, is minor and
/// does not stop a sprint (`LocalPlayer.isHorizontalCollisionMinor`, 0.13962634F: 8°).
pub const MINOR_COLLISION_ANGLE: f64 = 0.139_626_339_077_949_52;
/// Under this squared length, the facing or the move is no direction and a collision is not
/// minor (`isHorizontalCollisionMinor`'s 1.0E-5F widened).
pub const MINOR_COLLISION_MIN_SQ: f64 = 9.999_999_747_378_752e-6;

/// Does the body sprint this tick, given its food (`None`: unknown, as in a prediction, counts
/// as enough)? The client's rules (`LocalPlayer.aiStep`, decided before the move), with one
/// change: the bot's wish (`Controls.sprint`) is a held key whose release stops the sprint,
/// where a vanilla client's flag holds until a stop rule; the server takes either (it checks no
/// sprint, config/sprint.toml `[server]`).
/// - **A start** (not sprinting last tick) also needs no sneak (`isMovingSlowly`).
/// - **Either**, a start or going on: forward held, sprinting possible, and no horizontal
///   collision last tick unless a minor one (the stop rules run the same tick, after a start).
///   A sneak does not stop a sprint: one held into a crouch keeps the flag (at the sneak's
///   input).
/// - **Possible**: food over 6, and not in water. Vanilla refuses only shallow water (the eyes
///   out of it); under it the body swims, a pose not modelled, so here water refuses it all.
///   Blindness and using an item (which refuse a start) are not read: the bot moves with
///   neither.
pub fn sprint_allowed(body: &Body, food: Option<i32>, controls: &Controls) -> bool {
    let possible = food.is_none_or(|f| f > SPRINT_FOOD_FLOOR) && !body.in_water;
    let free = !body.horizontal_collision || body.minor_horizontal_collision;
    controls.sprint && controls.forward && possible && free && (body.sprinting || !controls.sneak)
}

/// Was a collision minor: does the move the body made still point within 8° of where it faced
/// (`LocalPlayer.isHorizontalCollisionMinor`)? `input` is the input along the facing, `(dx, dz)`
/// the move after collision; either under 1.0E-5 squared is not minor.
pub fn minor_collision(yaw: f64, input: f64, (dx, dz): (f64, f64)) -> bool {
    let rad = yaw.to_radians();
    let (g, h) = (-input * rad.sin(), input * rad.cos());
    let i = g * g + h * h;
    let j = dx * dx + dz * dz;
    if i < MINOR_COLLISION_MIN_SQ || j < MINOR_COLLISION_MIN_SQ {
        return false;
    }
    ((g * dx + h * dz) / (i * j).sqrt()).acos() < MINOR_COLLISION_ANGLE
}

/// `[x0, y0, z0, x1, y1, z1]`.
pub type Aabb = [f64; 6];
pub type Vec3 = [f64; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FluidKind {
    Water,
    Lava,
}

/// A fluid in a block: its kind and level (0 a source, 1-7 flowing, 8+ falling).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fluid {
    pub kind: FluidKind,
    pub level: u8,
}

/// What collides at a block: `None` nothing, `Some(1.0)` a full cube, `Some(top)` a flat box.
pub trait Solid: Fn(i32, i32, i32) -> Option<f64> {}
impl<F: Fn(i32, i32, i32) -> Option<f64>> Solid for F {}

/// The fluid at a block, if any.
pub trait FluidAt: Fn(i32, i32, i32) -> Option<Fluid> {}
impl<F: Fn(i32, i32, i32) -> Option<Fluid>> FluidAt for F {}

/// A fluid oracle for a dry world.
pub fn no_fluid(_: i32, _: i32, _: i32) -> Option<Fluid> {
    None
}

/// What a block does to a body moving in or on it (`Block.getSpeedFactor`, `getJumpFactor`),
/// as the game's floats widened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Surface {
    /// The horizontal velocity is multiplied by this after each move (soul sand, honey: 0.4).
    pub speed_factor: f64,
    /// A jump's velocity is multiplied by this (honey: 0.5).
    pub jump_factor: f64,
    /// Water or a bubble column: with the feet in it, the block under them is not asked.
    pub wet: bool,
}

impl Surface {
    /// A block that changes nothing.
    pub const PLAIN: Surface = Surface {
        speed_factor: 1.0,
        jump_factor: 1.0,
        wet: false,
    };
}

/// What the block at a cell does to movement.
pub trait SurfaceAt: Fn(i32, i32, i32) -> Surface {}
impl<F: Fn(i32, i32, i32) -> Surface> SurfaceAt for F {}

/// A surface oracle for a world of plain blocks.
pub fn plain_surface(_: i32, _: i32, _: i32) -> Surface {
    Surface::PLAIN
}

/// How far under the feet the block that affects movement is looked for
/// (`Entity.getBlockPosBelowThatAffectsMyMovement`, 0.500001).
pub const BELOW_AFFECTS: f64 = 0.500_001;
/// The slab under the feet a supporting block must reach (`Entity.checkSupportingBlock`, 1.0E-6).
pub const SUPPORT_SLAB: f64 = 1.0e-6;

/// The block a body on the ground stands on: of the blocks whose shape reaches the feet, the
/// one whose centre is nearest the body (`Level.findSupportingBlock`; ties to the lower y, then
/// z, then x, as `Vec3i.compareTo`). The game's second try, the box moved back by the last
/// move, is not made.
pub fn supporting_block(solid: &impl Solid, pos: Vec3) -> Option<(i32, i32, i32)> {
    let b = aabb(pos);
    let slab = [b[0], b[1] - SUPPORT_SLAB, b[2], b[3], b[1], b[5]];
    let mut best: Option<(f64, (i32, i32, i32))> = None;
    for c in solid_boxes(solid, slab) {
        if !(0..3).all(|axis| overlaps(&slab, &c, axis)) {
            continue;
        }
        let cell = (floor(c[0]), floor(c[1]), floor(c[2]));
        let d = (c[0] + 0.5 - pos[0]).powi(2)
            + (c[1] + 0.5 - pos[1]).powi(2)
            + (c[2] + 0.5 - pos[2]).powi(2);
        let key = (cell.1, cell.2, cell.0);
        let nearer = best.is_none_or(|(bd, bc)| d < bd || (d == bd && key < (bc.1, bc.2, bc.0)));
        if nearer {
            best = Some((d, cell));
        }
    }
    best.map(|(_, cell)| cell)
}

/// The block under the body that affects its movement: 0.500001 under the feet, in the column
/// of the supporting block when the body is on the ground (`Entity.getOnPos`).
pub fn below_that_affects(solid: &impl Solid, pos: Vec3, on_ground: bool) -> (i32, i32, i32) {
    let y = floor(pos[1] - BELOW_AFFECTS);
    match on_ground.then(|| supporting_block(solid, pos)).flatten() {
        Some((x, _, z)) => (x, y, z),
        None => (floor(pos[0]), y, floor(pos[2])),
    }
}

/// The factor the horizontal velocity is multiplied by after a move (`Entity.getBlockSpeedFactor`,
/// javap of the 26.1.2 server jar, probe 45339873719): that of the block the feet are in (a body
/// on soul sand stands inside its cell, the block being 14/16 high), alone when that block is
/// water or a bubble column; when it is 1, that of the block under the body.
pub fn block_speed_factor(
    solid: &impl Solid,
    surface: &impl SurfaceAt,
    pos: Vec3,
    on_ground: bool,
) -> f64 {
    let here = surface(floor(pos[0]), floor(pos[1]), floor(pos[2]));
    if here.wet || here.speed_factor != 1.0 {
        return here.speed_factor;
    }
    let (x, y, z) = below_that_affects(solid, pos, on_ground);
    surface(x, y, z).speed_factor
}

/// The factor a jump's velocity is multiplied by (`Entity.getBlockJumpFactor`): that of the
/// block the feet are in, or when it is 1 that of the block under the body.
pub fn block_jump_factor(
    solid: &impl Solid,
    surface: &impl SurfaceAt,
    pos: Vec3,
    on_ground: bool,
) -> f64 {
    let here = surface(floor(pos[0]), floor(pos[1]), floor(pos[2]));
    if here.jump_factor != 1.0 {
        return here.jump_factor;
    }
    let (x, y, z) = below_that_affects(solid, pos, on_ground);
    surface(x, y, z).jump_factor
}

/// The pace of a steady walk on a block with this speed factor, as a share of the plain walk's:
/// a walk settles at `accel / (1 - factor × inertia)` a tick, so the share is
/// `(1 - inertia) / (1 - factor × inertia)`: 1 for a plain block, 0.581 for soul sand and honey
/// (2.51 blocks a second against 4.32). A route's cost per block there is its inverse. The factor
/// is `data::generated::movement::movement(block).speed_factor`, or `World::surface`'s.
pub fn pace_factor(speed_factor: f64) -> f64 {
    (1.0 - GROUND_INERTIA) / (1.0 - speed_factor * GROUND_INERTIA)
}

/// The player's body: what the step reads and writes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Body {
    /// Feet centre.
    pub pos: Vec3,
    /// Blocks per tick.
    pub vel: Vec3,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub jump_ticks: i64,
    pub fall_distance: f64,
    pub in_water: bool,
    pub in_lava: bool,
    pub eye_in_water: bool,
    /// The horizontal collision was minor (`minor_collision`): it does not stop a sprint.
    #[serde(default)]
    pub minor_horizontal_collision: bool,
    /// Sprinting after the stop rules (`sprint_allowed`), as `step_on` left it: what the client
    /// reports to the server with a start or stop sprinting command.
    #[serde(default)]
    pub sprinting: bool,
}

/// What is pressed this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    pub forward: bool,
    pub jump: bool,
    pub sneak: bool,
    /// Degrees, Notchian (south 0, west +90).
    pub yaw: f64,
    /// Degrees down (Notchian: +90 straight down), sent with the look when set (issue 14a: a
    /// bucket is used along the look); `None` keeps the pitch the server last set.
    #[serde(default)]
    pub pitch: Option<f32>,
    /// The client's wish to sprint (the sprint key held): the body sprints only while
    /// `sprint_allowed` holds.
    #[serde(default)]
    pub sprint: bool,
}

fn floor(v: f64) -> i32 {
    v.floor() as i32
}

/// The box of a player whose feet centre is `pos`.
pub fn aabb([x, y, z]: Vec3) -> Aabb {
    [
        x - HALF_WIDTH,
        y,
        z - HALF_WIDTH,
        x + HALF_WIDTH,
        y + HEIGHT,
        z + HALF_WIDTH,
    ]
}

/// The box grown along a velocity: the region a move may sweep through this tick.
pub fn expand([x0, y0, z0, x1, y1, z1]: Aabb, [vx, vy, vz]: Vec3) -> Aabb {
    [
        x0 + 0.0f64.min(vx),
        y0 + 0.0f64.min(vy),
        z0 + 0.0f64.min(vz),
        x1 + 0.0f64.max(vx),
        y1 + 0.0f64.max(vy),
        z1 + 0.0f64.max(vz),
    ]
}

/// The boxes of every block touching the region that collides: a unit box for a full cube, one
/// up to the top `solid` gives for a flat shape. x outer, then y, then z, as clojurecraft's `for`.
pub fn solid_boxes(solid: &impl Solid, [x0, y0, z0, x1, y1, z1]: Aabb) -> Vec<Aabb> {
    let mut out = Vec::new();
    for x in floor(x0)..=floor(x1) {
        for y in floor(y0)..=floor(y1) {
            for z in floor(z0)..=floor(z1) {
                if let Some(top) = solid(x, y, z) {
                    let (x, y, z) = (f64::from(x), f64::from(y), f64::from(z));
                    out.push([x, y, z, x + 1.0, y + top, z + 1.0]);
                }
            }
        }
    }
    out
}

/// Do boxes overlap strictly along `axis` (0 x, 1 y, 2 z)? Touching faces do not.
pub fn overlaps(a: &Aabb, b: &Aabb, axis: usize) -> bool {
    a[axis] < b[axis + 3] && a[axis + 3] > b[axis]
}

/// vanilla's collision tolerance (`VoxelShape.collideX`, 1.0E-7): a face within this of the
/// box's own counts as ahead of it, and the other axes must overlap by more than this. Without
/// it a box whose edge rounds a hair past a face (-2.3 + 0.3 = -1.9999999999999998) walks into
/// the block, and the server corrects it (found by the model's generated worlds, issue 08b).
pub const COLLISION_EPSILON: f64 = 1.0e-7;

/// Shrink the move `d` along `axis` so `boxx` does not enter any of `boxes`.
pub fn clip(boxx: &Aabb, boxes: &[Aabb], axis: usize, d: f64) -> f64 {
    let others: [usize; 2] = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let e = COLLISION_EPSILON;
    boxes.iter().fold(d, |d, c| {
        if others
            .iter()
            .all(|o| boxx[*o] + e < c[*o + 3] && boxx[*o + 3] - e > c[*o])
        {
            if d > 0.0 && c[axis] >= boxx[axis + 3] - e {
                d.min(c[axis] - boxx[axis + 3])
            } else if d < 0.0 && c[axis + 3] <= boxx[axis] + e {
                d.max(c[axis + 3] - boxx[axis])
            } else {
                d
            }
        } else {
            d
        }
    })
}

/// The box moved `d` along `axis`.
pub fn shift(mut b: Aabb, axis: usize, d: f64) -> Aabb {
    b[axis] += d;
    b[axis + 3] += d;
    b
}

/// `v`, or 0.0 below `NEGLIGIBLE` (vanilla's velocity snap).
pub fn squash(v: f64) -> f64 {
    if v.abs() < NEGLIGIBLE { 0.0 } else { v }
}

/// How high a fluid of `level` fills its block, 0-1: 8 and above (falling) is full, else
/// `1 - (level + 1) / 9`, so a source fills 8/9.
pub fn fluid_height(level: u8) -> f64 {
    if level >= 8 {
        1.0
    } else {
        1.0 - f64::from(level + 1) / 9.0
    }
}

/// Does the box, contracted by `c`, overlap any block of fluid `kind`, counting a block only up
/// to the height its level fills?
pub fn in_fluid(
    fluid: &impl FluidAt,
    kind: FluidKind,
    [cx, cy, cz]: Vec3,
    [x0, y0, z0, x1, y1, z1]: Aabb,
) -> bool {
    let (x0, y0, z0, x1, y1, z1) = (x0 + cx, y0 + cy, z0 + cz, x1 - cx, y1 - cy, z1 - cz);
    for bx in floor(x0)..=floor(x1) {
        for by in floor(y0)..=floor(y1) {
            for bz in floor(z0)..=floor(z1) {
                if let Some(f) = fluid(bx, by, bz)
                    && f.kind == kind
                    && y0 < f64::from(by) + fluid_height(f.level)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Is the eye (feet + 1.62) inside water, counting the block only up to its fill height?
pub fn eye_in_water(fluid: &impl FluidAt, [x, y, z]: Vec3) -> bool {
    let ey = y + EYE_HEIGHT;
    fluid(floor(x), floor(ey), floor(z))
        .is_some_and(|f| f.kind == FluidKind::Water && (ey - ey.floor()) < fluid_height(f.level))
}

/// Does the box at `pos` overlap any water, however little? Vanilla's test since 1.13.
pub fn touching_water(fluid: &impl FluidAt, pos: Vec3) -> bool {
    in_fluid(fluid, FluidKind::Water, TOUCH_CONTRACT, aabb(pos))
}

/// Does the box at `pos` overlap any water or lava, however little?
pub fn touching_fluid(fluid: &impl FluidAt, pos: Vec3) -> bool {
    touching_water(fluid, pos) || in_fluid(fluid, FluidKind::Lava, TOUCH_CONTRACT, aabb(pos))
}

/// Is the box, moved by `d`, clear of solids and of water? The test behind the out-of-liquid
/// impulse: a bank the body can rise onto.
pub fn free(solid: &impl Solid, fluid: &impl FluidAt, b: Aabb, [dx, dy, dz]: Vec3) -> bool {
    let moved = shift(shift(shift(b, 0, dx), 1, dy), 2, dz);
    solid_boxes(solid, moved).is_empty()
        && !in_fluid(fluid, FluidKind::Water, [0.0, 0.0, 0.0], moved)
}

/// The move of a body on the ground that hit something low walking `v` and got only `(hx, hz)`
/// along: lifted by the step height (less where something is over it), moved along, set down by
/// what it rose and the fall it had; `None` when that gets it no farther. Vanilla's maxUpStep.
pub fn rise_onto(
    solid: &impl Solid,
    b: Aabb,
    boxes: &[Aabb],
    [vx, vy, vz]: Vec3,
    (hx, hz): (f64, f64),
) -> Option<Vec3> {
    let mut boxes = boxes.to_vec();
    boxes.extend(solid_boxes(solid, expand(b, [vx, STEP_HEIGHT, vz])));
    let up = clip(&b, &boxes, 1, STEP_HEIGHT);
    let lifted = shift(b, 1, up);
    let dx = clip(&lifted, &boxes, 0, vx);
    let lifted = shift(lifted, 0, dx);
    let dz = clip(&lifted, &boxes, 2, vz);
    let lifted = shift(lifted, 2, dz);
    let down = clip(&lifted, &boxes, 1, -up + vy);
    (dx.abs() + dz.abs() > hx.abs() + hz.abs()).then_some([dx, up + down, dz])
}

/// Does any solid box overlap the region, by more than a touching face? Vanilla's `noCollision`,
/// negated.
pub fn collides(solid: &impl Solid, region: Aabb) -> bool {
    solid_boxes(solid, region)
        .iter()
        .any(|c| (0..3).all(|axis| overlaps(&region, c, axis)))
}

/// Could the box, moved `(dx, dz)`, fall at least `h`: is the slab of that depth under it free?
/// `Player.canFallAtLeast`, the box drawn in by 1.0E-7 at its sides.
pub fn can_fall_at_least(solid: &impl Solid, b: &Aabb, dx: f64, dz: f64, h: f64) -> bool {
    let e = COLLISION_EPSILON;
    !collides(
        solid,
        [
            b[0] + e + dx,
            b[1] - h - e,
            b[2] + e + dz,
            b[3] - e + dx,
            b[1],
            b[5] - e + dz,
        ],
    )
}

/// The horizontal move a sneaking body is allowed: `(vx, vz)` cut back in steps of 0.05 until
/// the box would still have ground within a step's height under it, so it stops at an edge
/// instead of walking off. `Player.maybeBackOffFromEdge` (javap of the 26.1.2 server jar, probe
/// 45339873719): only for a move that is not upward, from the ground or within a step's height
/// of it; x alone, then z alone, then both.
pub fn back_off_from_edge(solid: &impl Solid, body: &Body, [vx, vy, vz]: Vec3) -> (f64, f64) {
    let b = aabb(body.pos);
    let above_ground = body.on_ground
        || (body.fall_distance < STEP_HEIGHT
            && !can_fall_at_least(solid, &b, 0.0, 0.0, STEP_HEIGHT - body.fall_distance));
    // A move no body makes (a velocity from a bad frame) is left alone: the loops below count
    // down in steps of 0.05 and must end.
    let sane = vx.abs() <= MAX_BACK_OFF && vz.abs() <= MAX_BACK_OFF;
    if vy > 0.0 || !above_ground || !sane {
        return (vx, vz);
    }
    let back = |d: f64| {
        if d.abs() <= EDGE_BACK_OFF {
            0.0
        } else {
            d - d.signum() * EDGE_BACK_OFF
        }
    };
    let (mut dx, mut dz) = (vx, vz);
    while dx != 0.0 && can_fall_at_least(solid, &b, dx, 0.0, STEP_HEIGHT) {
        dx = back(dx);
    }
    while dz != 0.0 && can_fall_at_least(solid, &b, 0.0, dz, STEP_HEIGHT) {
        dz = back(dz);
    }
    while dx != 0.0 && dz != 0.0 && can_fall_at_least(solid, &b, dx, dz, STEP_HEIGHT) {
        dx = back(dx);
        dz = back(dz);
    }
    (dx, dz)
}

/// One tick of land movement: `(pos', vel', landed, hit)`. The box is swept an axis at a time
/// (y, then x, then z); a body on the ground stopped by something low steps onto it.
pub fn land_tick(solid: &impl Solid, pos: Vec3, [vx, vy, vz]: Vec3) -> (Vec3, Vec3, bool, bool) {
    let b = aabb(pos);
    let boxes = solid_boxes(solid, expand(b, [vx, vy, vz]));
    let dy = clip(&b, &boxes, 1, vy);
    let moved = shift(b, 1, dy);
    let dx = clip(&moved, &boxes, 0, vx);
    let moved = shift(moved, 0, dx);
    let dz = clip(&moved, &boxes, 2, vz);
    let [dx, dy, dz] = ((dx != vx || dz != vz) && dy != vy && vy < 0.0)
        .then(|| rise_onto(solid, b, &boxes, [vx, vy, vz], (dx, dz)))
        .flatten()
        .unwrap_or([dx, dy, dz]);
    let landed = dy != vy && vy < 0.0;
    let hit = dx != vx || dz != vz;
    let [x, y, z] = pos;
    (
        [x + dx, y + dy, z + dz],
        [
            if dx != vx { 0.0 } else { vx },
            if dy != vy { 0.0 } else { vy },
            if dz != vz { 0.0 } else { vz },
        ],
        landed,
        hit,
    )
}

/// One tick of movement: the body after `controls` for a tick in this world, every block plain.
pub fn step(solid: &impl Solid, fluid: &impl FluidAt, body: &Body, controls: &Controls) -> Body {
    step_on(solid, fluid, &plain_surface, body, controls)
}

/// One tick of movement in a world whose blocks may slow a body or damp its jump, the food
/// unknown (a sprint is not stopped by hunger): `step_fed` with `None`.
pub fn step_on(
    solid: &impl Solid,
    fluid: &impl FluidAt,
    surface: &impl SurfaceAt,
    body: &Body,
    controls: &Controls,
) -> Body {
    step_fed(solid, fluid, surface, body, controls, None)
}

/// One tick of movement, knowing the food the body has to sprint with. Food is not part of the
/// body (physics never changes it); the caller that holds it, the world, passes it each tick,
/// and `step_on`'s callers, which predict, pass none.
pub fn step_fed(
    solid: &impl Solid,
    fluid: &impl FluidAt,
    surface: &impl SurfaceAt,
    body: &Body,
    controls: &Controls,
    food: Option<i32>,
) -> Body {
    let pos = body.pos;
    let b = aabb(pos);
    let in_water = in_fluid(fluid, FluidKind::Water, WATER_CONTRACT, b);
    let in_lava = in_fluid(fluid, FluidKind::Lava, LAVA_CONTRACT, b);
    let liquid = in_water || in_lava;
    let touching = liquid || touching_fluid(fluid, pos);
    // The client decides its sprint before it moves (`LocalPlayer.aiStep`, then `travel`),
    // from last tick's collision and this tick's water.
    let sprinting = sprint_allowed(&Body { in_water, ..*body }, food, controls);
    let [vx, vy, vz] = body.vel.map(squash);
    let jump_ticks = 0.max(body.jump_ticks - 1);
    // Vanilla jumps from the ground, or, in water under 0.4 deep, from the air: the hop that takes
    // a body out over a ledge it is in the last inch of the water under.
    let jumping = controls.jump
        && jump_ticks == 0
        && !liquid
        && (body.on_ground || touching_water(fluid, pos));
    let vy = if jumping {
        JUMP_VELOCITY * block_jump_factor(solid, surface, pos, body.on_ground)
    } else if liquid && controls.jump {
        vy + SWIM_UP
    } else if liquid && controls.sneak {
        vy - SWIM_DOWN
    } else {
        vy
    };
    let jump_ticks = if jumping { JUMP_COOLDOWN } else { jump_ticks };
    // A sprint-jump pushes the body along its yaw, before the tick's input.
    let (vx, vz) = if jumping && sprinting {
        let rad = controls.yaw.to_radians();
        (
            vx - rad.sin() * SPRINT_JUMP_BOOST,
            vz + rad.cos() * SPRINT_JUMP_BOOST,
        )
    } else {
        (vx, vz)
    };
    let accel = if liquid {
        LIQUID_ACCELERATION
    } else if body.on_ground && sprinting {
        ground_acceleration() * (1.0 + SPRINT_SPEED_BOOST)
    } else if body.on_ground {
        ground_acceleration()
    } else if sprinting {
        SPRINT_AIR_ACCELERATION
    } else {
        AIR_ACCELERATION
    };
    let f = if controls.forward { FORWARD_INPUT } else { 0.0 };
    // A sneaking client scales its input by the sneaking speed, in water as on land.
    let f = if controls.sneak {
        f * SNEAKING_SPEED
    } else {
        f
    };
    let rad = controls.yaw.to_radians();
    let vx = vx - f * accel * rad.sin();
    let vz = vz + f * accel * rad.cos();
    // Sneaking, the move (never the velocity) is cut back at an edge: vanilla backs the movement
    // off before the sweep and keeps the velocity, which only a collision zeroes.
    let (mx, mz) = if controls.sneak {
        back_off_from_edge(solid, body, [vx, vy, vz])
    } else {
        (vx, vz)
    };
    let (pos2, [wx, vy, wz], landed, hit) = land_tick(solid, pos, [mx, vy, mz]);
    // `land_tick` gives the move back unless it hit something, then zero.
    let vx = if wx == mx { vx } else { 0.0 };
    let vz = if wz == mz { vz } else { 0.0 };
    // The block's speed factor ends every move (`Entity.move`), before the tick's inertia.
    let slow = block_speed_factor(solid, surface, pos2, landed);
    let (vx, vz) = (vx * slow, vz * slow);
    // The inertia is the ground the body stood on before it moved, not where it ended
    // (`LivingEntity.travelInAir` reads `onGround()` before `handleRelativeFrictionAndCalculate
    // Movement` moves it): a jump tick keeps the ground's 0.546, a landing tick the air's 0.91.
    // clojurecraft read it after the move; decision inertia-before-the-move.
    let inertia = if in_lava {
        LAVA_INERTIA
    } else if in_water {
        WATER_INERTIA
    } else if body.on_ground {
        GROUND_INERTIA
    } else {
        AIR_INERTIA
    };
    let g = if in_lava {
        LAVA_GRAVITY
    } else if in_water {
        WATER_GRAVITY
    } else {
        GRAVITY
    };
    let vy2 = if liquid {
        vy * inertia - g
    } else {
        (vy - GRAVITY) * VERTICAL_DRAG
    };
    // The one real lift out of a liquid: pressing into a bank whose top is free while the box
    // touches the fluid at all; the probe is 0.6 less what the body rose this tick.
    let vy2 = if touching
        && hit
        && free(
            solid,
            fluid,
            aabb(pos2),
            [vx, vy2 + BANK_PROBE + (pos[1] - pos2[1]), vz],
        ) {
        OUT_OF_LIQUID_IMPULSE
    } else {
        vy2
    };
    let fall = if liquid || landed {
        0.0
    } else if vy < 0.0 {
        body.fall_distance + (pos[1] - pos2[1])
    } else {
        body.fall_distance
    };
    Body {
        pos: pos2,
        vel: [vx * inertia, vy2, vz * inertia],
        on_ground: landed,
        horizontal_collision: hit,
        jump_ticks,
        fall_distance: fall,
        in_water,
        in_lava,
        eye_in_water: eye_in_water(fluid, pos2),
        minor_horizontal_collision: hit
            && minor_collision(controls.yaw, f, (pos2[0] - pos[0], pos2[2] - pos[2])),
        sprinting,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flat ground: full cubes at y = 63 and below.
    fn ground(_: i32, y: i32, _: i32) -> Option<f64> {
        (y <= 63).then_some(1.0)
    }

    fn standing() -> Body {
        Body {
            pos: [0.5, 64.0, 0.5],
            on_ground: true,
            ..Body::default()
        }
    }

    #[test]
    fn a_jump_peaks_near_1_25_blocks() {
        let mut b = standing();
        let mut peak: f64 = 64.0;
        let jump = Controls {
            jump: true,
            ..Controls::default()
        };
        let idle = Controls::default();
        for t in 0..30 {
            b = step(&ground, &no_fluid, &b, if t == 0 { &jump } else { &idle });
            peak = peak.max(b.pos[1]);
        }
        assert!((peak - 64.0 - 1.25).abs() < 0.01, "peak {peak}");
        assert!(b.on_ground && (b.pos[1] - 64.0).abs() < 1e-9);
    }

    #[test]
    fn a_ledge_needs_a_jump_a_carpet_does_not() {
        let walk = Controls {
            forward: true,
            ..Controls::default()
        };
        // A block at z = 2 on the ground stops the walk.
        let wall = |x: i32, y: i32, z: i32| {
            if y <= 63 || (y == 64 && z == 2 && x == 0) {
                Some(1.0)
            } else {
                None
            }
        };
        let mut b = standing();
        for _ in 0..40 {
            b = step(&wall, &no_fluid, &b, &walk);
        }
        assert!(b.pos[2] < 1.71, "walked through a block: {:?}", b.pos);
        // A carpet there is walked onto.
        let carpet = |x: i32, y: i32, z: i32| {
            if y <= 63 {
                Some(1.0)
            } else if y == 64 && z == 2 && x == 0 {
                Some(0.0625)
            } else {
                None
            }
        };
        let mut b = standing();
        let mut highest: f64 = 0.0;
        for _ in 0..40 {
            b = step(&carpet, &no_fluid, &b, &walk);
            highest = highest.max(b.pos[1]);
        }
        assert!(
            b.pos[2] > 3.0 && (highest - 64.0625).abs() < 1e-9,
            "did not step onto the carpet: {:?}, highest {highest}",
            b.pos
        );
    }

    /// A platform of full cubes at y = 63 for z <= 2, nothing beyond: an edge at z = 3.
    fn platform(_: i32, y: i32, z: i32) -> Option<f64> {
        (y == 63 && z <= 2).then_some(1.0)
    }

    #[test]
    fn sneaking_stops_at_an_edge_walking_falls_off_it() {
        let sneak = Controls {
            forward: true,
            sneak: true,
            ..Controls::default()
        };
        let mut b = standing();
        for _ in 0..400 {
            b = step(&platform, &no_fluid, &b, &sneak);
            assert!(b.pos[1] > 64.0 - 1e-9, "left the ground at {:?}", b.pos);
        }
        // The box (0.3 each side) may hang over the edge at z = 3 but never clear it, and
        // vanilla's 0.05 steps stop it within 0.05 of the last hold.
        assert!(
            b.pos[2] > 3.2 && b.pos[2] < 3.3,
            "stopped at the edge: z {}",
            b.pos[2]
        );
        assert!(b.on_ground);
        let walk = Controls {
            forward: true,
            ..Controls::default()
        };
        let mut b = standing();
        for _ in 0..60 {
            b = step(&platform, &no_fluid, &b, &walk);
        }
        assert!(b.pos[1] < 60.0, "walked off: {:?}", b.pos);
    }

    #[test]
    fn sneaking_walks_at_three_tenths_of_the_pace() {
        // Terminal ground speed is accel × input / (1 - inertia): the input is the only term
        // sneaking changes, so the two speeds are in the ratio of the attribute, 0.3.
        let speed = |c: Controls| {
            let mut b = standing();
            for _ in 0..200 {
                b = step(&ground, &no_fluid, &b, &c);
            }
            let before = b.pos[2];
            b = step(&ground, &no_fluid, &b, &c);
            b.pos[2] - before
        };
        let walk = speed(Controls {
            forward: true,
            ..Controls::default()
        });
        let sneak = speed(Controls {
            forward: true,
            sneak: true,
            ..Controls::default()
        });
        // Vanilla's walk is 4.317 blocks a second, its sneak 1.295.
        assert!((walk * 20.0 - 4.317).abs() < 0.01, "walk {}", walk * 20.0);
        assert!(
            (sneak * 20.0 - 1.295).abs() < 0.01,
            "sneak {}",
            sneak * 20.0
        );
        assert!((sneak / walk - SNEAKING_SPEED).abs() < 1e-9);
    }

    fn sprint() -> Controls {
        Controls {
            forward: true,
            sprint: true,
            ..Controls::default()
        }
    }

    /// The pace in blocks a second after 200 ticks of `c` on flat ground, and the body.
    fn steady(c: &Controls, food: Option<i32>) -> (f64, Body) {
        let mut b = standing();
        for _ in 0..200 {
            b = step_fed(&ground, &no_fluid, &plain_surface, &b, c, food);
        }
        let before = b.pos[2];
        b = step_fed(&ground, &no_fluid, &plain_surface, &b, c, food);
        ((b.pos[2] - before) * 20.0, b)
    }

    #[test]
    fn sprinting_runs_at_thirteen_tenths_of_the_pace() {
        // Terminal ground speed is accel × input / (1 - inertia), the acceleration ×1.3.
        let (walk, wb) = steady(
            &Controls {
                forward: true,
                ..Controls::default()
            },
            None,
        );
        let (run, rb) = steady(&sprint(), None);
        assert!((run - 5.612).abs() < 0.01, "sprint {run}");
        assert!((run / walk - (1.0 + SPRINT_SPEED_BOOST)).abs() < 1e-9);
        assert!(rb.sprinting && !wb.sprinting);
        // Sneaking refuses a start: the pace is the sneak's.
        let (sneak, sb) = steady(
            &Controls {
                sneak: true,
                ..sprint()
            },
            None,
        );
        assert!(
            !sb.sprinting && (sneak - 1.295).abs() < 0.01,
            "sneak {sneak}"
        );
    }

    #[test]
    fn a_sprint_held_into_a_crouch_keeps_the_flag_at_the_sneaks_input() {
        let (_, mut b) = steady(&sprint(), None);
        let crouch = Controls {
            sneak: true,
            ..sprint()
        };
        for _ in 0..200 {
            b = step(&ground, &no_fluid, &b, &crouch);
        }
        let before = b.pos[2];
        b = step(&ground, &no_fluid, &b, &crouch);
        let pace = (b.pos[2] - before) * 20.0;
        // 1.295 × 1.3: the sneak's input on the sprint's speed.
        assert!(b.sprinting && (pace - 1.295 * 1.3).abs() < 0.01, "{pace}");
        // Letting the wish go stops it (the bot's held key; vanilla's flag would hold).
        let walked = step(
            &ground,
            &no_fluid,
            &b,
            &Controls {
                sprint: false,
                ..crouch
            },
        );
        assert!(!walked.sprinting);
    }

    #[test]
    fn food_six_stops_the_sprint_seven_does_not() {
        let (at6, b6) = steady(&sprint(), Some(SPRINT_FOOD_FLOOR));
        let (at7, b7) = steady(&sprint(), Some(SPRINT_FOOD_FLOOR + 1));
        assert!(!b6.sprinting && (at6 - 4.317).abs() < 0.01, "food 6: {at6}");
        assert!(b7.sprinting && (at7 - 5.612).abs() < 0.01, "food 7: {at7}");
    }

    #[test]
    fn a_sprint_jump_is_pushed_along_the_yaw() {
        // From a steady sprint, the jump tick adds 0.2 along the yaw to the velocity.
        let (_, b) = steady(&sprint(), None);
        let ran = step(&ground, &no_fluid, &b, &sprint());
        let jumped = step(
            &ground,
            &no_fluid,
            &b,
            &Controls {
                jump: true,
                ..sprint()
            },
        );
        // The same tick's inertia applies to both, the ground's (both stood on it before moving).
        let boost = (jumped.vel[2] - ran.vel[2]) / GROUND_INERTIA;
        assert!((boost - SPRINT_JUMP_BOOST).abs() < 1e-9, "boost {boost}");
        assert!(jumped.vel[0].abs() < 1e-12);
        // Held, sprint-jumping keeps a pace well over the sprint's: vanilla's 7.127 b/s, worked
        // by hand over its 12-tick cycle (the jump tick at 0.1274 + the 0.2 push and the ground's
        // 0.546, then 11 air ticks at 0.02548 and 0.91). Inertia read after the move (the old,
        // clojurecraft rule) gives 8.756 (decision inertia-before-the-move). Measured over 240
        // ticks, 20 hops, after 100 to settle.
        let hopping = Controls {
            jump: true,
            ..sprint()
        };
        let mut hb = standing();
        for _ in 0..100 {
            hb = step(&ground, &no_fluid, &hb, &hopping);
        }
        let from = hb.pos[2];
        for _ in 0..240 {
            hb = step(&ground, &no_fluid, &hb, &hopping);
        }
        let hop = (hb.pos[2] - from) * 20.0 / 240.0;
        assert!(
            hb.sprinting && (hop - 7.127).abs() < 0.02,
            "sprint-jump {hop}"
        );
    }

    #[test]
    fn a_wall_stops_the_sprint_a_glancing_one_does_not() {
        // A wall across the way at z = 3: running into it ends the sprint.
        let wall = |_: i32, y: i32, z: i32| (y <= 63 || (y <= 65 && z == 3)).then_some(1.0);
        let mut b = standing();
        let mut ran = false;
        for _ in 0..40 {
            b = step(&wall, &no_fluid, &b, &sprint());
            ran |= b.sprinting;
        }
        assert!(ran && !b.sprinting && b.horizontal_collision, "{b:?}");
        // A wall along the way at x = -1, run at 5° into it: the move still points within 8° of
        // the facing, so the collision is minor and the sprint goes on.
        let side = |x: i32, y: i32, _: i32| (y <= 63 || (y <= 65 && x == -1)).then_some(1.0);
        let at = |yaw: f64| {
            let c = Controls { yaw, ..sprint() };
            let mut b = standing();
            for _ in 0..60 {
                b = step(&side, &no_fluid, &b, &c);
            }
            b
        };
        let glance = at(5.0);
        assert!(glance.horizontal_collision && glance.minor_horizontal_collision);
        assert!(glance.sprinting, "{glance:?}");
        // At 20° the collision is not minor: the sprint stops.
        let steep = at(20.0);
        assert!(steep.horizontal_collision && !steep.sprinting, "{steep:?}");
    }

    #[test]
    fn in_water_the_body_does_not_sprint() {
        let water = |_: i32, y: i32, _: i32| {
            (y == 64).then_some(Fluid {
                kind: FluidKind::Water,
                level: 0,
            })
        };
        let mut b = standing();
        for _ in 0..20 {
            b = step(&ground, &water, &b, &sprint());
        }
        assert!(b.in_water && !b.sprinting);
    }

    #[test]
    fn a_sneaking_body_still_steps_down_less_than_a_step() {
        // A carpet's height down is not an edge (under the 0.6 step): the sneak walks off it.
        let carpet = |_: i32, y: i32, z: i32| {
            if y <= 63 {
                Some(1.0)
            } else if y == 64 && z <= 0 {
                Some(0.0625)
            } else {
                None
            }
        };
        let sneak = Controls {
            forward: true,
            sneak: true,
            ..Controls::default()
        };
        let mut b = Body {
            pos: [0.5, 64.0625, 0.5],
            on_ground: true,
            ..Body::default()
        };
        for _ in 0..100 {
            b = step(&carpet, &no_fluid, &b, &sneak);
        }
        assert!(
            b.pos[2] > 2.0 && (b.pos[1] - 64.0).abs() < 1e-9,
            "walked off the carpet: {:?}",
            b.pos
        );
    }

    /// Soul sand's numbers as the game holds them: a float 0.4, widened.
    const SOUL: Surface = Surface {
        speed_factor: 0.4f32 as f64,
        jump_factor: 1.0,
        wet: false,
    };

    fn walk() -> Controls {
        Controls {
            forward: true,
            ..Controls::default()
        }
    }

    /// The distance of the 201st tick of a walk.
    fn pace(solid: &impl Solid, fluid: &impl FluidAt, surface: &impl SurfaceAt, from: Body) -> f64 {
        let mut b = from;
        for _ in 0..200 {
            b = step_on(solid, fluid, surface, &b, &walk());
        }
        let before = b.pos[2];
        step_on(solid, fluid, surface, &b, &walk()).pos[2] - before
    }

    #[test]
    fn soul_sand_slows_the_walk_to_the_games_pace() {
        // A soul sand floor: 14/16 high, so the feet stand inside its cell at y 63.875.
        let sand = |_: i32, y: i32, _: i32| match y {
            63 => Some(0.875),
            y if y < 63 => Some(1.0),
            _ => None,
        };
        let soul = |_: i32, y: i32, _: i32| if y == 63 { SOUL } else { Surface::PLAIN };
        let on_sand = Body {
            pos: [0.5, 63.875, 0.5],
            on_ground: true,
            ..Body::default()
        };
        let slow = pace(&sand, &no_fluid, &soul, on_sand);
        let plain = pace(&ground, &no_fluid, &plain_surface, standing());
        // Each tick: the input's acceleration, the move, then the velocity × factor × inertia.
        // The pace settles at accel / (1 - factor × inertia): 0.098 / (1 - 0.4 × 0.546).
        let k = f64::from(0.4f32);
        let want = FORWARD_INPUT * ground_acceleration() / (1.0 - k * GROUND_INERTIA);
        assert!((slow - want).abs() < 1e-12, "{slow} against {want}");
        // 2.51 blocks a second on soul sand, against the walk's 4.32.
        assert!((slow * 20.0 - 2.508).abs() < 0.001, "{}", slow * 20.0);
        assert!((plain * 20.0 - 4.317).abs() < 0.001, "{}", plain * 20.0);
        // The share a route's cost can read is the same number.
        assert!((pace_factor(k) - slow / plain).abs() < 1e-12);
        assert!((pace_factor(k) - 0.581).abs() < 0.001);
        assert_eq!(pace_factor(1.0), 1.0);
        // The same floor with nothing said about it is walked at the plain pace.
        let unsaid = pace(&sand, &no_fluid, &plain_surface, on_sand);
        assert!((unsaid - plain).abs() < 1e-12);
    }

    #[test]
    fn the_block_under_the_body_slows_it_when_the_feet_are_in_air() {
        // A full cube with a speed factor (no such block in the game: its own two are lower
        // than a cube, but the rule reads the block under the body all the same).
        let slow_floor = |_: i32, y: i32, _: i32| if y == 63 { SOUL } else { Surface::PLAIN };
        let slow = pace(&ground, &no_fluid, &slow_floor, standing());
        let k = f64::from(0.4f32);
        let want = FORWARD_INPUT * ground_acceleration() / (1.0 - k * GROUND_INERTIA);
        assert!((slow - want).abs() < 1e-12, "{slow} against {want}");
        // In the air over it (more than half a block up) it does nothing.
        let high = [0.5, 66.0, 0.5];
        assert_eq!(block_speed_factor(&ground, &slow_floor, high, false), 1.0);
        assert_eq!(
            block_speed_factor(&ground, &slow_floor, [0.5, 64.4, 0.5], false),
            f64::from(0.4f32),
            "within half a block of it"
        );
    }

    #[test]
    fn the_supporting_block_is_asked_when_the_body_hangs_over_an_edge() {
        // Slow blocks for z <= 0, nothing beyond z = 1: the body's centre is past the edge, over
        // air, its box still on the last block.
        let ledge = |_: i32, y: i32, z: i32| (y == 63 && z <= 0).then_some(1.0);
        let slow = |_: i32, y: i32, z: i32| {
            if y == 63 && z <= 0 {
                SOUL
            } else {
                Surface::PLAIN
            }
        };
        let over = [0.5, 64.0, 1.2];
        assert_eq!(supporting_block(&ledge, over), Some((0, 63, 0)));
        assert_eq!(below_that_affects(&ledge, over, true), (0, 63, 0));
        assert_eq!(
            block_speed_factor(&ledge, &slow, over, true),
            f64::from(0.4f32)
        );
        // Off the ground the column of the centre is asked: air.
        assert_eq!(block_speed_factor(&ledge, &slow, over, false), 1.0);
        // Of two blocks under the box, the nearer centre.
        assert_eq!(
            supporting_block(&ground, [0.9, 64.0, 0.5]),
            Some((0, 63, 0))
        );
        assert_eq!(
            supporting_block(&ground, [1.1, 64.0, 0.5]),
            Some((1, 63, 0))
        );
        assert_eq!(supporting_block(&ground, [0.5, 65.0, 0.5]), None);
    }

    #[test]
    fn water_at_the_feet_hides_the_soul_sand_under_it() {
        let wet = |_: i32, y: i32, _: i32| match y {
            63 => SOUL,
            64 => Surface {
                wet: true,
                ..Surface::PLAIN
            },
            _ => Surface::PLAIN,
        };
        assert_eq!(
            block_speed_factor(&ground, &wet, [0.5, 64.0, 0.5], true),
            1.0
        );
        let dry = |_: i32, y: i32, _: i32| if y == 63 { SOUL } else { Surface::PLAIN };
        assert_eq!(
            block_speed_factor(&ground, &dry, [0.5, 64.0, 0.5], true),
            f64::from(0.4f32)
        );
    }

    #[test]
    fn honey_halves_the_jump() {
        // Honey's jump factor is 0.5: the jump leaves at 0.21, not 0.42.
        let honey = |_: i32, y: i32, _: i32| {
            if y == 63 {
                Surface {
                    speed_factor: f64::from(0.4f32),
                    jump_factor: 0.5,
                    wet: false,
                }
            } else {
                Surface::PLAIN
            }
        };
        let jump = Controls {
            jump: true,
            ..Controls::default()
        };
        let b = step_on(&ground, &no_fluid, &honey, &standing(), &jump);
        assert!((b.pos[1] - 64.21).abs() < 1e-9, "rose {}", b.pos[1] - 64.0);
        let b = step_on(&ground, &no_fluid, &plain_surface, &standing(), &jump);
        assert!((b.pos[1] - 64.42).abs() < 1e-9);
    }

    #[test]
    fn falling_builds_fall_distance_landing_resets_it() {
        let mut b = Body {
            pos: [0.5, 70.0, 0.5],
            ..Body::default()
        };
        let mut most: f64 = 0.0;
        for _ in 0..60 {
            b = step(&ground, &no_fluid, &b, &Controls::default());
            most = most.max(b.fall_distance);
        }
        assert!((most - 6.0).abs() < 0.5, "fell {most}");
        assert_eq!(b.fall_distance, 0.0);
        assert!(b.on_ground);
    }

    #[test]
    fn in_still_water_a_body_sinks_slowly_and_swims_up() {
        let water = |_: i32, y: i32, _: i32| {
            (60..=66).contains(&y).then_some(Fluid {
                kind: FluidKind::Water,
                level: 0,
            })
        };
        let floor = |_: i32, y: i32, _: i32| (y <= 59).then_some(1.0);
        let mut b = Body {
            pos: [0.5, 63.0, 0.5],
            ..Body::default()
        };
        b = step(&floor, &water, &b, &Controls::default());
        assert!(b.in_water && b.eye_in_water);
        assert!(
            b.vel[1] < 0.0 && b.vel[1] > -0.01,
            "sinks slowly: {}",
            b.vel[1]
        );
        let up = Controls {
            jump: true,
            ..Controls::default()
        };
        let before = b.pos[1];
        for _ in 0..10 {
            b = step(&floor, &water, &b, &up);
        }
        assert!(b.pos[1] > before, "swam up");
    }
}
