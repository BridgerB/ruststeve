//! Issue 18: the burrow in the model (crates/sim/src/enderman.rs, docs/problems/pearls-model.md):
//! a level burrow one wide and two high running east, B the player's cell, M the mouth, O
//! outside. An angry enderman paths to the node nearest the player (O: it is 2.9 high and M is
//! two), a hit never pushes it into the burrow, the roof cuts a stare from B, and a pearl at O
//! is in reach from M and not from B. The parity rows e04 to e08 hold the same on a real server.

use data::ItemKind;
use protocol::{C2s, S2c, decode};
use sim::nether::Stage;
use sim::{Config, In, Sim, Wire};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
/// The cavern's floor top; the feet stand one over it.
const FLOOR: i32 = 40;
const FEET: i32 = FLOOR + 1;
const FEET_Y: f64 = 41.0;

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

/// A cavern with the burrow: B at (0, 41, 0), M at (1, 41, 0), O at (2, 41, 0) and open floor
/// east of it; the player stands at `at` (B or M).
fn burrow(at: (i32, i32, i32)) -> Stage {
    let mut stage = Stage::new(2, FLOOR, FLOOR + 20);
    let stone = data::BlockKind::Stone;
    stage
        .fill((-1, -1), (FEET, FEET + 2), (-1, 1), stone)
        .fill((0, 1), (FEET, FEET + 2), (-1, -1), stone)
        .fill((0, 1), (FEET, FEET + 2), (1, 1), stone)
        .fill((0, 1), (FEET + 2, FEET + 2), (0, 0), stone)
        .spawn(at);
    stage
}

fn joined(at: (i32, i32, i32)) -> Sim {
    join(
        Sim::new(CONFIG, burrow(at).build().unwrap())
            .seeded(7)
            .with_natural_spawns(),
    )
}

fn look(sim: &mut Sim, yaw: f32, pitch: f32) {
    let [x, y, z] = sim.pos;
    send(
        sim,
        C2s::MovePlayerPosRot {
            x,
            y,
            z,
            yaw,
            pitch,
            flags: 1,
        },
    );
}

fn anger(sim: &mut Sim, id: i32) {
    if let Some(m) = sim.survival.life.mobs.get_mut(&id)
        && let sim::life::Brain::Enderman(e) = &mut m.brain
    {
        e.angry = true;
    }
}

fn cell(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| (v + 1e-7).floor() as i32)
}

const O: [i32; 3] = [2, FEET, 0];

#[test]
fn an_angry_enderman_walks_to_the_mouth_and_never_reaches_the_bot_in_b() {
    let mut sim = joined((0, FEET, 0));
    let id = sim.summon_enderman([10.5, FEET_Y, 0.5]);
    anger(&mut sim, id);
    // Looking down: no stare, so nothing freezes it.
    look(&mut sim, 270.0, 90.0);
    let mut first = None;
    for t in 1..=400u32 {
        tick(&mut sim);
        let pos = sim.survival.life.mobs[&id].pos;
        assert!(pos[0] >= 2.3 - 1e-9, "into the mouth at tick {t}: {pos:?}");
        if first.is_none() && cell(pos) == O {
            first = Some(t);
        }
    }
    let t = first.expect("came to O");
    assert!(t <= 200, "came to O after {t} ticks");
    assert_eq!(cell(sim.survival.life.mobs[&id].pos), O, "stays at O");
    assert!((sim.health - 20.0).abs() < 1e-6, "hit: {}", sim.health);
    assert!(sim.survival.life.hurt_by_mob.is_empty());
    assert_eq!(sim.ender(id).unwrap().teleports, 0);
}

#[test]
fn strikes_from_b_kill_it_at_the_mouth_and_a_hit_never_pushes_it_in() {
    let mut sim = joined((0, FEET, 0));
    sim.survival.give((ItemKind::IronSword, 1));
    let id = sim.summon_enderman([2.5, FEET_Y, 0.5]);
    anger(&mut sim, id);
    look(&mut sim, 270.0, 20.0);
    let mut hits = 0;
    for t in 1..=2000u32 {
        tick(&mut sim);
        let m = sim.survival.life.mobs[&id];
        if m.dead_at.is_some() {
            break;
        }
        assert!(
            m.pos[0] >= 2.3 - 1e-9,
            "into the mouth at tick {t}: {:?}",
            m.pos
        );
        // A full charge (iron sword 1.6 a second: 12.5 ticks) and in reach (3.0 to its box).
        if t % 14 == 0 && m.pos[0] - 0.3 - sim.pos[0] <= 3.0 {
            let before = m.health;
            send(&mut sim, C2s::Attack { entity: id });
            if sim.survival.life.mobs[&id].health < before {
                hits += 1;
            }
        }
    }
    assert!(
        sim.survival.life.mobs[&id].dead_at.is_some(),
        "dead after {hits} hits"
    );
    // 40 health at 6 a hit: 7.
    assert_eq!(hits, 7);
    assert_eq!(sim.ender(id).unwrap().teleports, 0);
    assert!(sim.survival.life.hurt_by_mob.is_empty());
}

#[test]
fn the_roof_cuts_a_stare_from_b_at_an_enderman_at_o() {
    let mut sim = joined((0, FEET, 0));
    let at = [2.5, FEET_Y, 0.5];
    let id = sim.summon_enderman(at);
    // Straight at its eye from B.
    let eye = [sim.pos[0], sim.pos[1] + 1.62, sim.pos[2]];
    let d = [at[0] - eye[0], at[1] + 2.55 - eye[1], at[2] - eye[2]];
    let yaw = (-d[0]).atan2(d[2]).to_degrees() as f32;
    let pitch = -(d[1].atan2(d[0].hypot(d[2]))).to_degrees() as f32;
    look(&mut sim, yaw, pitch);
    assert!(!sim.stares_at(sim.survival.life.mobs[&id].pos));
    for _ in 0..60 {
        tick(&mut sim);
    }
    let e = sim.ender(id).unwrap();
    assert!(!e.stared_at && !e.angry);
}

fn pearls_after(at: (i32, i32, i32)) -> i32 {
    let mut stage = burrow(at);
    stage.spawn(at);
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(7);
    sim.place_item((ItemKind::EnderPearl, 1), [2.5, FEET_Y, 0.5]);
    let mut sim = join(sim);
    for _ in 0..60 {
        tick(&mut sim);
    }
    sim.survival
        .slots
        .iter()
        .flatten()
        .filter(|(k, _)| *k == ItemKind::EnderPearl)
        .map(|(_, n)| *n)
        .sum()
}

#[test]
fn a_pearl_at_o_is_picked_up_from_m_and_not_from_b() {
    assert_eq!(pearls_after((0, FEET, 0)), 0, "picked from B");
    assert_eq!(pearls_after((1, FEET, 0)), 1, "not picked from M");
}
