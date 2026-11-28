//! A row with a position tolerance differs in `position` when the bodies ended further apart than
//! it (issue 56), and a row without one compares only its class.

use std::collections::BTreeMap;

use results::Source;
use results::parity::{Health, MomentResult, Outcome, compare, compare_within, render};

fn run(id: &str, source: Source, pos: [f64; 3]) -> MomentResult {
    MomentResult {
        moment: id.to_owned(),
        source,
        commit: "0".to_owned(),
        outcome: Some(Outcome {
            cell: pos.map(|v| v.floor() as i32),
            on_ground: true,
            in_water: false,
            corrected: false,
            health: Health::Full,
            inventory: BTreeMap::new(),
            grid: 0,
            food: Some(20),
            kills: 0,
            blocks: Vec::new(),
            mobs: Vec::new(),
            eye: None,
            frames: None,
        }),
        corrections: 0,
        ending: "done".to_owned(),
        final_pos: Some(pos),
    }
}

#[test]
fn a_gap_over_the_rows_tolerance_differs_in_position() {
    // Both in cell 10: the class matches, but 0.4 apart.
    let real = [
        run("near", Source::Real, [0.5, 64.0, 10.1]),
        run("far", Source::Real, [0.5, 64.0, 10.1]),
        run("free", Source::Real, [0.5, 64.0, 10.1]),
    ];
    let sim = [
        run("near", Source::Sim, [0.5, 64.0, 10.2]),
        run("far", Source::Sim, [0.5, 64.0, 10.5]),
        run("free", Source::Sim, [0.5, 64.0, 10.5]),
    ];
    let within: BTreeMap<String, f64> = [("near", 0.25), ("far", 0.25)]
        .map(|(k, t)| (k.to_owned(), t))
        .into_iter()
        .collect();
    let rows = compare_within(&real, &sim, &within);
    let row = |id: &str| {
        rows.iter()
            .find(|r| r.moment == id)
            .map(|r| (&r.differ, r.gap))
    };
    let (near, g) = row("near").unwrap();
    assert!(
        near.is_empty() && (g.unwrap() - 0.1).abs() < 1e-9,
        "{near:?} {g:?}"
    );
    let (far, g) = row("far").unwrap();
    assert!(
        *far == vec!["position"] && (g.unwrap() - 0.4).abs() < 1e-9,
        "{far:?}"
    );
    let (free, g) = row("free").unwrap();
    assert!(free.is_empty() && g.is_none(), "{free:?}");
    // `compare` is `compare_within` with no tolerances.
    assert!(compare(&real, &sim).iter().all(|r| r.differ.is_empty()));
    let (text, pass) = render(&rows, 2);
    assert!(pass && text.contains("position (gap 0.400)"), "{text}");
}
