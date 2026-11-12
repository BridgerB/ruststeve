//! bot: one bot against one server. For now it joins and stays in the world: the handshake,
//! offline login, configuration, then play, folding everything the server sends into `World`
//! and answering keep-alives and pings. With `--record` every run is a file `replay` can verify.
//!
//! ```text
//! cargo run -p bot -- [--host 127.0.0.1] [--port 25565] [--name rs-001] [--seconds 600]
//!                     [--record target/recordings/rs-001.rsr]
//! ```
//!
//! `--seconds 0` stays until the server closes. It exits 0 when the time is up while still in
//! play, and 1 when the server disconnects it first.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use net::Inbound;
use protocol::session::opening;
use protocol::{Out, S2c, State};
use record::{CHECKPOINT_TICKS, EffectRecord, EventRecord, Header, Record, Recorder};
use world::{Event, World};

/// How often the bot prints a status line.
const STATUS_EVERY: Duration = Duration::from_secs(10);
/// The loop's tick: the world gets the clock and a fresh seed this often (a game tick).
const TICK_EVERY: Duration = Duration::from_millis(50);

struct Args {
    host: String,
    port: u16,
    name: String,
    seconds: u64,
    record: Option<PathBuf>,
    /// Walk a scripted square (issue 06's live check) instead of standing still.
    walk: bool,
}

fn args() -> Result<Args, String> {
    let mut a = Args {
        host: "127.0.0.1".to_owned(),
        port: 25565,
        name: "rs-001".to_owned(),
        seconds: 600,
        record: None,
        walk: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(flag) = it.next() {
        let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--host" => a.host = value,
            "--port" => a.port = value.parse().map_err(|e| format!("--port: {e}"))?,
            "--name" => a.name = value,
            "--seconds" => a.seconds = value.parse().map_err(|e| format!("--seconds: {e}"))?,
            "--record" => a.record = Some(PathBuf::from(value)),
            "--walk" => a.walk = value == "square",
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
    fn summary(&self, world: &World, why: &str) -> String {
        let session = &world.session;
        format!(
            "{why}: {:.0} s connected, {:.0} s in play, {} keep-alives answered, {} server ticks, {} teleports, {} columns, health {:?}, standing on {:?}, {} entities, {} inventory stacks, {} sightings, packets {:?}, unknown ids {}, decode errors {}",
            self.started.elapsed().as_secs_f64(),
            self.play_since.map_or(0.0, |t| t.elapsed().as_secs_f64()),
            session.keep_alives_answered,
            session.server_ticks,
            session.teleports,
            world.columns.len(),
            world.player.health,
            world.block_under_player(),
            world.entities.len(),
            world.inventory.slots.len(),
            world.memory.log().len(),
            self.per_state,
            self.unknown.len(),
            self.decode_errors
        )
    }
}

/// The shell's randomness: xorshift64 from a clock-derived start. The core never draws its
/// own; it gets these on the tick, and the recording keeps them.
struct Seeds(u64);

impl Seeds {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// The world and, when recording, its file: every event is written before the step, its
/// effects after, and a checkpoint every `CHECKPOINT_TICKS` server ticks.
struct Run {
    world: World,
    recorder: Option<Recorder>,
    events: u64,
    next_checkpoint: u64,
}

impl Run {
    fn step(
        &mut self,
        conn: &net::Connection,
        record: EventRecord,
        event: Event,
    ) -> Result<(), String> {
        if let Some(r) = &mut self.recorder {
            r.write(&Record::Event(record))?;
        }
        let effects = self.world.step(&event);
        self.events += 1;
        if let Some(r) = &mut self.recorder {
            let recorded = effects
                .iter()
                .map(EffectRecord::of)
                .collect::<Result<Vec<_>, _>>()?;
            r.write(&Record::Effects(recorded))?;
            let ticks = self.world.session.server_ticks;
            if ticks >= self.next_checkpoint {
                r.write(&Record::Checkpoint {
                    event_index: self.events,
                    server_ticks: ticks,
                    world: Box::new(self.world.clone()),
                })?;
                self.next_checkpoint = (ticks / CHECKPOINT_TICKS + 1) * CHECKPOINT_TICKS;
            }
        }
        for e in &effects {
            conn.outbound
                .send(record::as_out(e))
                .map_err(|_| "the writer thread stopped".to_owned())?;
        }
        Ok(())
    }

    fn finish(self) -> Result<(), String> {
        self.recorder.map_or(Ok(()), Recorder::finish)
    }
}

fn run(a: &Args) -> Result<(), String> {
    let addr = format!("{}:{}", a.host, a.port);
    let conn = net::connect(&addr).map_err(|e| format!("connect {addr}: {e}"))?;
    let protocol = i32::try_from(data::VERSION.protocol).map_err(|e| e.to_string())?;
    for p in opening(protocol, &a.host, a.port, &a.name) {
        conn.outbound
            .send(Out::Send(p))
            .map_err(|_| "the writer thread stopped".to_owned())?;
    }
    println!(
        "bot {}: connecting to {addr} as protocol {protocol}",
        a.name
    );

    let seed = now_ms() | 1;
    let mut seeds = Seeds(seed);
    let recorder = match &a.record {
        Some(path) => {
            let r = Recorder::create(path, &Header::new(&a.name, &addr, seed))?;
            println!(
                "bot {}: recording to {} (commit {})",
                a.name,
                path.display(),
                record::COMMIT
            );
            Some(r)
        }
        None => None,
    };
    let mut run = Run {
        world: World::new(),
        recorder,
        events: 0,
        next_checkpoint: CHECKPOINT_TICKS,
    };
    let mut stats = Stats {
        started: Instant::now(),
        play_since: None,
        per_state: BTreeMap::new(),
        unknown: BTreeMap::new(),
        decode_errors: 0,
    };
    let mut last_status = Instant::now();
    let mut last_tick = Instant::now();
    let mut walk_ticks: u64 = 0;
    let mut walk_start: Option<[f64; 3]> = None;
    let mut last_controls: Option<physics::Controls> = None;
    let mut walked = 0.0f64;
    let limit = (a.seconds > 0).then(|| Duration::from_secs(a.seconds));

    let result = loop {
        if let (Some(limit), Some(t)) = (limit, stats.play_since)
            && t.elapsed() >= limit
        {
            println!("bot {}: {}", a.name, stats.summary(&run.world, "done"));
            if a.walk {
                println!(
                    "bot {}: walked {walked:.1} blocks from the start, server corrections {}",
                    a.name,
                    run.world.session.teleports.saturating_sub(1)
                );
            }
            break Ok(());
        }
        if last_status.elapsed() >= STATUS_EVERY {
            last_status = Instant::now();
            println!(
                "bot {}: {:?} in {}, {}",
                a.name,
                run.world.session.state,
                run.world
                    .dimension
                    .as_ref()
                    .map_or("-", |d| d.name.as_str()),
                stats.summary(&run.world, "status")
            );
        }
        if last_tick.elapsed() >= TICK_EVERY {
            last_tick = Instant::now();
            if a.walk && run.world.body.is_some() {
                let start = *walk_start.get_or_insert(run.world.body.map_or([0.0; 3], |b| b.pos));
                let c = walk_controls(walk_ticks);
                walk_ticks += 1;
                if Some(c) != last_controls {
                    last_controls = Some(c);
                    if let Err(e) = run.step(&conn, EventRecord::Controls(c), Event::Controls(c)) {
                        break Err(e);
                    }
                }
                if let Some(b) = run.world.body {
                    walked = ((b.pos[0] - start[0]).powi(2) + (b.pos[2] - start[2]).powi(2)).sqrt();
                }
            }
            let (now_ms, seed) = (now_ms(), seeds.next());
            if let Err(e) = run.step(
                &conn,
                EventRecord::Tick { now_ms, seed },
                Event::Tick { now_ms, seed },
            ) {
                break Err(e);
            }
        }
        let msg = match conn.inbound.recv_timeout(TICK_EVERY) {
            Ok(m) => m,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                break Err(stats.summary(&run.world, "the reader thread stopped"));
            }
        };
        let (packet, raw) = match msg {
            Inbound::Packet { packet, raw } => (packet, raw),
            Inbound::DecodeError { state, id, error } => {
                stats.decode_errors += 1;
                eprintln!("bot {}: cannot decode {state:?} id {id}: {error}", a.name);
                continue;
            }
            Inbound::Closed(why) => {
                break Err(stats.summary(&run.world, &format!("closed ({why})")));
            }
        };
        *stats.per_state.entry(run.world.session.state).or_default() += 1;
        match &packet {
            S2c::Unknown { state, id, .. } => *stats.unknown.entry((*state, *id)).or_default() += 1,
            S2c::LoginDisconnect { reason } => {
                break Err(stats.summary(&run.world, &format!("kicked in login: {reason}")));
            }
            S2c::ConfigDisconnect { reason } | S2c::Disconnect { reason } => {
                break Err(stats.summary(
                    &run.world,
                    &format!(
                        "kicked in {:?}: {}",
                        run.world.session.state,
                        printable(reason)
                    ),
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
            _ => {}
        }
        let before = run.world.session.state;
        let record = EventRecord::Packet {
            state: raw.state,
            id: raw.id,
            body: raw.body,
        };
        if let Err(e) = run.step(&conn, record, Event::Packet(packet)) {
            break Err(e);
        }
        if before != run.world.session.state {
            println!(
                "bot {}: {before:?} -> {:?}",
                a.name, run.world.session.state
            );
            if run.world.session.state == State::Play {
                stats.play_since.get_or_insert_with(Instant::now);
            }
        }
    };
    let events = run.events;
    run.finish()?;
    if a.record.is_some() {
        println!("bot {}: recorded {events} events", a.name);
    }
    result
}

/// Issue 06's scripted walk, a test harness until the planner (issue 09): a staircase, 40 ticks
/// south then 40 west, over and over, so the body keeps getting farther from where it began,
/// jumping for a few ticks at the start of every west leg so ledges and slopes are climbed.
fn walk_controls(tick: u64) -> physics::Controls {
    let side = (tick / 40) % 4;
    let into_side = tick % 40;
    physics::Controls {
        forward: true,
        jump: side % 2 == 1 && into_side < 6,
        sneak: false,
        yaw: [0.0, 90.0, 0.0, 90.0][side as usize],
    }
}
