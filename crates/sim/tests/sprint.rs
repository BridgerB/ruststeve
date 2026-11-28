//! Issue 56: the server's side of the sprint, as the 26.1.2 jar has it (config/sprint.toml,
//! crates/sim/src/life.rs): `player_command` START and STOP set the flag, a sprint on the ground
//! costs 0.1 exhaustion a metre and a sprint-jump 0.2, and the client's physics refuses the
//! sprint at food 6 or under.

use physics::{Body, Controls};
use protocol::metadata::MetaValue;
use protocol::{C2s, PlayerCommandAction, S2c, decode};
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

/// Joined at (0.5, 64, 0.5) on a flat world 4 columns each way, food and saturation as given.
fn joined(food: i32, saturation: f32) -> Sim {
    let mut sim = Sim::new(CONFIG, flat_world(4, 63).unwrap()).seeded(3);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    sim.survival.life.food = food;
    sim.survival.life.saturation = saturation;
    sim
}

fn command(action: PlayerCommandAction) -> C2s {
    C2s::PlayerCommand {
        entity: 1,
        action,
        data: 0,
    }
}

fn move_to(pos: [f64; 3], on_ground: bool) -> C2s {
    C2s::MovePlayerPosRot {
        x: pos[0],
        y: pos[1],
        z: pos[2],
        yaw: 0.0,
        pitch: 0.0,
        flags: u8::from(on_ground),
    }
}

/// The client's loop, as the bot's world runs it: the body stepped by physics knowing the
/// server's food, a START or STOP when the sprint changes, then the move, then a server tick.
/// The packets the server sent.
fn run(sim: &mut Sim, body: &mut Body, c: &Controls, ticks: u32) -> Vec<S2c> {
    let mut seen = Vec::new();
    for _ in 0..ticks {
        let next = {
            let solid = |x, y, z| sim.solid(x, y, z);
            let food = Some(sim.survival.life.food);
            physics::step_fed(
                &solid,
                &physics::no_fluid,
                &physics::plain_surface,
                body,
                c,
                food,
            )
        };
        if next.sprinting != body.sprinting {
            let action = if next.sprinting {
                PlayerCommandAction::StartSprinting
            } else {
                PlayerCommandAction::StopSprinting
            };
            seen.extend(send(sim, command(action)));
        }
        seen.extend(send(sim, move_to(next.pos, next.on_ground)));
        *body = next;
        seen.extend(tick(sim));
    }
    seen
}

fn standing() -> Body {
    Body {
        pos: [0.5, 64.0, 0.5],
        on_ground: true,
        ..Body::default()
    }
}

fn sprint() -> Controls {
    Controls {
        forward: true,
        sprint: true,
        ..Controls::default()
    }
}

#[test]
fn a_player_command_is_no_violation_and_sets_the_flag() {
    let mut sim = joined(20, 5.0);
    send(&mut sim, command(PlayerCommandAction::StartSprinting));
    assert!(sim.survival.life.sprinting);
    // The flag goes out as bit 0x08 of index 0 at the tick (`setSharedFlag(3, true)`).
    let echoed = tick(&mut sim).into_iter().any(|p| {
        matches!(p, S2c::SetEntityData { id: 1, entries }
            if entries.contains(&(0, MetaValue::Byte(sim::life::FLAG_SPRINTING))))
    });
    assert!(echoed, "the sprint flag echoed");
    send(&mut sim, command(PlayerCommandAction::StopSprinting));
    assert!(!sim.survival.life.sprinting);
    let cleared = tick(&mut sim).into_iter().any(|p| {
        matches!(p, S2c::SetEntityData { id: 1, entries }
            if entries.contains(&(0, MetaValue::Byte(0))))
    });
    assert!(cleared, "the flag's end echoed");
    assert_eq!(
        (
            sim.survival.life.sprint_starts,
            sim.survival.life.sprint_stops
        ),
        (1, 1)
    );
    assert!(sim.violations.is_empty(), "{:?}", sim.violations);
    // An action the model does not play is still counted.
    send(&mut sim, command(PlayerCommandAction::OpenInventory));
    assert_eq!(sim.violations.len(), 1, "{:?}", sim.violations);
}

#[test]
fn a_sprint_costs_a_tenth_a_metre_on_the_ground_and_nothing_in_the_air() {
    let mut sim = joined(20, 5.0);
    send(&mut sim, command(PlayerCommandAction::StartSprinting));
    // On the ground: ten moves of 25 cm.
    for i in 1..=10 {
        send(
            &mut sim,
            move_to([0.5, 64.0, 0.5 + 0.25 * f64::from(i)], true),
        );
        tick(&mut sim);
    }
    let life = &sim.survival.life;
    assert_eq!(life.sprint_cm, 250);
    assert!(
        (life.move_exhaustion - 0.25).abs() < 1e-5,
        "{}",
        life.move_exhaustion
    );
    assert!((life.exhaustion - 0.25).abs() < 1e-5, "{}", life.exhaustion);
    // In the air (the packet off the ground, no rise): the same moves cost nothing.
    for i in 11..=20 {
        send(
            &mut sim,
            move_to([0.5, 64.0, 0.5 + 0.25 * f64::from(i)], false),
        );
        tick(&mut sim);
    }
    assert_eq!(sim.survival.life.sprint_cm, 250);
    assert!((sim.survival.life.exhaustion - 0.25).abs() < 1e-5);
    // A walk on the ground costs nothing either.
    send(&mut sim, move_to([0.5, 64.0, 5.5], true));
    tick(&mut sim);
    send(&mut sim, command(PlayerCommandAction::StopSprinting));
    let before = sim.survival.life.exhaustion;
    for i in 1..=10 {
        send(
            &mut sim,
            move_to([0.5, 64.0, 5.5 + 0.25 * f64::from(i)], true),
        );
        tick(&mut sim);
    }
    assert!((sim.survival.life.exhaustion - before).abs() < 1e-6);
    assert!(sim.violations.is_empty(), "{:?}", sim.violations);
}

#[test]
fn a_sprint_jump_costs_two_tenths_and_a_walk_jump_five_hundredths() {
    let mut sim = joined(20, 5.0);
    let jump = |sim: &mut Sim| {
        let before = sim.survival.life.exhaustion;
        send(sim, move_to([0.5, 64.42, 0.5], false));
        tick(sim);
        send(sim, move_to([0.5, 64.0, 0.5], true));
        tick(sim);
        sim.survival.life.exhaustion - before
    };
    let walk = jump(&mut sim);
    assert!((walk - 0.05).abs() < 1e-6, "{walk}");
    send(&mut sim, command(PlayerCommandAction::StartSprinting));
    let run = jump(&mut sim);
    assert!((run - 0.2).abs() < 1e-6, "{run}");
    assert_eq!(sim.survival.life.sprint_jumps, 1);
    // No horizontal move: no distance charged.
    assert_eq!(sim.survival.life.sprint_cm, 0);
}

#[test]
fn forty_metres_sprinted_cost_one_food_point_with_no_saturation() {
    let mut sim = joined(20, 0.0);
    let mut body = standing();
    // About 39 m at 5.6 m/s, then on past 41.
    run(&mut sim, &mut body, &sprint(), 140);
    let cm = sim.survival.life.sprint_cm;
    assert!((3800..4000).contains(&cm), "{cm} cm after {:?}", body.pos);
    assert_eq!(sim.survival.life.food, 20, "under 40 m: no point yet");
    run(&mut sim, &mut body, &sprint(), 10);
    let cm = sim.survival.life.sprint_cm;
    assert!(cm > 4000, "{cm}");
    assert_eq!(sim.survival.life.food, 19, "one point at 40 m");
    // The charge is the jar's, by whole centimetres; the walked distance agrees within a few.
    let walked = (body.pos[2] - 0.5) * 100.0;
    assert!((walked - cm as f64).abs() < 50.0, "{walked} vs {cm}");
    let expected = cm as f32 * 0.001;
    let got = sim.survival.life.move_exhaustion;
    assert!((got - expected).abs() < 1e-3, "{got} vs {expected}");
    assert_eq!(sim.survival.life.sprint_starts, 1);
    assert!(sim.violations.is_empty(), "{:?}", sim.violations);
}

#[test]
fn at_food_six_the_body_refuses_the_sprint() {
    // Food 6: physics refuses the start, so no START goes out and nothing is charged.
    let mut sim = joined(6, 0.0);
    let mut body = standing();
    run(&mut sim, &mut body, &sprint(), 40);
    assert!(!body.sprinting);
    assert_eq!(sim.survival.life.sprint_starts, 0);
    assert_eq!(sim.survival.life.sprint_cm, 0);
    // Food 7: it starts; sprinted down to 6, the body stops it and the client says STOP.
    let mut sim = joined(7, 0.0);
    sim.survival.life.exhaustion = 3.9;
    let mut body = standing();
    run(&mut sim, &mut body, &sprint(), 60);
    assert_eq!(sim.survival.life.food, 6);
    assert!(!body.sprinting);
    assert!(!sim.survival.life.sprinting);
    assert_eq!(
        (
            sim.survival.life.sprint_starts,
            sim.survival.life.sprint_stops
        ),
        (1, 1)
    );
    assert!(sim.violations.is_empty(), "{:?}", sim.violations);
}

#[test]
fn exhaustion_is_capped_at_forty() {
    let mut sim = joined(20, 5.0);
    sim.survival.life.exhaustion = 39.95;
    send(&mut sim, command(PlayerCommandAction::StartSprinting));
    // A sprint-jump's 0.2 tops it out at 40 (`FoodData.addExhaustion`).
    send(&mut sim, move_to([0.5, 64.42, 0.5], false));
    assert!((sim.survival.life.exhaustion - 40.0).abs() < 1e-6);
}

/// The value of `key` in `[table]` of config/sprint.toml, read by hand: the file is the
/// source, the constants its mirror.
fn sprint_toml(table: &str, key: &str) -> f64 {
    let text = include_str!("../../../config/sprint.toml");
    let mut current = "";
    for line in text.lines().map(str::trim) {
        if let Some(t) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = t;
            continue;
        }
        if current != table || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return v.trim().parse().unwrap();
        }
    }
    panic!("no {key} in [{table}]")
}

#[test]
fn the_constants_are_config_sprint_toml() {
    use sim::life::{
        EXHAUSTION_CAP, EXHAUSTION_JUMP, EXHAUSTION_PER_POINT, EXHAUSTION_SPRINT_JUMP,
        EXHAUSTION_SPRINT_PER_METRE, EXHAUSTION_SWIM_PER_METRE, FLAG_SPRINTING,
        SPRINT_HIT_STRENGTH,
    };
    let same = |c: f32, table: &str, key: &str| {
        let v = sprint_toml(table, key);
        // The file writes the float's round value; the constant is that float.
        assert!((f64::from(c) - v).abs() < 1e-6, "{key}: {c} vs {v}");
    };
    same(
        EXHAUSTION_SPRINT_PER_METRE,
        "exhaustion",
        "sprint_per_metre",
    );
    same(EXHAUSTION_SWIM_PER_METRE, "exhaustion", "swim_per_metre");
    same(EXHAUSTION_JUMP, "exhaustion", "jump");
    same(EXHAUSTION_SPRINT_JUMP, "exhaustion", "sprint_jump");
    same(EXHAUSTION_CAP, "exhaustion", "cap");
    same(EXHAUSTION_PER_POINT, "exhaustion", "per_point");
    assert_eq!(
        f64::from(FLAG_SPRINTING),
        sprint_toml("packet", "shared_flag_bit")
    );
    assert_eq!(
        SPRINT_HIT_STRENGTH,
        sprint_toml("stop", "sprint_hit_strength_above")
    );
}
