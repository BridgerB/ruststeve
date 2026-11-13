//! Issue 12a: the model's air, fire, hunger, eating and melee, as the 26.1.2 jar has them
//! (crates/sim/src/life.rs).

use data::{BlockKind, EntityKind, ItemKind};
use protocol::{C2s, S2c, decode};
use sim::{Config, In, Sim, Wire, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: false,
};

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

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// Joined at (0.5, 64, 0.5) on the grass, with blocks set first and items given.
fn joined(set: &[((i32, i32, i32), BlockKind)], give: &[(ItemKind, i32)]) -> Sim {
    let mut w = flat_world(2, 63).unwrap();
    for ((x, y, z), k) in set {
        w.set_block(*x, *y, *z, state(*k)).unwrap();
    }
    let mut sim = Sim::new(CONFIG, w).seeded(3);
    for g in give {
        sim.survival.give(*g);
    }
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

#[test]
fn under_water_the_air_runs_out_then_it_drowns() {
    // A shaft of water two high over the feet, capped.
    let mut set = vec![
        ((0, 64, 0), BlockKind::Water),
        ((0, 65, 0), BlockKind::Water),
        ((0, 66, 0), BlockKind::Stone),
    ];
    for (dx, dz) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
        set.push(((dx, 64, dz), BlockKind::Stone));
        set.push(((dx, 65, dz), BlockKind::Stone));
    }
    let mut sim = joined(&set, &[]);
    let mut seen_air = Vec::new();
    for _ in 0..330 {
        for p in tick(&mut sim) {
            if let S2c::SetEntityData { id: 1, entries } = p {
                seen_air.extend(entries.iter().filter_map(|(i, v)| match (i, v) {
                    (1, protocol::metadata::MetaValue::Int(a)) => Some(*a),
                    _ => None,
                }));
            }
        }
    }
    assert!(
        seen_air.contains(&0),
        "the air reached 0: {:?}",
        seen_air.last()
    );
    // One drowning hit of 2 by tick 330; full food and saturation then heal it back quickly.
    assert!(sim.health < 20.0, "{}", sim.health);
}

#[test]
fn a_fire_block_burns_and_water_puts_it_out() {
    let mut sim = joined(
        &[
            ((0, 63, 0), BlockKind::Netherrack),
            ((0, 64, 0), BlockKind::Fire),
        ],
        &[],
    );
    for _ in 0..40 {
        tick(&mut sim);
    }
    assert!(sim.health < 20.0);
    assert!(sim.survival.life.fire_ticks > 0);
}

#[test]
fn bread_eaten_raises_the_food() {
    let mut sim = joined(&[], &[(ItemKind::Bread, 2)]);
    sim.survival.life.food = 12;
    send(
        &mut sim,
        C2s::UseItem {
            hand: 0,
            sequence: 1,
            yaw: 0.0,
            pitch: 0.0,
        },
    );
    let mut out = Vec::new();
    for _ in 0..33 {
        out.extend(tick(&mut sim));
    }
    assert_eq!(sim.survival.life.food, 17);
    assert!(
        out.iter()
            .any(|p| matches!(p, S2c::EntityEvent { id: 1, status: 9 }))
    );
    assert!(
        out.iter()
            .any(|p| matches!(p, S2c::SetHealth { food: 17, .. }))
    );
    assert_eq!(sim.survival.slots[36], Some((ItemKind::Bread, 1)));
    // Full food: bread is refused.
    sim.survival.life.food = 20;
    send(
        &mut sim,
        C2s::UseItem {
            hand: 0,
            sequence: 2,
            yaw: 0.0,
            pitch: 0.0,
        },
    );
    assert!(sim.survival.life.using.is_none());
}

#[test]
fn a_stone_sword_kills_a_husk_in_four_charged_swings() {
    let mut w = flat_world(2, 63).unwrap();
    w.set_block(2, 64, 0, state(BlockKind::Air)).unwrap();
    let mut sim = Sim::new(CONFIG, w).seeded(3);
    sim.survival.give((ItemKind::StoneSword, 1));
    let husk = sim.summon(EntityKind::Husk, [2.5, 64.0, 0.5]);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    let join = send(&mut sim, C2s::FinishConfiguration);
    assert!(
        join.iter()
            .any(|p| matches!(p, S2c::AddEntity { id, .. } if *id == husk))
    );
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    let mut died = false;
    for _ in 0..4 {
        for _ in 0..13 {
            tick(&mut sim);
        }
        let out = send(&mut sim, C2s::Attack { entity: husk });
        died |= out
            .iter()
            .any(|p| matches!(p, S2c::EntityEvent { id, status: 3 } if *id == husk));
    }
    assert!(
        died,
        "health {:?}",
        sim.survival.life.mobs.get(&husk).map(|m| m.health)
    );
    assert_eq!(sim.survival.life.kills, 1);
}
