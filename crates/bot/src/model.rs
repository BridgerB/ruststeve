//! The server model as the bot's body (brain/ruststeve/design/body-boundary.md): a thread that
//! holds a `sim::Sim` behind the same channel pair a socket's reader and writer give, so the
//! bot's loop runs against it unchanged. The model's packets are encoded and decoded exactly as
//! a server's (`S2c::encode`, `decode`), and recorded the same way.

use std::sync::mpsc::{RecvTimeoutError, SyncSender, sync_channel};
use std::time::{Duration, Instant};

use net::{Connection, Inbound, Raw};
use protocol::{Out, decode};
use sim::{In, Sim, Wire};

/// A server tick: the model ticks on the wall clock here, as a server does.
const TICK: Duration = Duration::from_millis(50);

fn deliver(tx: &SyncSender<Inbound>, w: Wire) -> bool {
    let body: Vec<u8> = w.body.as_ref().clone();
    let msg = match decode(w.state, w.id, &body) {
        Ok(packet) => Inbound::Packet {
            packet,
            raw: Raw {
                state: w.state,
                id: w.id,
                body,
            },
        },
        Err(error) => Inbound::DecodeError {
            state: w.state,
            id: w.id,
            error,
        },
    };
    tx.send(msg).is_ok()
}

/// Start the model on its own thread and return the connection the bot uses.
pub fn connect(mut sim: Sim) -> Connection {
    let (in_tx, inbound) = sync_channel(4096);
    let (outbound, out_rx) = sync_channel::<Out>(1024);
    let thread = std::thread::spawn(move || {
        let mut next_tick = Instant::now() + TICK;
        let run = |sim: &mut Sim, input: In| -> Result<bool, String> {
            for w in sim.step(&input).map_err(|e| e.to_string())? {
                if !deliver(&in_tx, w) {
                    return Ok(false);
                }
            }
            Ok(sim.open())
        };
        let why = loop {
            let wait = next_tick.saturating_duration_since(Instant::now());
            let input = match out_rx.recv_timeout(wait) {
                Ok(Out::Send(p)) => In::Packet(p),
                // The model never compresses: nothing to do.
                Ok(Out::SetCompression(_)) => continue,
                Err(RecvTimeoutError::Timeout) => {
                    next_tick += TICK;
                    In::Tick
                }
                Err(RecvTimeoutError::Disconnected) => break "the bot closed".to_owned(),
            };
            match run(&mut sim, input) {
                Ok(true) => {}
                Ok(false) => break sim.closed.clone().unwrap_or_else(|| "closed".to_owned()),
                Err(e) => break format!("model: {e}"),
            }
        };
        if !sim.violations.is_empty() {
            eprintln!(
                "model: {} violations: {:?}",
                sim.violations.len(),
                sim.violations
            );
        }
        let _ = in_tx.send(Inbound::Closed(why));
    });
    Connection {
        inbound,
        outbound,
        threads: vec![thread],
    }
}
