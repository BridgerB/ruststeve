//! Issue 09a: ruststeve's pathfinder answers as clojurecraft's `path/plan` does, route for route,
//! over terrain captured from the race seed. `fixtures/expected.txt` was written once, outside
//! this repository, by clojurecraft's own `path/plan` (be85a92) over `fixtures/queries.txt`.

use std::collections::BTreeMap;
use std::sync::Arc;

use plan::path::{Cell, Goal, Status, plan};
use protocol::S2c;
use protocol::chunk::Column;
use world::{Event, World};

fn dir() -> String {
    format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"))
}

/// The world of one fixture set, the player standing at `from`.
fn world(name: &str, from: Cell) -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    }));
    for e in std::fs::read_dir(dir()).unwrap().flatten() {
        let file = e.file_name().to_string_lossy().into_owned();
        let Some(rest) = file.strip_prefix(&format!("{name}_")) else {
            continue;
        };
        let parts: Vec<i32> = rest
            .trim_end_matches(".bin")
            .split('_')
            .map(|p| p.parse().unwrap())
            .collect();
        let column = Column::decode(&std::fs::read(e.path()).unwrap()).unwrap();
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: parts[0],
            z: parts[1],
            column: Arc::new(column),
        }));
    }
    w.step(&Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x: f64::from(from.0) + 0.5,
        y: f64::from(from.1),
        z: f64::from(from.2) + 0.5,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    }));
    w
}

fn goal(w: &[&str]) -> Goal {
    let n: Vec<f64> = w[1..].iter().map(|v| v.parse().unwrap()).collect();
    let cell = |i: usize| (n[i] as i32, n[i + 1] as i32, n[i + 2] as i32);
    match w[0] {
        "near" => Goal::Near {
            pos: cell(0),
            range: n[3],
        },
        "block" => Goal::Block { pos: cell(0) },
        "xz" => Goal::Xz {
            pos: (n[0] as i32, 0, n[1] as i32),
        },
        "away" => Goal::Away {
            pos: cell(0),
            range: n[3],
        },
        "afar" => Goal::Afar {
            pos: cell(0),
            range: n[3],
        },
        other => panic!("goal {other}"),
    }
}

fn cell(s: &str) -> Cell {
    let v: Vec<i32> = s.split(',').map(|p| p.parse().unwrap()).collect();
    (v[0], v[1], v[2])
}

#[test]
fn every_route_equals_clojurecraft() {
    let expected: BTreeMap<String, Vec<String>> =
        std::fs::read_to_string(format!("{}/expected.txt", dir()))
            .unwrap()
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let w: Vec<String> = l.split_whitespace().map(str::to_owned).collect();
                (w[0].clone(), w[1..].to_vec())
            })
            .collect();
    let queries = std::fs::read_to_string(format!("{}/queries.txt", dir())).unwrap();
    let mut wrong = Vec::new();
    let mut n = 0;
    for l in queries
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
    {
        let w: Vec<&str> = l.split_whitespace().collect();
        let from = (
            w[2].parse().unwrap(),
            w[3].parse().unwrap(),
            w[4].parse().unwrap(),
        );
        let r = plan(
            &world(w[1], from),
            from,
            &goal(&w[6..]),
            w[5].parse().unwrap(),
        );
        let want = &expected[w[0]];
        let status = match r.status {
            Status::Found => "found",
            Status::Partial => "partial",
            Status::None => "none",
        };
        let cost: f64 = want[1].parse().unwrap();
        let wps: Vec<Cell> = want
            .get(3)
            .map_or(Vec::new(), |s| s.split(';').map(cell).collect());
        let mut diffs = Vec::new();
        if status != want[0] {
            diffs.push(format!("status {status} vs {}", want[0]));
        }
        if r.cost != cost {
            diffs.push(format!("cost {:?} vs {cost:?}", r.cost));
        }
        if r.from
            .map(|c| format!("{},{},{}", c.0, c.1, c.2))
            .as_deref()
            != Some(want[2].as_str())
        {
            diffs.push(format!("from {:?} vs {}", r.from, want[2]));
        }
        if r.waypoints != wps {
            let first = r.waypoints.iter().zip(&wps).position(|(a, b)| a != b);
            diffs.push(format!(
                "waypoints differ ({} vs {}, first at {first:?})",
                r.waypoints.len(),
                wps.len()
            ));
        }
        if !diffs.is_empty() {
            wrong.push(format!("{}: {}", w[0], diffs.join("; ")));
        }
        n += 1;
    }
    assert_eq!(n, expected.len(), "every query answered");
    assert!(
        wrong.is_empty(),
        "{} of {n} routes differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}
