//! bot: one bot against one server (the loop is the `bot` library). It joins and stays in the
//! world, folding everything the server sends into `World`. With `--record` every run is a file
//! `replay` can verify.
//!
//! ```text
//! cargo run -p bot -- [--host 127.0.0.1] [--port 25565] [--name rs-001] [--seconds 600]
//!                     [--record target/recordings/rs-001.rsr] [--walk square]
//! ```
//!
//! `--seconds 0` stays until the server closes. It exits 0 when the time is up while still in
//! the world, and 1 when the run ends any other way (a death, a kick, a lost connection).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use results::Ending;

fn opts() -> Result<bot::Opts, String> {
    let mut o = bot::Opts {
        host: "127.0.0.1".to_owned(),
        port: 25565,
        name: "rs-001".to_owned(),
        record: None,
        walk: false,
        cap_ticks: None,
        deadline: None,
        start: None,
        verbose: true,
    };
    let mut seconds = 600u64;
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--host" => o.host = value,
            "--port" => o.port = value.parse().map_err(|e| format!("--port: {e}"))?,
            "--name" => o.name = value,
            "--seconds" => seconds = value.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--record" => o.record = Some(PathBuf::from(value)),
            "--walk" => o.walk = value == "square",
            other => return Err(format!("unknown flag {other}")),
        }
    }
    o.deadline = (seconds > 0).then(|| Instant::now() + Duration::from_secs(seconds));
    Ok(o)
}

fn main() -> ExitCode {
    let o = match opts() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("bot: {e}");
            return ExitCode::FAILURE;
        }
    };
    let name = o.name.clone();
    let out = match bot::run(o) {
        Ok(out) => out,
        Err(e) => {
            eprintln!("bot {name}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let w = &out.world;
    println!(
        "bot {name}: {:.0} s connected, {:.0} s in play, {} keep-alives answered, {} server ticks, {} teleports (server corrections {}), {} columns, health {:?}, at {:?}, {} entities, {} inventory stacks, {} sightings, unknown ids {}, decode errors {}",
        out.stats.wall_ms as f64 / 1000.0,
        out.stats.play_ms as f64 / 1000.0,
        w.session.keep_alives_answered,
        w.session.server_ticks,
        w.session.teleports,
        w.session.teleports.saturating_sub(1),
        w.columns.len(),
        w.player.health,
        w.player.pos,
        w.entities.len(),
        w.inventory.slots.len(),
        w.memory.log().len(),
        out.stats.unknown.len(),
        out.stats.decode_errors
    );
    let in_world = w.body.is_some();
    match out.ending {
        Ending::Cut | Ending::Cap if in_world => ExitCode::SUCCESS,
        _ => ExitCode::FAILURE,
    }
}
