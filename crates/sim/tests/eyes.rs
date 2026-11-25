//! Issue 19: the eye of ender in the model (crates/sim/src/eye.rs, docs/problems/eyes-model.md).
//! A thrown eye flies the line to the stronghold's point exactly, 12 blocks out and up toward 8,
//! and ends at life 81 as a dropped eye (4 in 5) or a shatter; with no stronghold (the Nether) a
//! use throws nothing and keeps the eye. Rows y01 and y02 hold the same on a real server.

use data::{EntityKind, ItemKind};
use protocol::{C2s, S2c, decode};
use sim::eye::{Eye, LIFE, TOO_FAR, TOO_FAR_RISE, eye_step, eye_target};
use sim::{Config, In, Sim, Wire};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: true,
};
const GROUND: i32 = 64;
/// The stronghold the race seed's eyes fly to from every landing but one (RCON `locate`,
/// probe 45685968956).
const STRONGHOLD: [f64; 3] = [1648.0, 0.0, -16.0];

fn packets(out: Vec<Wire>) -> Vec<S2c> {
    out.iter()
        .filter_map(|w| decode(w.state, w.id, &w.body).ok())
        .collect()
}

fn send(sim: &mut Sim, p: C2s) -> Vec<S2c> {
    packets(sim.step(&In::Packet(p)).unwrap())
}

fn tick(sim: &mut Sim) -> Vec<S2c> {
    packets(sim.step(&In::Tick).unwrap())
}

fn join(mut sim: Sim) -> Sim {
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    sim
}

fn throw(sim: &mut Sim) -> Vec<S2c> {
    send(
        sim,
        C2s::UseItem {
            hand: 0,
            sequence: 1,
            yaw: 0.0,
            pitch: -20.0,
        },
    )
}

fn eye_world(seed: u64, stronghold: Option<[f64; 3]>) -> Sim {
    let world = sim::flat_world(3, GROUND).unwrap();
    let mut sim = Sim::new(CONFIG, world).seeded(seed);
    if let Some(p) = stronghold {
        sim = sim.with_stronghold(p);
    }
    sim.survival.give((ItemKind::EnderEye, 3));
    join(sim)
}

fn eyes_held(sim: &Sim) -> i32 {
    sim.survival
        .slots
        .iter()
        .flatten()
        .filter(|(k, _)| *k == ItemKind::EnderEye)
        .map(|(_, n)| n)
        .sum()
}

/// The flight folded from `eye_step`: every place, spawn first.
fn flight(from: [f64; 3], point: [f64; 3]) -> Vec<[f64; 3]> {
    let mut e = Eye {
        pos: from,
        delta: [0.0; 3],
        target: eye_target(from, point),
        life: 0,
        survive: true,
    };
    let mut out = vec![e.pos];
    while e.life <= LIFE {
        eye_step(&mut e);
        out.push(e.pos);
    }
    out
}

#[test]
fn a_far_eye_flies_the_exact_line_twelve_out_and_rises() {
    let from = [0.5, 65.9, 0.5];
    let path = flight(from, STRONGHOLD);
    // 81 moves: life 1 to 81, discarded once it passes 80.
    assert_eq!(path.len(), 82);
    let bearing = (STRONGHOLD[2] - from[2]).atan2(STRONGHOLD[0] - from[0]);
    for p in &path[1..] {
        let (dx, dz) = (p[0] - from[0], p[2] - from[2]);
        if dx.hypot(dz) > 1e-9 {
            let off = (dz.atan2(dx) - bearing).abs();
            assert!(off < 1e-9, "off the line by {off} rad at {p:?}");
        }
    }
    let flown = |p: [f64; 3]| (p[0] - from[0]).hypot(p[2] - from[2]);
    let last = path[path.len() - 1];
    let far = path.iter().map(|p| flown(*p)).fold(0.0, f64::max);
    assert!(far > TOO_FAR - 1.0 && far < TOO_FAR + 2.0, "farthest {far}");
    assert!((flown(last) - TOO_FAR).abs() < 1.5, "ends {}", flown(last));
    let rise = last[1] - from[1];
    assert!(rise > 2.0 && rise < TOO_FAR_RISE + 3.0, "rose {rise}");
    // The flight, for the wall file (`cargo test -p sim --test eyes -- --nocapture`).
    for (t, p) in path.iter().enumerate() {
        println!(
            "t {t} plane {:.3} up {:.3} speed {:.4}",
            flown(*p),
            p[1] - from[1],
            path.get(t + 1).map_or(0.0, |q| flown(*q) - flown(*p))
        );
    }
}

#[test]
fn a_near_eye_goes_to_the_point_itself() {
    let from = [0.5, 30.9, 0.5];
    let point = [6.0, 0.0, -4.0];
    assert_eq!(eye_target(from, point), point);
    let path = flight(from, point);
    let last = path[path.len() - 1];
    assert!((last[0] - point[0]).hypot(last[2] - point[2]) < 1.0);
    assert!(last[1] < from[1], "it goes down toward the point");
}

#[test]
fn a_throw_spends_one_eye_and_ends_dropped_or_shattered() {
    let (mut dropped, mut shattered) = (0, 0);
    for seed in 1..=60 {
        let mut sim = eye_world(seed, Some(STRONGHOLD));
        let out = throw(&mut sim);
        let added = out.iter().find_map(|p| match p {
            S2c::AddEntity { id, kind, .. } if *kind == EntityKind::EyeOfEnder.id() => Some(*id),
            _ => None,
        });
        let id = added.expect("an eye added");
        assert_eq!(eyes_held(&sim), 2, "seed {seed}: one spent");
        let mut removed_at = None;
        let mut end = None;
        for t in 1..=100 {
            for p in tick(&mut sim) {
                match p {
                    S2c::RemoveEntities { ids } if ids.contains(&id) => removed_at = Some(t),
                    S2c::AddEntity { kind, .. } if kind == EntityKind::Item.id() => {
                        end = Some("dropped");
                    }
                    S2c::LevelEvent { event: 2003, .. } => end = Some("shattered"),
                    _ => {}
                }
            }
        }
        assert_eq!(removed_at, Some(81), "seed {seed}");
        match end {
            Some("dropped") => dropped += 1,
            Some("shattered") => shattered += 1,
            other => panic!("seed {seed}: {other:?}"),
        }
    }
    assert_eq!(dropped + shattered, 60);
    // 4 in 5 survive: 48 expected of 60.
    assert!((40..=56).contains(&dropped), "dropped {dropped} of 60");
}

#[test]
fn with_no_stronghold_a_use_throws_nothing_and_keeps_the_eye() {
    let mut sim = eye_world(1, None);
    let out = throw(&mut sim);
    assert!(
        !out.iter().any(
            |p| matches!(p, S2c::AddEntity { kind, .. } if *kind == EntityKind::EyeOfEnder.id())
        )
    );
    assert_eq!(eyes_held(&sim), 3);
    assert!(sim.eyes.is_empty());
}
