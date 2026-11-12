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
    /// The bot's teleports when the controls began: later ones are corrections.
    pub teleports_at_script: Option<u64>,
    /// The lowest health the model told of while the controls ran.
    pub min_health_in_script: Option<f32>,
}

/// xorshift64: the tick seeds, from the run's seed.
fn next(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

/// Deliver the model's packets to the bot and the bot's to the model until both are quiet.
fn deliver(world: &mut World, sim: &mut Sim, mut to_bot: Vec<sim::Wire>, run: &mut Counts) {
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

/// Run the bot against `sim` for `ticks` server ticks, pressing `controls(tick)` from
/// `settle` ticks after the body is placed (a real run's start settles the same 20). Joins
/// first (the opening packets), then ticks.
pub fn run(
    mut sim: Sim,
    name: &str,
    seed: u64,
    ticks: u64,
    settle: u64,
    mut controls: impl FnMut(&World, u64) -> physics::Controls,
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
    deliver(&mut world, &mut sim, first, &mut counts);
    let mut pressed: Option<physics::Controls> = None;
    let mut walk_tick = 0;
    // The settle, as bot::run counts it: physics ticks with the ground loaded and no teleport
    // since; the script begins once `settle` have passed.
    let mut since = world.ticks;
    let mut teleports_seen = world.session.teleports;
    let mut teleports_at_script = None;
    let mut min_health_in_script: Option<f32> = None;
    for t in 0..ticks {
        if !sim.open() || world.death.is_some() {
            break;
        }
        let mut to_bot = sim.step(&In::Tick).unwrap_or_default();
        if teleports_at_script.is_none()
            && (teleports_seen != world.session.teleports || !world.columns_loaded_under_body())
        {
            since = world.ticks;
            teleports_seen = world.session.teleports;
        }
        if world.body.is_some() && (teleports_at_script.is_some() || world.ticks - since >= settle)
        {
            teleports_at_script.get_or_insert(world.session.teleports);
            if let Some(h) = world.player.health {
                min_health_in_script = Some(min_health_in_script.map_or(h, |m: f32| m.min(h)));
            }
            let c = controls(&world, walk_tick);
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
        deliver(&mut world, &mut sim, to_bot, &mut counts);
    }
    let died = world.death.is_some();
    Run {
        world,
        sim,
        decode_errors: counts.decode_errors,
        unknown: counts.unknown,
        died,
        teleports_at_script,
        min_health_in_script,
    }
}
