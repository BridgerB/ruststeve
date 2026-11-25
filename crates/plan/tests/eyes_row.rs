//! The `eyes` goal row on the real goal table (issue 19c, docs/problems/eyes-throw.md): the needs
//! planner makes the eyes the held pearls allow from rods in the 2x2 grid, and keeps a rod it does
//! not need.

use data::{BlockKind, ItemKind};
use plan::eyes::eyes_row;
use plan::make::{Act, Choice, Counts, Goals, Table, Wear, make_with_wear};

fn real_table() -> Table {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap()
}

/// Run the planner's crafts on `held` until the row is done (nothing near: no table), each craft
/// taking its needs and giving its provides; the crafts made, in order, and what is held after.
fn craft_all(held: &[(ItemKind, i32)]) -> (Vec<String>, Counts) {
    let t = real_table();
    let mut c: Counts = held.iter().copied().collect();
    let mut made = Vec::new();
    let near = |_: BlockKind| false;
    for _ in 0..64 {
        let Some(row) = eyes_row(&t, &c) else {
            return (made, c);
        };
        let r = match make_with_wear(&t, &c, &near, false, &Wear::default(), &row) {
            Choice::Done => return (made, c),
            Choice::Stuck => panic!("stuck with {c:?}"),
            Choice::Act(r) => r,
        };
        match r.act {
            Some(Act::Craft(rec)) => {
                assert!(plan::make::fits_grid(rec), "{} needs a table", rec.id)
            }
            other => panic!("not a craft: {} {other:?}", r.id),
        }
        made.push(r.id.clone());
        for (need, n) in &r.needs {
            let mut left = *n;
            for k in &need.items {
                let have = c.get(k).copied().unwrap_or(0);
                let take = have.min(left);
                c.insert(*k, have - take);
                left -= take;
            }
            assert_eq!(left, 0, "{} short of {}", r.id, need.key);
        }
        for (need, n) in &r.provides {
            *c.entry(need.items[0]).or_default() += n;
        }
    }
    panic!("no end after 64 crafts: {c:?}");
}

fn n(c: &Counts, k: ItemKind) -> i32 {
    c.get(&k).copied().unwrap_or(0)
}

#[test]
fn twelve_pearls_and_seven_rods_make_twelve_eyes_and_keep_a_rod() {
    let (made, c) = craft_all(&[(ItemKind::EnderPearl, 12), (ItemKind::BlazeRod, 7)]);
    assert_eq!(n(&c, ItemKind::EnderEye), 12, "{made:?}");
    assert_eq!(n(&c, ItemKind::BlazeRod), 1, "{made:?}");
    assert_eq!(n(&c, ItemKind::EnderPearl), 0);
    assert_eq!(n(&c, ItemKind::BlazePowder), 0);
    assert_eq!(
        made.iter().filter(|m| *m == "craft:blaze_powder").count(),
        6
    );
    assert_eq!(made.iter().filter(|m| *m == "craft:ender_eye").count(), 12);
}

#[test]
fn fourteen_pearls_make_fourteen_eyes_from_all_seven_rods() {
    let (_, c) = craft_all(&[(ItemKind::EnderPearl, 14), (ItemKind::BlazeRod, 7)]);
    assert_eq!(n(&c, ItemKind::EnderEye), 14);
    assert_eq!(n(&c, ItemKind::BlazeRod), 0);
}

#[test]
fn eyes_held_count_toward_the_cap_and_nothing_is_made_past_it() {
    let (made, c) = craft_all(&[
        (ItemKind::EnderEye, 13),
        (ItemKind::EnderPearl, 3),
        (ItemKind::BlazeRod, 2),
    ]);
    assert_eq!(n(&c, ItemKind::EnderEye), 14, "{made:?}");
    assert_eq!(n(&c, ItemKind::EnderPearl), 2);
    assert_eq!(n(&c, ItemKind::BlazeRod), 1);
    let (made, _) = craft_all(&[(ItemKind::EnderEye, 14), (ItemKind::EnderPearl, 2)]);
    assert!(made.is_empty());
    let (made, _) = craft_all(&[(ItemKind::BlazeRod, 7)]);
    assert!(made.is_empty(), "no pearls: nothing");
}
