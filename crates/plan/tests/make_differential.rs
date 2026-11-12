//! Issue 10c's differential test: the needs planner chooses what clojurecraft's `make` chooses.
//! `tests/fixtures/make-expected.txt` holds clojurecraft's own answers (make/next-row at
//! be85a92) for 480 inventories toward the kit; the same inventories go through `plan::make`
//! with `config/goals.toml`, and every chosen act must be the same.

use data::ItemKind;
use plan::make::{Choice, Counts, Goals, Table, make};

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
        let got = match make(&table, &counts, kit) {
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
