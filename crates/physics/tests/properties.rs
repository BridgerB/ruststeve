//! `step` never panics and never makes a non-finite body, whatever is pressed, over a random
//! small grid of cubes, carpets, water and lava.

use physics::{Body, Controls, Fluid, FluidKind, step};
use proptest::prelude::*;

/// A cell of the 8x8x8 grid around the origin: 0 air, 1 cube, 2 carpet, 3 water, 4 lava.
fn cell(grid: &[u8], x: i32, y: i32, z: i32) -> u8 {
    if !(0..8).contains(&x) || !(0..8).contains(&y) || !(0..8).contains(&z) {
        return 0;
    }
    #[allow(clippy::cast_sign_loss)]
    let i = (x * 64 + y * 8 + z) as usize;
    grid[i]
}

fn controls() -> impl Strategy<Value = Controls> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        -360.0..360.0f64,
    )
        .prop_map(|(forward, jump, sneak, yaw)| Controls {
            forward,
            jump,
            sneak,
            yaw,
        })
}

proptest! {
    #[test]
    fn the_body_stays_finite(
        grid in proptest::collection::vec(0u8..5, 512),
        levels in proptest::collection::vec(0u8..8, 512),
        start in (0.5..7.5f64, 0.5..7.5f64, 0.5..7.5f64),
        presses in proptest::collection::vec(controls(), 1..120),
    ) {
        let solid = |x, y, z| match cell(&grid, x, y, z) {
            1 => Some(1.0),
            2 => Some(0.0625),
            _ => None,
        };
        let fluid = |x: i32, y: i32, z: i32| {
            let kind = match cell(&grid, x, y, z) {
                3 => FluidKind::Water,
                4 => FluidKind::Lava,
                _ => return None,
            };
            #[allow(clippy::cast_sign_loss)]
            let level = levels[((x * 64 + y * 8 + z) as usize) % 512];
            Some(Fluid { kind, level })
        };
        let mut body = Body { pos: [start.0, start.1, start.2], ..Body::default() };
        for c in &presses {
            body = step(&solid, &fluid, &body, c);
            prop_assert!(body.pos.iter().chain(&body.vel).all(|v| v.is_finite()), "{body:?}");
            prop_assert!(body.fall_distance.is_finite());
        }
    }
}
