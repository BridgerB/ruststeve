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
//! gives the fluid there and its level.

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
}

/// What is pressed this tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Controls {
    pub forward: bool,
    pub jump: bool,
    pub sneak: bool,
    /// Degrees, Notchian (south 0, west +90).
    pub yaw: f64,
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

/// One tick of movement: the body after `controls` for a tick in this world.
pub fn step(solid: &impl Solid, fluid: &impl FluidAt, body: &Body, controls: &Controls) -> Body {
    let pos = body.pos;
    let b = aabb(pos);
    let in_water = in_fluid(fluid, FluidKind::Water, WATER_CONTRACT, b);
    let in_lava = in_fluid(fluid, FluidKind::Lava, LAVA_CONTRACT, b);
    let liquid = in_water || in_lava;
    let touching = liquid || touching_fluid(fluid, pos);
    let [vx, vy, vz] = body.vel.map(squash);
    let jump_ticks = 0.max(body.jump_ticks - 1);
    // Vanilla jumps from the ground, or, in water under 0.4 deep, from the air: the hop that takes
    // a body out over a ledge it is in the last inch of the water under.
    let jumping = controls.jump
        && jump_ticks == 0
        && !liquid
        && (body.on_ground || touching_water(fluid, pos));
    let vy = if jumping {
        JUMP_VELOCITY
    } else if liquid && controls.jump {
        vy + SWIM_UP
    } else if liquid && controls.sneak {
        vy - SWIM_DOWN
    } else {
        vy
    };
    let jump_ticks = if jumping { JUMP_COOLDOWN } else { jump_ticks };
    let accel = if liquid {
        LIQUID_ACCELERATION
    } else if body.on_ground {
        ground_acceleration()
    } else {
        AIR_ACCELERATION
    };
    let f = if controls.forward { FORWARD_INPUT } else { 0.0 };
    let rad = controls.yaw.to_radians();
    let vx = vx - f * accel * rad.sin();
    let vz = vz + f * accel * rad.cos();
    let (pos2, [vx, vy, vz], landed, hit) = land_tick(solid, pos, [vx, vy, vz]);
    let inertia = if in_lava {
        LAVA_INERTIA
    } else if in_water {
        WATER_INERTIA
    } else if landed {
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
