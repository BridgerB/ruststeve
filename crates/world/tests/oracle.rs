//! Issue 04's first done-condition: a captured clojurecraft recording's chunk stream decodes to
//! the same block states clojurecraft saw, at 100 sampled positions in the overworld and 100 in
//! the Nether.
//!
//! `fixtures/expected.txt` and the column files were made once, outside this repository, by
//! running clojurecraft's own `chunk/decode` and `block-at` (at be85a92) over the race recording
//! named in the table's header; the columns are the recording's raw chunk bytes. This test decodes
//! the same bytes with `protocol::chunk` and the world's `state_at` and compares every position.

use std::path::Path;
use std::sync::Arc;

use protocol::S2c;
use protocol::chunk::Column;
use world::{BlockPos, Event, World};

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

/// `(dimension, x, y, z, state)` rows of the expected table.
fn expected() -> Vec<(String, i32, i32, i32, u32)> {
    let text = std::fs::read_to_string(fixtures().join("expected.txt")).unwrap();
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (
                f[0].to_owned(),
                f[1].parse().unwrap(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
            )
        })
        .collect()
}

/// A world in `dimension` holding every fixture column of that dimension, loaded through the
/// same packets the server sends.
fn world_for(dimension: &str) -> World {
    let (type_id, name) = match dimension {
        "nether" => (3, "minecraft:the_nether"),
        _ => (0, "minecraft:overworld"),
    };
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Respawn {
        dimension_type: type_id,
        dimension: name.to_owned(),
    }));
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_str().unwrap().to_owned();
        let Some(rest) = stem.strip_prefix(&format!("{dimension}_")) else {
            continue;
        };
        let (cx, cz) = rest.split_once('_').unwrap();
        let column = Column::decode(&std::fs::read(&path).unwrap())
            .unwrap_or_else(|e| panic!("{stem}: {e}"));
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: cx.parse().unwrap(),
            z: cz.parse().unwrap(),
            column: Arc::new(column),
        }));
    }
    w
}

fn check(dimension: &str, min_y: i32) {
    let w = world_for(dimension);
    let rows: Vec<_> = expected()
        .into_iter()
        .filter(|r| r.0 == dimension)
        .collect();
    assert_eq!(rows.len(), 100, "{dimension}: 100 sampled positions");
    for col in w.columns.values() {
        assert_eq!(col.min_y, min_y);
    }
    let wrong: Vec<String> = rows
        .iter()
        .filter_map(|(_, x, y, z, want)| {
            let got = w.state_at(BlockPos::new(*x, *y, *z));
            (got != Some(*want)).then(|| format!("({x}, {y}, {z}): {got:?}, clojurecraft {want}"))
        })
        .collect();
    assert!(
        wrong.is_empty(),
        "{dimension}: {} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
}

#[test]
fn overworld_matches_clojurecraft() {
    check("overworld", -64);
}

#[test]
fn nether_matches_clojurecraft() {
    check("nether", 0);
}

#[test]
fn fixture_states_are_real_blocks() {
    // Sanity: every expected state names a block in the generated tables.
    for (_, x, y, z, state) in expected() {
        assert!(
            data::block_of_state(state).is_some(),
            "({x}, {y}, {z}) state {state}"
        );
    }
}
