//! Issue 17 (docs/problems/endermen-leave.md): the stare filter (`plan::look`, the 26.1.2 jar's
//! `EnderMan.isBeingStaredBy`, probe 45472254333), the provoked enderman, ticking, the provisions
//! counts and the fortress footprint, on hand-built worlds.

use data::{EntityKind, ItemKind};
use physics::Controls;
use plan::endermen::{
    self, FORTRESS_PAD, Sighting, angry, filler_held, in_footprint, near_ticking,
    pickaxe_uses_left, ticking,
};
use plan::look::{ENDERMAN_EYE, keep_off, stares};
use protocol::S2c;
use protocol::metadata::MetaValue;
use world::{Event, World};

const FEET: [f64; 3] = [0.5, 64.0, 0.5];

fn eye() -> [f64; 3] {
    [FEET[0], FEET[1] + physics::EYE_HEIGHT, FEET[2]]
}

fn bare() -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: FEET,
        ..physics::Body::default()
    });
    w.simulation_distance = Some(3);
    w
}

fn with(mut w: World, id: i32, kind: EntityKind, at: [f64; 3]) -> World {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
    w
}

fn enderman_at(at: [f64; 3]) -> World {
    with(bare(), 41, EntityKind::Enderman, at)
}

/// The look straight at an enderman's eye point, from the bot's eye.
fn at_its_eye(at: [f64; 3]) -> (f32, f32) {
    let (yaw, pitch) = plan::walk::look_at(eye(), [at[0], at[1] + ENDERMAN_EYE, at[2]]);
    #[allow(clippy::cast_possible_truncation)]
    (yaw as f32, pitch)
}

#[test]
fn a_look_at_its_eye_stares_and_is_kept_off() {
    let at = [10.5, 64.0, 0.5];
    let w = enderman_at(at);
    let (yaw, pitch) = at_its_eye(at);
    assert_eq!(stares(&w, eye(), yaw, pitch), Some(41));
    let c = Controls {
        yaw: f64::from(yaw),
        pitch: Some(pitch),
        forward: true,
        ..Controls::default()
    };
    let (kept, id) = keep_off(&w, c);
    assert_eq!(id, Some(41));
    assert!(kept.forward, "the walk goes on");
    assert!(
        (kept.yaw - c.yaw).abs() < f64::EPSILON,
        "the heading is the yaw"
    );
    let p = kept.pitch.unwrap();
    assert!((p - pitch).abs() > 1.0, "the pitch changed: {p}");
    assert_eq!(stares(&w, eye(), yaw, p), None);
}

/// The dig's look at a block 20 east, 2 up, passes 2.8 degrees from the eye of an enderman 10
/// east: inside the jar's cone at 10 blocks (about 4 degrees). Sent with a changed look.
#[test]
fn a_dig_whose_look_crosses_its_head_is_sent_with_a_changed_look() {
    let w = enderman_at([10.5, 64.0, 0.5]);
    let (yaw, pitch) = plan::walk::look_at(eye(), [20.5, 66.5, 0.5]);
    let c = Controls {
        yaw,
        pitch: Some(pitch),
        ..Controls::default()
    };
    #[allow(clippy::cast_possible_truncation)]
    let y = yaw as f32;
    assert_eq!(stares(&w, eye(), y, pitch), Some(41));
    let (kept, id) = keep_off(&w, c);
    assert_eq!(id, Some(41));
    assert_eq!(stares(&w, eye(), y, kept.pitch.unwrap()), None);
}

#[test]
fn a_look_away_or_beyond_its_range_is_not_changed() {
    // Beyond the follow range of 64 the goal never takes the player: the look is left alone.
    let far = [70.5, 64.0, 0.5];
    let w = enderman_at(far);
    let (yaw, pitch) = at_its_eye(far);
    assert_eq!(stares(&w, eye(), yaw, pitch), None);
    // Within range, a look at its feet 30 blocks off is about 5 degrees under its eye,
    // outside the cone (2.3 degrees at 30).
    let near = [30.5, 64.0, 0.5];
    let w = enderman_at(near);
    let (yaw, pitch) = plan::walk::look_at(eye(), [30.5, 64.0, 0.5]);
    #[allow(clippy::cast_possible_truncation)]
    let y = yaw as f32;
    assert_eq!(stares(&w, eye(), y, pitch), None);
    let c = Controls {
        yaw,
        pitch: Some(pitch),
        ..Controls::default()
    };
    assert_eq!(keep_off(&w, c), (c, None));
}

#[test]
fn a_carved_pumpkin_on_the_head_stares_at_nothing() {
    let at = [10.5, 64.0, 0.5];
    let mut w = enderman_at(at);
    w.inventory.slots.insert(5, (ItemKind::CarvedPumpkin, 1));
    let (yaw, pitch) = at_its_eye(at);
    assert_eq!(stares(&w, eye(), yaw, pitch), None);
}

#[test]
fn an_enderman_ticks_inside_the_simulation_distance() {
    let w = bare();
    // The player's chunk is 0; chunk 3 ticks, chunk 4 does not (a square of 3).
    assert!(ticking(&w, [63.9, 64.0, 0.5]));
    assert!(!ticking(&w, [64.1, 64.0, 0.5]));
    assert!(ticking(&w, [-47.9, 64.0, 63.0]));
    assert!(!ticking(&w, [0.5, 64.0, -49.0]));
    let mut unknown = bare();
    unknown.simulation_distance = None;
    assert!(!ticking(&unknown, [1.5, 64.0, 0.5]));
}

#[test]
fn the_stage_is_a_ticking_enderman_within_32() {
    let w = enderman_at([31.5, 64.0, 0.5]);
    assert!(matches!(near_ticking(&w), Some(Sighting { id: 41, .. })));
    let w = enderman_at([33.5, 64.0, 0.5]);
    assert_eq!(near_ticking(&w), None);
    assert_eq!(endermen::endermen(&w).len(), 1);
}

#[test]
fn a_provoked_enderman_is_its_creepy_flag_or_its_hit() {
    let mut w = enderman_at([6.5, 64.0, 0.5]);
    assert_eq!(angry(&w), None, "a calm enderman is left alone");
    w.entities
        .get_mut(&41)
        .unwrap()
        .metadata
        .insert(endermen::CREEPY_INDEX, MetaValue::Bool(true));
    assert_eq!(angry(&w), Some(41));
    let mut w = enderman_at([6.5, 64.0, 0.5]);
    w.hits.push(world::Hit {
        at_tick: w.session.server_ticks,
        damage_type: "mob_attack".to_owned(),
        cause: Some(EntityKind::Enderman),
        direct: Some(EntityKind::Enderman),
    });
    assert_eq!(angry(&w), Some(41));
}

/// Never a swing at an enderman in issue 17, provoked or not: the defend row never takes one.
#[test]
fn an_enderman_is_never_the_foe() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: plan::make::Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let rules = plan::make::Table::new(&goals).unwrap().survival;
    let mut w = enderman_at([2.5, 64.0, 0.5]);
    w.hits.push(world::Hit {
        at_tick: w.session.server_ticks,
        damage_type: "mob_attack".to_owned(),
        cause: Some(EntityKind::Enderman),
        direct: Some(EntityKind::Enderman),
    });
    assert_eq!(plan::defend::foe(&w, &rules, &[]), None);
    assert_eq!(plan::defend::pressing(&w, &rules, -1), None);
    assert!(rules.pickaxe_reserve >= 40, "the reserve is in the config");
    assert!(rules.travel_health > rules.danger_health);
}

#[test]
fn provisions_count_pickaxe_uses_and_filler() {
    let mut w = bare();
    w.inventory.slots.insert(36, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(37, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(9, (ItemKind::Netherrack, 64));
    w.inventory.slots.insert(10, (ItemKind::Cobblestone, 20));
    w.inventory.slots.insert(11, (ItemKind::BlazeRod, 7));
    assert_eq!(pickaxe_uses_left(&w), 262, "two fresh stone pickaxes");
    assert_eq!(filler_held(&w), 84, "rods are not filler");
}

#[test]
fn the_fortress_footprint_is_the_bricks_bounds_padded() {
    let b = (100, 120, 160, 200);
    assert!(in_footprint(b, (129, 64, 159)));
    assert!(in_footprint(b, (100 - FORTRESS_PAD, 30, 120)));
    assert!(!in_footprint(b, (100 - FORTRESS_PAD - 1, 64, 150)));
    assert!(!in_footprint(b, (130, 64, 200 + FORTRESS_PAD + 1)));
}

/// A provoked enderman (17e.3) cuts in ahead of every task: the planner notes it, holds still with
/// its eyes down, builds its pocket where it stands (none to build on a world with no blocks
/// loaded: it holds), and never strikes. (The model's enderman, angered by hand, never came to
/// strike in a closed loop, so the rule is held here on the planner itself.)
#[test]
fn a_provoked_enderman_is_answered_by_the_pocket_never_a_swing() {
    let mut w = enderman_at([6.5, 64.0, 0.5]);
    w.entities
        .get_mut(&41)
        .unwrap()
        .metadata
        .insert(endermen::CREEPY_INDEX, MetaValue::Bool(true));
    let mut plan = plan::Plan::new(plan::Aim::Hold);
    let c = plan.step(&w);
    assert_eq!(c.pitch, Some(endermen::EYES_DOWN));
    assert!(!c.forward);
    let notes: Vec<String> = plan.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        notes
            .iter()
            .any(|n| n == "provoked enderman 41: into a sealed pocket"),
        "{notes:?}"
    );
    assert!(
        notes.iter().any(|n| n == "held from enderman 41"),
        "{notes:?}"
    );
    assert!(
        plan.take_acts()
            .iter()
            .all(|a| !matches!(a, world::Act::Attack { .. })),
        "never a swing"
    );
    assert_eq!(plan.ender.threat, Some(41));
    // Calm again: the threat is let go, with a note.
    w.entities
        .get_mut(&41)
        .unwrap()
        .metadata
        .insert(endermen::CREEPY_INDEX, MetaValue::Bool(false));
    plan.step(&w);
    assert_eq!(plan.ender.threat, None);
    assert!(
        plan.events
            .iter()
            .any(|e| e.what == "enderman 41 calm or gone"),
        "{:?}",
        plan.events
    );
}

const NETHER: &str = "minecraft:the_nether";

fn state(k: data::BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// A block seen exposed (air above it) in memory, as the world records one.
fn seen(w: &mut World, pos: (i32, i32, i32), k: data::BlockKind) {
    let air = Some(state(data::BlockKind::Air));
    let s =
        memory::Seen::check(NETHER, pos, state(k), [None, air, None, None, None, None]).unwrap();
    w.memory.record_sign(&s);
    w.memory.record(s, 0);
}

/// Race 45481633896: three bots at the spawner's bridge (y 61) walked at soul cells seen at y 22
/// under the fortress and into wither skeletons; the valley west of the spawner, at their own
/// level, was refused whole for the fortress's bounding box. The target is the soul cell nearest
/// the feet within 12 of their height, never one with nether bricks seen beside it.
#[test]
fn the_valley_target_is_soul_ground_at_the_bots_level_away_from_bricks() {
    let mut w = bare();
    w.dimension = Some(world::Dimension {
        type_id: 3,
        name: NETHER.to_owned(),
        min_y: Some(0),
        height: Some(256),
    });
    // Column (2, 0): soul soil at the lava sea's level (y 22), 13 cells (a known valley).
    for x in 32..45 {
        seen(&mut w, (x, 22, 3), data::BlockKind::SoulSoil);
    }
    // Column (-2, 0): a wart room: soul sand at y 63 beside nether bricks.
    for x in -32..-19 {
        seen(&mut w, (x, 63, 3), data::BlockKind::SoulSand);
    }
    for x in -32..-19 {
        seen(&mut w, (x, 64, 5), data::BlockKind::NetherBricks);
    }
    let (none, walled) = endermen::valley_target(&w, &[]);
    assert_eq!(none, None, "deep soil and a wart room are no target");
    assert_eq!(walled, 13);
    // Column (0, 3): valley floor at y 60, no bricks: the target, over the nearest cell.
    for x in 0..13 {
        seen(&mut w, (x, 60, 50), data::BlockKind::SoulSand);
    }
    let (target, _) = endermen::valley_target(&w, &[]);
    assert_eq!(target, Some((0, 61, 50)));
    // A place given up is skipped, and the cells near it with it.
    let (next, _) = endermen::valley_target(&w, &[(0, 61, 50)]);
    assert_eq!(next, Some((9, 61, 50)));
}
