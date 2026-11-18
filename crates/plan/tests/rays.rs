//! The scoop's exact sight line (`scoop::ray_cells`) and the station's pure parts
//! (`station::reach_sources`, `station::kept`): docs/problems/portal.md walls 18 and 27.

use plan::build::{Facing, Frame};
use plan::scoop::ray_cells;
use plan::station::{REACH, kept, reach_sources};

#[test]
fn a_ray_along_one_axis_enters_each_cell_once() {
    assert_eq!(
        ray_cells([0.5, 0.5, 0.5], [3.5, 0.5, 0.5]),
        [(1, 0, 0), (2, 0, 0), (3, 0, 0)]
    );
    assert_eq!(
        ray_cells([0.5, 0.5, 0.5], [0.5, -2.5, 0.5]),
        [(0, -1, 0), (0, -2, 0), (0, -3, 0)]
    );
}

#[test]
fn a_ray_inside_one_cell_enters_none() {
    assert!(ray_cells([0.2, 0.2, 0.2], [0.8, 0.9, 0.4]).is_empty());
}

/// Race 44041169156, rs-l00-b04 at event 117544: the eye's line to the top of the lava source
/// (-114, -55, 32) crossed x -113 at y -53.02, inside the top corner of the deepslate at
/// (-113, -54, 32). Forty samples along it saw only air; the server took the deepslate and the
/// bucket filled nothing, eight times.
#[test]
fn the_grazed_corner_of_race_44041169156_is_entered() {
    let eye = [-112.283_632_488_271_79, -51.38, 32.501_795_493_901_376];
    let aim = [-113.5, -54.15, 32.5];
    let cells = ray_cells(eye, aim);
    assert_eq!(
        cells,
        [
            (-113, -53, 32),
            (-113, -54, 32),
            (-114, -54, 32),
            (-114, -55, 32)
        ]
    );
}

#[test]
fn a_ray_ends_in_the_cell_of_its_end_and_never_past_it() {
    let cells = ray_cells([0.5, 1.5, 0.5], [2.9, 0.1, 1.2]);
    assert_eq!(cells.last(), Some(&(2, 0, 1)));
    // Each step moves one cell along one axis.
    let mut at = (0, 1, 0);
    for c in &cells {
        let moved = (c.0 - at.0).abs() + (c.1 - at.1).abs() + (c.2 - at.2).abs();
        assert_eq!(moved, 1, "{at:?} to {c:?}");
        at = *c;
    }
}

#[test]
fn a_stand_reaches_sources_under_its_feet_nearest_first() {
    let stand = (0, 10, 0);
    let sources = [
        (3, 9, 0),  // 3.7 from the eye
        (1, 9, 0),  // 2.3: the nearest
        (1, 10, 0), // level with the feet: never
        (1, 11, 0), // over the feet: never
        (6, 9, 0),  // 5.4: out of reach
        (0, 5, 0),  // straight down, 6.1: out of reach
    ];
    assert_eq!(reach_sources(&sources, stand), [(1, 9, 0), (3, 9, 0)]);
    // Every source kept is within the reach, measured from the eye at the stand's middle.
    let eye = [0.5, 10.0 + physics::EYE_HEIGHT, 0.5];
    for s in reach_sources(&sources, stand) {
        let c = [
            f64::from(s.0) + 0.5,
            f64::from(s.1) + 0.5,
            f64::from(s.2) + 0.5,
        ];
        let d = (0..3).map(|i| (eye[i] - c[i]).powi(2)).sum::<f64>().sqrt();
        assert!(d <= REACH, "{s:?} at {d}");
    }
}

/// Race 44159281696, rs-l00-b06 at event 31068: the feet in (-18, -51, -66), the box 0.16 into
/// (-18, -50, -67) on z; the server refused a block there four times.
#[test]
fn the_box_of_race_44159281696_overlaps_the_cell_its_feet_are_not_in() {
    let body = physics::Body {
        pos: [-17.53, -51.0, -65.86],
        ..physics::Body::default()
    };
    assert!(plan::cast::overlaps(&body, (-18, -50, -67)));
    assert!(plan::cast::overlaps(&body, (-18, -51, -66)));
    // A box touching a face does not overlap: standing on a block, beside a wall.
    let on = physics::Body {
        pos: [0.5, 1.0, 0.5],
        ..physics::Body::default()
    };
    assert!(!plan::cast::overlaps(&on, (0, 0, 0)));
    assert!(!plan::cast::overlaps(&on, (1, 1, 0)));
    assert!(!plan::cast::overlaps(&on, (0, 3, 0)));
}

/// Kitlava race 44236062785, place 0 at event 119456: the body at (26.5, -52, -71.44) stood on
/// (26, -53, -72), dug it and fell into the lava sea's cave.
#[test]
fn the_body_of_race_44236062785_stands_on_the_block_it_dug() {
    let body = physics::Body {
        pos: [26.5, -52.0, -71.44],
        ..physics::Body::default()
    };
    assert!(plan::cast::standing_on(&body, (26, -53, -72)));
    assert!(!plan::cast::standing_on(&body, (26, -53, -74)));
    assert!(!plan::cast::standing_on(&body, (26, -52, -72)));
}

#[test]
fn the_kept_rock_is_the_frames_plane_and_the_row_behind_it() {
    let frame = Frame {
        origin: (10, 60, 20),
        facing: Facing::South,
    };
    let cells = kept(frame);
    // Six wide, seven high, two deep.
    assert_eq!(cells.len(), 6 * 7 * 2);
    assert!(cells.contains(&frame.at((0, 0, 0))));
    assert!(cells.contains(&frame.at((-1, -1, -1))));
    assert!(cells.contains(&frame.at((4, 5, 0))));
    // The room in front of the frame is never kept.
    assert!(!cells.contains(&frame.at((0, 0, 1))));
    assert!(!cells.contains(&frame.at((2, 0, 2))));
}

/// Wall 46: the step up toward an op over the feet goes one column along the longer axis, one up;
/// right under it, east. The race's case: feet (3, 54, 112), op (3, 60, 114).
#[test]
fn a_step_up_goes_toward_the_op_along_the_longer_axis() {
    use plan::station::stair_up_step;
    assert_eq!(stair_up_step((3, 54, 112), (3, 60, 114)), (3, 55, 113));
    assert_eq!(stair_up_step((0, 10, 0), (5, 14, -2)), (1, 11, 0));
    assert_eq!(stair_up_step((0, 10, 0), (-1, 14, 4)), (0, 11, 1));
    assert_eq!(stair_up_step((0, 10, 0), (0, 14, 0)), (1, 11, 0));
}
