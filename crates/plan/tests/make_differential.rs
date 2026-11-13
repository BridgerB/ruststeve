//! Issue 10c's differential test: the needs planner chooses what clojurecraft's `make` chooses.
//! `tests/fixtures/make-expected.txt` holds clojurecraft's own answers (make/next-row at
//! be85a92) for 480 inventories toward the kit; the same inventories go through `plan::make`
//! with `config/goals.toml`, and every chosen act must be the same.

use data::ItemKind;
use plan::make::{Choice, Counts, Goals, Table, make, make_with};

#[test]
fn the_needs_planner_chooses_what_clojurecraft_chooses() {
    let goals: Goals = toml::from_str(include_str!("../../../config/goals.toml")).unwrap();
    let table = Table::new(&goals).unwrap();
    let kit = table.until("table").unwrap();
    let expected = include_str!("fixtures/make-expected.txt");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in expected.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| f[i].parse::<i32>().unwrap();
        let counts: Counts = [
            (ItemKind::OakLog, n(0)),
            (ItemKind::OakPlanks, n(1)),
            (ItemKind::Stick, n(2)),
            (ItemKind::CraftingTable, n(3)),
        ]
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .collect();
        let got = match make(&table, &counts, &|_| false, kit) {
            Choice::Done => "done".to_owned(),
            Choice::Stuck => "stuck".to_owned(),
            Choice::Act(r) => r.id.clone(),
        };
        if got != f[4] {
            wrong.push(format!("{line}: got {got}"));
        }
        checked += 1;
    }
    assert_eq!(checked, 480);
    assert!(
        wrong.is_empty(),
        "{} of 480 differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

/// Issue 11b: block needs (a table near, else place one; 3x3 recipes need it) and holds (the
/// mine row keeps its pickaxe), against clojurecraft's own answers for 1,920 inventories with and
/// without a remembered table.
#[test]
fn block_needs_and_holds_choose_what_clojurecraft_chooses() {
    let goals: Goals = toml::from_str(include_str!("../../../config/goals.toml")).unwrap();
    let table = Table::new(&goals).unwrap();
    let goal = table.until("stone-pickaxe").unwrap();
    let expected = include_str!("fixtures/make-expected-stone.txt");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in expected.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| f[i].parse::<i32>().unwrap();
        let counts: Counts = [
            (ItemKind::OakLog, n(0)),
            (ItemKind::OakPlanks, n(1)),
            (ItemKind::Stick, n(2)),
            (ItemKind::CraftingTable, n(3)),
            (ItemKind::Cobblestone, n(4)),
            (ItemKind::WoodenPickaxe, n(5)),
        ]
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .collect();
        let near = n(6) == 1;
        let near = move |b: data::BlockKind| near && b == data::BlockKind::CraftingTable;
        let got = match make(&table, &counts, &near, goal) {
            Choice::Done => "done".to_owned(),
            Choice::Stuck => "stuck".to_owned(),
            Choice::Act(r) => r.id.clone(),
        };
        if got != f[7] {
            wrong.push(format!("{line}: got {got}"));
        }
        checked += 1;
    }
    assert_eq!(checked, 1920);
    assert!(
        wrong.is_empty(),
        "{} of 1920 differ:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Issue 13b: fuel needs, smelt rows and spares, toward the iron kit, against clojurecraft's own
/// answers for 12,288 inventories with and without a remembered table and furnace.
#[test]
fn fuel_smelting_and_spares_choose_what_clojurecraft_chooses() {
    let goals: Goals = toml::from_str(include_str!("../../../config/goals.toml")).unwrap();
    let table = Table::new(&goals).unwrap();
    let goal = table.until("iron-kit").unwrap();
    let expected = include_str!("fixtures/make-expected-iron.txt");
    let mut checked = 0;
    let mut wrong = Vec::new();
    for line in expected.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let n = |i: usize| f[i].parse::<i32>().unwrap();
        let counts: Counts = [
            (ItemKind::OakLog, n(0)),
            (ItemKind::OakPlanks, n(1)),
            (ItemKind::Stick, n(2)),
            (ItemKind::Cobblestone, n(3)),
            (ItemKind::Coal, n(4)),
            (ItemKind::RawIron, n(5)),
            (ItemKind::IronIngot, n(6)),
            (ItemKind::Gravel, n(7)),
            (ItemKind::Flint, n(8)),
            (ItemKind::CraftingTable, n(9)),
            (ItemKind::StonePickaxe, 1),
        ]
        .into_iter()
        .filter(|(_, c)| *c > 0)
        .collect();
        let (table_near, furnace_near) = (n(10) == 1, n(11) == 1);
        let near = move |b: data::BlockKind| match b {
            data::BlockKind::CraftingTable => table_near,
            data::BlockKind::Furnace => furnace_near,
            _ => false,
        };
        let got = match make_with(&table, &counts, &near, false, goal) {
            Choice::Done => "done".to_owned(),
            Choice::Stuck => "stuck".to_owned(),
            Choice::Act(r) => match r.act {
                Some(plan::make::Act::Gather) => "gather".to_owned(),
                _ => r.id.clone(),
            },
        };
        if got != f[12] {
            wrong.push(format!("{line}: got {got}"));
        }
        checked += 1;
    }
    assert_eq!(checked, 12288);
    assert!(
        wrong.is_empty(),
        "{} of 12288 differ:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(30)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
