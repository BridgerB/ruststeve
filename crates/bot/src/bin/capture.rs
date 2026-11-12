//! capture: real worlds for the server model (brain/ruststeve/design/parity-table.md). A
//! spectator joins a real server, is teleported over each landing in `config/landings.toml` by
//! RCON, and keeps every `level_chunk_with_light` body until every column within the radius
//! has arrived; each landing becomes one file, byte for byte as the server sent it.
//!
//! ```text
//! RUSTSTEVE_RCON_PASSWORD=... cargo run -p bot --bin capture -- --out target/worlds \
//!     [--landings config/landings.toml] [--radius 4] [--host 127.0.0.1] [--port 25565] [--rcon-port 25575]
//! ```
//!
//! A spectator changes nothing in the world, so the terrain captured is the terrain the race's
//! bots meet on the same seed. The capturing client never ticks; after each teleport it reports
//! its new position once, because the server re-centres a player's chunks only when the client
//! reports a move (`handleMovePlayer` calls `ServerChunkCache.move`), not on the teleport.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use bot::captured::{CapturedWorld, Column, FORMAT_VERSION};
use data::generated::packets::PlayClientbound;
use net::Inbound;
use protocol::{Out, State};
use results::parity::Landings;
use world::{Effect, Event, World};

const NAME: &str = "rs-capture";
/// How long one landing may take before the capture gives up on it.
const LANDING_LIMIT: Duration = Duration::from_secs(120);

struct Args {
    out: PathBuf,
    landings: String,
    radius: i32,
    host: String,
    port: u16,
    rcon_port: u16,
    seed: String,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        out: PathBuf::from("target/worlds"),
        landings: "config/landings.toml".to_owned(),
        radius: 4,
        host: "127.0.0.1".to_owned(),
        port: 25565,
        rcon_port: 25575,
        seed: "typecraft".to_owned(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let v = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--out" => a.out = PathBuf::from(v),
            "--landings" => a.landings = v,
            "--radius" => a.radius = v.parse().map_err(|e| format!("--radius: {e}"))?,
            "--host" => a.host = v,
            "--port" => a.port = v.parse().map_err(|e| format!("--port: {e}"))?,
            "--rcon-port" => a.rcon_port = v.parse().map_err(|e| format!("--rcon-port: {e}"))?,
            "--seed" => a.seed = v,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

/// Pump the connection: fold every packet, answer what the protocol needs, keep chunk bodies.
/// Returns once `done` holds or the limit passes.
fn pump(
    conn: &net::Connection,
    world: &mut World,
    chunks: &mut BTreeMap<(i32, i32), Vec<u8>>,
    limit: Duration,
    done: impl Fn(&World, &BTreeMap<(i32, i32), Vec<u8>>) -> bool,
) -> Result<bool, String> {
    let level_chunk = PlayClientbound::LevelChunkWithLight.id();
    let until = Instant::now() + limit;
    while Instant::now() < until {
        if done(world, chunks) {
            return Ok(true);
        }
        let msg = match conn.inbound.recv_timeout(Duration::from_millis(100)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return Err("the connection closed".to_owned()),
        };
        let (packet, raw) = match msg {
            Inbound::Packet { packet, raw } => (packet, raw),
            Inbound::DecodeError { state, id, error } => {
                return Err(format!("cannot decode {state:?} id {id}: {error}"));
            }
            Inbound::Closed(why) => return Err(format!("closed: {why}")),
        };
        if let protocol::S2c::LevelChunkWithLight { x, z, .. } = &packet
            && raw.state == State::Play
            && raw.id == level_chunk
        {
            chunks.insert((*x, *z), raw.body.clone());
        }
        for e in world.step(&Event::Packet(packet)) {
            let out = match e {
                Effect::Send(p) => Out::Send(p),
                Effect::SetCompression(t) => Out::SetCompression(t),
            };
            conn.outbound
                .send(out)
                .map_err(|_| "the writer thread stopped".to_owned())?;
        }
    }
    Ok(done(world, chunks))
}

fn capture(a: &Args) -> Result<(), String> {
    let landings = Landings::read(&a.landings)?;
    let password =
        std::env::var("RUSTSTEVE_RCON_PASSWORD").map_err(|_| "set RUSTSTEVE_RCON_PASSWORD")?;
    std::fs::create_dir_all(&a.out).map_err(|e| format!("{}: {e}", a.out.display()))?;
    let addr = format!("{}:{}", a.host, a.port);
    let conn = net::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let protocol = i32::try_from(data::VERSION.protocol).map_err(|e| e.to_string())?;
    for p in protocol::session::opening(protocol, &a.host, a.port, NAME) {
        conn.outbound
            .send(Out::Send(p))
            .map_err(|_| "the writer thread stopped".to_owned())?;
    }
    let mut world = World::new();
    let mut chunks = BTreeMap::new();
    let joined = pump(
        &conn,
        &mut world,
        &mut chunks,
        Duration::from_secs(60),
        |w, _| w.session.teleports > 0,
    )?;
    if !joined {
        return Err("never placed in the world".to_owned());
    }
    let mut rcon = rcon::Rcon::connect(&format!("{}:{}", a.host, a.rcon_port), &password)?;
    println!(
        "capture: {}",
        rcon.command(&format!("gamemode spectator {NAME}"))?
    );
    for (i, l) in landings.landing.iter().enumerate() {
        let (cx, cz) = (l.x >> 4, l.z >> 4);
        let wanted: Vec<(i32, i32)> = (-a.radius..=a.radius)
            .flat_map(|dx| (-a.radius..=a.radius).map(move |dz| (cx + dx, cz + dz)))
            .collect();
        // Columns still held from an earlier landing are not sent again (the client has them); a
        // spectator changes nothing, so their bytes are still the world's.
        println!(
            "capture: landing {i} at {} {}: {}",
            l.x,
            l.z,
            rcon.command(&format!("tp {NAME} {} 200 {}", l.x, l.z))?
        );
        let target = [f64::from(l.x) + 0.5, 200.0, f64::from(l.z) + 0.5];
        let arrived = pump(
            &conn,
            &mut world,
            &mut chunks,
            Duration::from_secs(30),
            |w, _| {
                w.player.pos.is_some_and(|p| {
                    (p[0] - target[0]).abs() < 1e-6 && (p[2] - target[2]).abs() < 1e-6
                })
            },
        )?;
        if !arrived {
            return Err(format!("landing {i}: the teleport never arrived"));
        }
        conn.outbound
            .send(Out::Send(protocol::C2s::MovePlayerPosRot {
                x: target[0],
                y: target[1],
                z: target[2],
                yaw: 0.0,
                pitch: 0.0,
                flags: 0,
            }))
            .map_err(|_| "the writer thread stopped".to_owned())?;
        let got = pump(&conn, &mut world, &mut chunks, LANDING_LIMIT, |_, c| {
            wanted.iter().all(|k| c.contains_key(k))
        })?;
        if !got {
            let missing = wanted.iter().filter(|k| !chunks.contains_key(k)).count();
            return Err(format!("landing {i}: {missing} columns never arrived"));
        }
        let file = CapturedWorld {
            format_version: FORMAT_VERSION,
            commit: record::COMMIT.to_owned(),
            seed: a.seed.clone(),
            landing: *l,
            radius: a.radius,
            columns: wanted
                .iter()
                .filter_map(|k| {
                    chunks.get(k).map(|body| Column {
                        x: k.0,
                        z: k.1,
                        body: body.clone(),
                    })
                })
                .collect(),
        };
        let path = CapturedWorld::path(&a.out, i);
        file.write(&path)?;
        println!(
            "capture: landing {i}: {} columns -> {}",
            file.columns.len(),
            path.display()
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("capture: {e}");
            return ExitCode::FAILURE;
        }
    };
    match capture(&a) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("capture: {e}");
            ExitCode::FAILURE
        }
    }
}
