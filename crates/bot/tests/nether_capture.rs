//! The tool that wrote `tests/fixtures/nether-exit.rsw` (docs/problems/nether-testbed.md): the
//! chunk columns round this world's Nether portal exit and on to the fortress the bots saw, as
//! the server sent them to a real bot (race 45194253932, lane 5), cut from its recording. It is
//! `#[ignore]`d: a tool run once on a runner, kept so the fixture can be cut again.
//!
//! ```text
//! NETHER_RSR=lane/rs-l05-b00.rsr NETHER_OUT=probe-out/nether-exit.rsw \
//!     cargo test -p bot --test nether_capture -- --ignored --nocapture
//! ```
//!
//! Setup data for the model: the server's side of a test. The bot under test still sees only
//! what the model sends it, by exposed faces.

use std::path::Path;

use bot::captured::{CapturedWorld, Column, FORMAT_VERSION};
use record::{Folder, Record, Recording};

fn var<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore = "a tool, not a test: cuts the Nether exit fixture from a recording (NETHER_RSR)"]
fn write_the_nether_exit_fixture() {
    let rsr = std::env::var("NETHER_RSR").expect("NETHER_RSR: the recording to cut from");
    let out = std::env::var("NETHER_OUT").unwrap_or_else(|_| "nether-exit.rsw".to_owned());
    // Lane 5 entered the Nether at server tick 24,960 and began to dig at 25,543.
    let at: u64 = var("NETHER_AT", 25_500);
    // The chunk columns kept: the portal at (22, -20) to the bricks at (61, 95), a column or
    // two to each side.
    let (x0, x1) = (var("NETHER_X0", -1), var("NETHER_X1", 5));
    let (z0, z1) = (var("NETHER_Z0", -4), var("NETHER_Z1", 6));

    let (header, records) = Recording::read_all(Path::new(&rsr)).unwrap();
    let mut folder = Folder::new();
    let mut start = 0;
    for (i, rec) in records.iter().enumerate() {
        if let Record::Checkpoint {
            event_index,
            server_ticks,
            world,
        } = rec
            && *server_ticks <= at
        {
            folder = Folder::from_checkpoint((**world).clone(), *event_index);
            start = i + 1;
        }
    }
    for rec in &records[start..] {
        if folder.world.session.server_ticks >= at {
            break;
        }
        if let Record::Event(e) = rec {
            folder.step(e).unwrap();
        }
    }
    let w = &folder.world;
    assert!(world::stage::in_nether(w), "not in the Nether at tick {at}");
    let columns: Vec<Column> = w
        .columns
        .iter()
        .filter(|(c, _)| (x0..=x1).contains(&c.x) && (z0..=z1).contains(&c.z))
        .map(|(c, column)| Column {
            x: c.x,
            z: c.z,
            body: column.packet_body(c.x, c.z).unwrap(),
        })
        .collect();
    let body = w.body.unwrap().pos;
    let bricks = world::stage::bricks_near(w, 16);
    println!(
        "recording of {} at server tick {}: the bot at {body:?}, {} columns loaded, {} kept \
         ({} bytes of bodies); {} brick cells on record, the nearest {:?}",
        header.commit,
        w.session.server_ticks,
        w.columns.len(),
        columns.len(),
        columns.iter().map(|c| c.body.len()).sum::<usize>(),
        bricks.len(),
        bricks.iter().take(8).collect::<Vec<_>>(),
    );
    let missing: Vec<(i32, i32)> = (x0..=x1)
        .flat_map(|x| (z0..=z1).map(move |z| (x, z)))
        .filter(|(x, z)| !columns.iter().any(|c| c.x == *x && c.z == *z))
        .collect();
    println!("columns asked for and not loaded: {missing:?}");
    let captured = CapturedWorld {
        format_version: FORMAT_VERSION,
        commit: header.commit.clone(),
        seed: format!(
            "race 45194253932 lane 5, the Nether at server tick {}",
            w.session.server_ticks
        ),
        landing: results::parity::Landing {
            x: body[0].floor() as i32,
            z: body[2].floor() as i32,
        },
        radius: (x1 - x0).max(z1 - z0),
        columns,
    };
    captured.write(Path::new(&out)).unwrap();
    println!(
        "wrote {out}: {} bytes",
        std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0)
    );
}
