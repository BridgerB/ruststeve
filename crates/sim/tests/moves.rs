//! Issue 08b: the model judges moves as handleMovePlayer does (too quickly, wrongly, into a
//! block) and hurts falls as Entity.checkFallDamage and LivingEntity.calculateFallDamage do.

use data::BlockKind;
use protocol::{C2s, S2c, decode};
use sim::{Config, In, Sim, Wire, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
};

fn state(k: BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap()
}

fn packets(out: Vec<Wire>) -> Vec<S2c> {
    out.iter()
        .filter_map(|w| decode(w.state, w.id, &w.body).ok())
        .collect()
}

fn send(sim: &mut Sim, p: C2s) -> Vec<S2c> {
    packets(sim.step(&In::Packet(p)).unwrap())
}

/// Join and accept the first teleport: the player stands on the grass at (0.5, 64, 0.5).
fn joined(world: sim::ModelWorld) -> Sim {
    let mut sim = Sim::new(CONFIG, world);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    let joined = send(&mut sim, C2s::FinishConfiguration);
    assert!(
        joined
            .iter()
            .any(|p| matches!(p, S2c::PlayerPosition { .. }))
    );
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    sim
}

fn mv(sim: &mut Sim, x: f64, y: f64, z: f64, on_ground: bool) -> Vec<S2c> {
    sim.step(&In::Tick).unwrap();
    send(
        sim,
        C2s::MovePlayerPosRot {
            x,
            y,
            z,
            yaw: 0.0,
            pitch: 0.0,
            flags: u8::from(on_ground),
        },
    )
}

fn corrected(out: &[S2c]) -> bool {
    out.iter().any(|p| matches!(p, S2c::PlayerPosition { .. }))
}

#[test]
fn a_walking_step_is_accepted() {
    let mut sim = joined(flat_world(2, 63).unwrap());
    let out = mv(&mut sim, 0.5, 64.0, 0.72, true);
    assert!(!corrected(&out));
    assert_eq!(sim.pos, [0.5, 64.0, 0.72]);
    assert_eq!(sim.corrections, 0);
}

#[test]
fn a_jump_of_twenty_blocks_is_too_quick() {
    let mut sim = joined(flat_world(2, 63).unwrap());
    let out = mv(&mut sim, 20.5, 64.0, 0.5, true);
    assert!(corrected(&out));
    assert_eq!(sim.pos, [0.5, 64.0, 0.5], "sent back");
    assert_eq!(sim.corrections, 1);
}

#[test]
fn walking_into_a_wall_is_corrected() {
    let mut world = flat_world(2, 63).unwrap();
    for y in 64..=65 {
        world.set_block(1, y, 0, state(BlockKind::Stone)).unwrap();
    }
    let mut sim = joined(world);
    // Into the wall's cell, half a block: the server's own move stops at the face.
    let out = mv(&mut sim, 1.2, 64.0, 0.5, true);
    assert!(corrected(&out), "{out:?}");
}

#[test]
fn sinking_into_the_ground_is_corrected() {
    let mut sim = joined(flat_world(2, 63).unwrap());
    let out = mv(&mut sim, 0.5, 63.4, 0.5, true);
    assert!(corrected(&out), "inside the grass block");
}

#[test]
fn a_ten_block_fall_hurts_seven() {
    let mut world = flat_world(2, 63).unwrap();
    world.set_block(0, 73, 0, state(BlockKind::Stone)).unwrap();
    world.spawn = [0.5, 74.0, 0.5];
    let mut sim = joined(world);
    let mut y = 74.0;
    let mut hurt = Vec::new();
    // Step off the pillar and fall a block a tick to the ground at 64.
    mv(&mut sim, 1.5, 74.0, 0.5, false);
    while y > 64.0 {
        y = (y - 1.0f64).max(64.0);
        hurt.extend(mv(&mut sim, 1.5, y, 0.5, y == 64.0));
    }
    assert!(
        !sim.violations.iter().any(|v| v.contains("wrongly")),
        "{:?}",
        sim.violations
    );
    let fall = data::generated::registries::DAMAGE_TYPE
        .iter()
        .position(|n| *n == "fall")
        .and_then(|i| i32::try_from(i).ok())
        .unwrap();
    assert!(
        hurt.iter()
            .any(|p| matches!(p, S2c::DamageEvent { source_type, .. } if *source_type == fall))
    );
    assert!((sim.health - 13.0).abs() < 1e-6, "health {}", sim.health);
    assert!(
        !hurt
            .iter()
            .any(|p| matches!(p, S2c::PlayerCombatKill { .. }))
    );
}

#[test]
fn a_fall_into_water_does_not_hurt() {
    let mut world = flat_world(2, 63).unwrap();
    for y in 61..=63 {
        world.set_block(1, y, 0, state(BlockKind::Water)).unwrap();
    }
    world.set_block(0, 73, 0, state(BlockKind::Stone)).unwrap();
    world.spawn = [0.5, 74.0, 0.5];
    let mut sim = joined(world);
    mv(&mut sim, 1.5, 74.0, 0.5, false);
    let mut y = 74.0;
    while y > 61.0 {
        y = (y - 1.0f64).max(61.0);
        mv(&mut sim, 1.5, y, 0.5, y == 61.0);
    }
    assert!((sim.health - 20.0).abs() < 1e-6, "health {}", sim.health);
}

#[test]
fn a_long_fall_kills_with_the_servers_words() {
    let mut world = flat_world(2, 63).unwrap();
    world.set_block(0, 99, 0, state(BlockKind::Stone)).unwrap();
    world.spawn = [0.5, 100.0, 0.5];
    let mut sim = joined(world);
    mv(&mut sim, 1.5, 100.0, 0.5, false);
    let mut out = Vec::new();
    let mut y = 100.0;
    while y > 64.0 {
        y = (y - 1.5f64).max(64.0);
        out.extend(mv(&mut sim, 1.5, y, 0.5, y == 64.0));
    }
    let death = out.iter().find_map(|p| match p {
        S2c::PlayerCombatKill { death, .. } => Some(death.clone()),
        _ => None,
    });
    let death = death.expect("a death message");
    assert_eq!(death.key.as_deref(), Some("death.attack.fall"));
    assert_eq!(death.victim.as_deref(), Some("rs-test"));
    assert!(sim.health <= 0.0);
}

#[test]
fn generated_worlds_are_pure_functions_of_their_seed() {
    let a = sim::terrain::world(42, 2).unwrap();
    let b = sim::terrain::world(42, 2).unwrap();
    let c = sim::terrain::world(43, 2).unwrap();
    assert_eq!(a, b);
    assert_ne!(a.columns, c.columns);
    // Every column decodes as a server's would.
    for (&(x, z), col) in &a.columns {
        let p = decode(
            protocol::State::Play,
            data::generated::packets::PlayClientbound::LevelChunkWithLight.id(),
            &col.body,
        )
        .unwrap();
        assert!(matches!(p, S2c::LevelChunkWithLight { x: px, z: pz, .. } if px == x && pz == z));
    }
}
