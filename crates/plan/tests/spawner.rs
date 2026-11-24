//! Spawner wall 2 (race 45334573113, lane 1): with the spawner at (129, 64, 159) on record, a bot
//! backed down its own tunnel to (46, 60, 133), refused the spawner after three stalls and
//! explored to (8, 55, 63). The way on is the trail mark nearest the spawner.

use plan::trail_toward;

const SPAWNER: (i32, i32, i32) = (129, 64, 159);

#[test]
fn the_trail_mark_nearest_the_spawner_is_the_way_back() {
    // The lane's senses rows, as trail marks.
    let trail = [
        [22.5, 55.0, 129.5],
        [30.5, 63.0, 130.5],
        [41.5, 66.0, 133.5],
        [51.5, 64.0, 133.5],
        [55.5, 63.0, 134.5],
        [46.5, 60.0, 133.5],
    ];
    let marks = trail_toward(&trail, [46.5, 60.0, 133.5], SPAWNER);
    assert_eq!(marks.first(), Some(&[55.5, 63.0, 134.5]));
    assert_eq!(marks.len(), 2, "only marks well nearer than the feet");
}

#[test]
fn no_mark_nearer_than_the_feet_is_no_way_back() {
    let trail = [[22.5, 55.0, 129.5], [55.5, 63.0, 134.5]];
    // Standing at the nearest mark, and two blocks past it.
    assert!(trail_toward(&trail, [55.5, 63.0, 134.5], SPAWNER).is_empty());
    assert!(trail_toward(&trail, [57.5, 63.0, 134.5], SPAWNER).is_empty());
    assert!(trail_toward(&[], [0.5, 64.0, 0.5], SPAWNER).is_empty());
}

/// The same wall in race 45336326999 (lanes 0 and 1): a retreat from a skeleton took the bot from
/// (57, 64, 138), the nearest its dig had come, back to (43, 64, 133); three legs up its own
/// tunnel were three stalls against that mark and the spawner was refused.
#[test]
fn a_bot_well_behind_its_nearest_is_pushed_back() {
    // (57, 64, 138) to the spawner: 75 blocks.
    let best = 75.0;
    assert!(plan::pushed_back([43.5, 64.0, 133.5], SPAWNER, best));
    assert!(
        !plan::pushed_back([54.5, 64.0, 138.5], SPAWNER, best),
        "three steps short of the mark is one leg's work, not a push"
    );
    assert!(!plan::pushed_back([60.5, 64.0, 138.5], SPAWNER, best));
}
