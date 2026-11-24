//! The day clock on real bytes (`docs/problems/day-clock.md`): `set_time` packets cut from race
//! 45355573039 lane 0 (`rs-l00-b00.rsr`, commit be2197a, `replay export` in probe 45359992340).
//! The server sends the world clocks once, at the join; every later `set_time` carries the game
//! time alone, and the day runs on from the clock at its rate.

use protocol::{ClockUpdate, S2c, State, decode};
use world::{Event, World};

/// The join's `set_time` (event 57): game time 78919, two clocks (0 and 1) at 78919 ticks,
/// partial 0, rate 1.
const JOIN: &str = "00000000000134470200c7e804000000003f80000001c7e804000000003f800000";
/// The next one (event 161): game time 78927, no clocks.
const NEXT: &str = "000000000001344f00";
/// The run's last (event 54564): game time 90887, no clocks.
const LAST: &str = "000000000001630700";

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect()
}

fn set_time(hex: &str) -> S2c {
    let id = data::generated::packets::PlayClientbound::SetTime.id();
    decode(State::Play, id, &bytes(hex)).unwrap()
}

#[test]
fn the_join_carries_both_clocks() {
    let clock = |clock| ClockUpdate {
        clock,
        total_ticks: 78_919,
        partial_tick: 0.0,
        rate: 1.0,
    };
    let p = set_time(JOIN);
    assert_eq!(
        p,
        S2c::SetTime {
            game_time: 78_919,
            clocks: vec![clock(0), clock(1)],
        }
    );
    // The encoder writes the server's bytes back.
    let (_, body) = p.encode().unwrap();
    assert_eq!(body, bytes(JOIN));
    assert_eq!(
        set_time(NEXT),
        S2c::SetTime {
            game_time: 78_927,
            clocks: Vec::new(),
        }
    );
}

#[test]
fn the_day_runs_on_from_the_join() {
    let mut w = World::new();
    assert_eq!(w.day_time(), None, "not told yet");
    w.step(&Event::Packet(set_time(JOIN)));
    assert_eq!(w.day_time(), Some(78_919 % 24_000));
    assert_eq!(w.is_night(), Some(false));
    w.step(&Event::Packet(set_time(NEXT)));
    assert_eq!(w.day_time(), Some(6_927));
    // The run's end, 11,968 ticks on: past dusk.
    w.step(&Event::Packet(set_time(LAST)));
    assert_eq!(w.day_time(), Some(18_887));
    assert_eq!(w.is_night(), Some(true));
    assert_eq!(w.day_time().map(world::time::ticks_to_dawn), Some(4_573));
}

#[test]
fn a_stopped_clock_stands_still() {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::SetTime {
        game_time: 100,
        clocks: vec![ClockUpdate {
            clock: protocol::OVERWORLD_CLOCK,
            total_ticks: 13_000,
            partial_tick: 0.0,
            rate: 0.0,
        }],
    }));
    w.step(&Event::Packet(S2c::SetTime {
        game_time: 5_000,
        clocks: Vec::new(),
    }));
    assert_eq!(w.day_time(), Some(13_000));
}
