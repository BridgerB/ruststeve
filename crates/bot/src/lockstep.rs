//! The bot and the server model folded together in one thread, with no clock (clojurecraft's
//! `sim/run`): each server tick the model ticks, the bot's world ticks, and every packet each
//! side sends is delivered before the next tick. A run is a pure function of the model world,
//! the seed and the code, so it is reproduced rather than saved (brain/ruststeve/design/server-model.md).
//! The shell loop with its threads and clock is covered by `bot::run` against `bot::model`.

use protocol::{S2c, decode};
use sim::{In, Sim};
use world::{Effect, Event, World};

/// How a lockstep run went.
#[derive(Debug)]
pub struct Run {
    pub world: World,
    pub sim: Sim,
    /// Packets from the model that would not decode, and ids the bot does not model.
    pub decode_errors: u64,
    pub unknown: u64,
    /// The bot's own death, if the model told of one.
    pub died: bool,
}

/// xorshift64: the tick seeds, from the run's seed.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

/// Deliver the model's packets to the bot and the bot's to the model until both are quiet.
fn settle(world: &mut World, sim: &mut Sim, mut to_bot: Vec<sim::Wire>, run: &mut Counts) {
    let mut guard = 0;
    while !to_bot.is_empty() && guard < 64 {
        guard += 1;
        let mut to_model = Vec::new();
        for w in to_bot.drain(..) {
            match decode(w.state, w.id, &w.body) {
                Ok(p) => {
                    if matches!(p, S2c::Unknown { .. }) {
                        run.unknown += 1;
                    }
                    to_model.extend(world.step(&Event::Packet(p)));
                }
                Err(_) => run.decode_errors += 1,
            }
        }
        for e in to_model {
            if let Effect::Send(p) = e
                && let Ok(out) = sim.step(&In::Packet(p))
            {
                to_bot.extend(out);
            }
        }
    }
}

#[derive(Default)]
struct Counts {
    decode_errors: u64,
    unknown: u64,
}

/// Run the bot against `sim` for `ticks` server ticks, pressing `controls(tick)` once in the
/// world. Joins first (the opening packets), then ticks.
pub fn run(
    mut sim: Sim,
    name: &str,
    seed: u64,
    ticks: u64,
    controls: impl Fn(u64) -> physics::Controls,
) -> Run {
    let mut world = World::new();
    let mut counts = Counts::default();
    let mut s = seed | 1;
    let protocol = i32::try_from(data::VERSION.protocol).unwrap_or(0);
    let mut first = Vec::new();
    for p in protocol::session::opening(protocol, "model", 0, name) {
        if let Ok(out) = sim.step(&In::Packet(p)) {
            first.extend(out);
        }
    }
    settle(&mut world, &mut sim, first, &mut counts);
    let mut pressed: Option<physics::Controls> = None;
    let mut walk_tick = 0;
    for t in 0..ticks {
        if !sim.open() || world.death.is_some() {
            break;
        }
        let mut to_bot = sim.step(&In::Tick).unwrap_or_default();
        if world.body.is_some() {
            let c = controls(walk_tick);
            walk_tick += 1;
            if pressed != Some(c) {
                pressed = Some(c);
                world.step(&Event::Controls(c));
            }
        }
        let effects = world.step(&Event::Tick {
            now_ms: t * 50,
            seed: next(&mut s),
        });
        for e in effects {
            if let Effect::Send(p) = e
                && let Ok(out) = sim.step(&In::Packet(p))
            {
                to_bot.extend(out);
            }
        }
        settle(&mut world, &mut sim, to_bot, &mut counts);
    }
    let died = world.death.is_some();
    Run {
        world,
        sim,
        decode_errors: counts.decode_errors,
        unknown: counts.unknown,
        died,
    }
}
