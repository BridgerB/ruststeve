//! bot: one bot against one server. For now it joins and stays in the world (issue 03): the
//! handshake, offline login, configuration, then play, answering keep-alives and pings.
//!
//! ```text
//! cargo run -p bot -- [--host 127.0.0.1] [--port 25565] [--name rs-001] [--seconds 600]
//! ```
//!
//! `--seconds 0` stays until the server closes. It exits 0 when the time is up while still in
//! play, and 1 when the server disconnects it first.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use net::Inbound;
use protocol::session::opening;
use protocol::{Out, S2c, Session, State};

/// How often the bot prints a status line.
const STATUS_EVERY: Duration = Duration::from_secs(10);

struct Args {
    host: String,
    port: u16,
    name: String,
    seconds: u64,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        host: "127.0.0.1".to_owned(),
        port: 25565,
        name: "rs-001".to_owned(),
        seconds: 600,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--host" => a.host = value,
            "--port" => a.port = value.parse().map_err(|e| format!("--port: {e}"))?,
            "--name" => a.name = value,
            "--seconds" => a.seconds = value.parse().map_err(|e| format!("--seconds: {e}"))?,
            other => return Err(format!("unknown flag {other}")),
        }
    }
    Ok(a)
}

/// A text component's printable characters: enough to read a kick reason in a log.
fn printable(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn main() -> ExitCode {
    let a = match args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("bot: {e}");
            return ExitCode::FAILURE;
        }
    };
    match run(&a) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bot {}: {e}", a.name);
            ExitCode::FAILURE
        }
    }
}

/// What the bot counts while it runs, for its status and final lines.
struct Stats {
    started: Instant,
    play_since: Option<Instant>,
    per_state: BTreeMap<State, u64>,
    unknown: BTreeMap<(State, u32), u64>,
    decode_errors: u64,
}

impl Stats {
    fn summary(&self, session: &Session, why: &str) -> String {
        format!(
            "{why}: {:.0} s connected, {:.0} s in play, {} keep-alives answered, {} server ticks, {} teleports, packets {:?}, unknown ids {}, decode errors {}",
            self.started.elapsed().as_secs_f64(),
            self.play_since.map_or(0.0, |t| t.elapsed().as_secs_f64()),
            session.keep_alives_answered,
            session.server_ticks,
            session.teleports,
            self.per_state,
            self.unknown.len(),
            self.decode_errors
        )
    }
}

fn send(conn: &net::Connection, out: Out) -> Result<(), String> {
    conn.outbound
        .send(out)
        .map_err(|_| "the writer thread stopped".to_owned())
}

fn run(a: &Args) -> Result<(), String> {
    let addr = format!("{}:{}", a.host, a.port);
    let conn = net::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let protocol = i32::try_from(data::VERSION.protocol).map_err(|e| e.to_string())?;
    for p in opening(protocol, &a.host, a.port, &a.name) {
        send(&conn, Out::Send(p))?;
    }
    println!(
        "bot {}: connecting to {addr} as protocol {protocol}",
        a.name
    );

    let mut session = Session::new();
    let mut stats = Stats {
        started: Instant::now(),
        play_since: None,
        per_state: BTreeMap::new(),
        unknown: BTreeMap::new(),
        decode_errors: 0,
    };
    let mut last_status = Instant::now();
    let limit = (a.seconds > 0).then(|| Duration::from_secs(a.seconds));

    loop {
        if let (Some(limit), Some(t)) = (limit, stats.play_since)
            && t.elapsed() >= limit
        {
            println!("bot {}: {}", a.name, stats.summary(&session, "done"));
            return Ok(());
        }
        if last_status.elapsed() >= STATUS_EVERY {
            last_status = Instant::now();
            println!(
                "bot {}: {:?} in {}, {}",
                a.name,
                session.state,
                session.dimension.as_deref().unwrap_or("-"),
                stats.summary(&session, "status")
            );
        }
        let msg = match conn.inbound.recv_timeout(Duration::from_secs(1)) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                return Err(stats.summary(&session, "the reader thread stopped"));
            }
        };
        let packet = match msg {
            Inbound::Packet(p) => p,
            Inbound::DecodeError { state, id, error } => {
                stats.decode_errors += 1;
                eprintln!("bot {}: cannot decode {state:?} id {id}: {error}", a.name);
                continue;
            }
            Inbound::Closed(why) => return Err(stats.summary(&session, &format!("closed ({why})"))),
        };
        *stats.per_state.entry(session.state).or_default() += 1;
        match &packet {
            S2c::Unknown { state, id, .. } => *stats.unknown.entry((*state, *id)).or_default() += 1,
            S2c::LoginDisconnect { reason } => {
                return Err(stats.summary(&session, &format!("kicked in login: {reason}")));
            }
            S2c::ConfigDisconnect { reason } | S2c::Disconnect { reason } => {
                return Err(stats.summary(
                    &session,
                    &format!("kicked in {:?}: {}", session.state, printable(reason)),
                ));
            }
            S2c::Login {
                entity_id,
                dimension,
                ..
            } => {
                println!(
                    "bot {}: in play as entity {entity_id} in {dimension}",
                    a.name
                );
            }
            S2c::LoginCompression { .. }
            | S2c::LoginFinished { .. }
            | S2c::SelectKnownPacks
            | S2c::ConfigKeepAlive { .. }
            | S2c::ConfigPing { .. }
            | S2c::FinishConfiguration
            | S2c::KeepAlive { .. }
            | S2c::Ping { .. }
            | S2c::PlayerPosition { .. }
            | S2c::SetTime
            | S2c::ChunkBatchFinished { .. }
            | S2c::StartConfiguration => {}
        }
        let before = session.state;
        for out in session.on_packet(&packet) {
            send(&conn, out)?;
        }
        if before != session.state {
            println!("bot {}: {before:?} -> {:?}", a.name, session.state);
            if session.state == State::Play {
                stats.play_since.get_or_insert_with(Instant::now);
            }
        }
    }
}
