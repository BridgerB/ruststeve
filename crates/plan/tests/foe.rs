//! Blaze wall 15 (race 45102450333, lane 5): a spider in a pit 4.5 under the eye was fought 96
//! times, never struck and never striking. A foe a fight never struck nor was hurt by waits.

use data::EntityKind;
use plan::defend::foe;
use plan::make::{Goals, Table};
use protocol::S2c;
use world::{Event, World};

fn rules() -> plan::make::SurvivalRules {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    Table::new(&goals).unwrap().survival
}

fn spider_beside() -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: [0.5, 0.0, 0.5],
        ..physics::Body::default()
    });
    w.step(&Event::Packet(S2c::AddEntity {
        id: 95,
        uuid: [0; 16],
        kind: EntityKind::Spider.id(),
        x: 3.0,
        y: 0.0,
        z: 0.5,
    }));
    w
}

#[test]
fn a_spider_within_reach_is_answered_unless_ignored() {
    let r = rules();
    let w = spider_beside();
    assert_eq!(foe(&w, &r, &[]), Some((95, EntityKind::Spider)));
    assert_eq!(
        foe(&w, &r, &[95]),
        None,
        "an ignored spider that never hit waits"
    );
}

fn zombie_in_reach(health: f32) -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: [0.5, 0.0, 0.5],
        ..physics::Body::default()
    });
    w.step(&Event::Packet(S2c::AddEntity {
        id: 7,
        uuid: [0; 16],
        kind: EntityKind::Zombie.id(),
        x: 2.5,
        y: 0.0,
        z: 0.5,
    }));
    w.step(&Event::Packet(S2c::SetHealth {
        health,
        food: 20,
        saturation: 0.0,
    }));
    w
}

/// Never fight under danger health (overworld-leg wall 1): a retreat under the line sends no
/// swing at the zombie in its face; the same retreat with its health back over the line does.
#[test]
fn a_retreat_strikes_only_over_the_danger_line() {
    use plan::defend::{Defend, Stance};
    let r = rules();
    let hurt = zombie_in_reach(6.0);
    let mut d = Defend::new(&hurt, 7, EntityKind::Zombie, &r, 0);
    assert_eq!(d.stance, Stance::Retreat);
    for tick in 1..=60 {
        let (_, act, _) = d.step(&hurt, &r, tick);
        assert!(act.is_none(), "an act under the danger line: {act:?}");
    }
    assert_eq!(d.swings, 0);
    let well = zombie_in_reach(20.0);
    for tick in 61..=120 {
        d.step(&well, &r, tick);
    }
    assert_eq!(d.stance, Stance::Retreat);
    assert!(d.swings >= 1, "a retreat over the line never struck");
}

fn beside(kind: EntityKind, at: f64) -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: [0.5, 0.0, 0.5],
        ..physics::Body::default()
    });
    w.step(&Event::Packet(S2c::AddEntity {
        id: 118,
        uuid: [0; 16],
        kind: kind.id(),
        x: at,
        y: 0.0,
        z: 0.5,
    }));
    w
}

/// Spawner wall 1 (race 45334573113, lanes 0 and 2): two armoured bots backed away from a wither
/// skeleton for 700 and 900 ticks, it walked as fast as they did, and both died with no swing.
/// One within reach is fought, as a zombie is; the danger line still backs away. A creeper is
/// backed away from, from farther out, as before.
#[test]
fn a_wither_skeleton_within_reach_is_fought() {
    use plan::defend::{Defend, Stance};
    let r = rules();
    let mut w = beside(EntityKind::WitherSkeleton, 3.5);
    w.player.health = Some(20.0);
    assert_eq!(foe(&w, &r, &[]), Some((118, EntityKind::WitherSkeleton)));
    let d = Defend::new(&w, 118, EntityKind::WitherSkeleton, &r, 0);
    assert_eq!(d.stance, Stance::Fight);
    w.player.health = Some(r.danger_health);
    let d = Defend::new(&w, 118, EntityKind::WitherSkeleton, &r, 0);
    assert_eq!(
        d.stance,
        Stance::Retreat,
        "never a fight at the danger line"
    );
    // Beyond the defence radius it is left alone: no retreat from 8 blocks.
    let far = beside(EntityKind::WitherSkeleton, 7.5);
    assert_eq!(foe(&far, &r, &[]), None);
    let mut c = beside(EntityKind::Creeper, 7.5);
    c.player.health = Some(20.0);
    assert_eq!(foe(&c, &r, &[]), Some((118, EntityKind::Creeper)));
    let d = Defend::new(&c, 118, EntityKind::Creeper, &r, 0);
    assert_eq!(d.stance, Stance::Retreat);
}

fn add(w: &mut World, id: i32, kind: EntityKind, pos: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: pos[0],
        y: pos[1],
        z: pos[2],
    }));
}

fn standing() -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: [0.5, 0.0, 0.5],
        ..physics::Body::default()
    });
    w
}

/// Blaze-fight wall 6 (race 45337951964): a defend held on a blaze 10 blocks off turns to the
/// wither skeleton that comes within `defend_radius`; a zombified piglin that has not hit is not
/// turned to, and with nothing near there is nothing to turn to.
#[test]
fn a_melee_foe_in_reach_presses_a_defend_on_a_far_shooter() {
    let r = rules();
    let mut w = standing();
    add(&mut w, 7, EntityKind::Blaze, [10.5, 1.0, 0.5]);
    assert_eq!(plan::defend::pressing(&w, &r, 7), None);
    add(&mut w, 8, EntityKind::ZombifiedPiglin, [2.5, 0.0, 0.5]);
    assert_eq!(
        plan::defend::pressing(&w, &r, 7),
        None,
        "a neutral mob waits"
    );
    add(&mut w, 9, EntityKind::WitherSkeleton, [3.5, 0.0, 0.5]);
    assert_eq!(
        plan::defend::pressing(&w, &r, 7),
        Some((9, EntityKind::WitherSkeleton))
    );
}

/// The same race: a blaze 18.9 over the eye was taken as the foe. A shooter whose feet are more
/// than a strike's reach over the eye is not gone after; one at the bot's height is.
#[test]
fn a_shooter_far_overhead_is_not_gone_after() {
    let r = rules();
    let mut w = standing();
    w.player.health = Some(20.0);
    add(&mut w, 7, EntityKind::Blaze, [6.5, 12.0, 0.5]);
    assert_eq!(foe(&w, &r, &[]), None);
    add(&mut w, 8, EntityKind::Blaze, [8.5, 1.0, 0.5]);
    assert_eq!(foe(&w, &r, &[]), Some((8, EntityKind::Blaze)));
}

/// Blaze-fight wall 7: a fireball whose line passes within 1.5 blocks of the body's middle is
/// stepped out of, away from the line; one passing 2 blocks off, or flying away, is left; one
/// with no velocity yet is taken to fly at the body.
#[test]
fn a_fireball_s_line_is_stepped_out_of() {
    use plan::defend::dodge_side;
    let mid = [0.5, 64.9, 0.5];
    let west = Some([-1.0, 0.0, 0.0]);
    // Its line a block south (+z) of the body: step north.
    let s = dodge_side(mid, [10.5, 64.9, 1.5], west).expect("a dodge");
    assert!(s[1] < -0.9 && s[0].abs() < 0.1, "{s:?}");
    // Two blocks off: no dodge.
    assert_eq!(dodge_side(mid, [10.5, 64.9, 2.5], west), None);
    // Flying away: no dodge.
    assert_eq!(
        dodge_side(mid, [10.5, 64.9, 0.5], Some([1.0, 0.0, 0.0])),
        None
    );
    // Dead on: a step across its heading.
    let s = dodge_side(mid, [10.5, 64.9, 0.5], west).expect("a dodge");
    assert!(s[0].abs() < 0.1 && s[1].abs() > 0.9, "{s:?}");
    // No velocity yet: as dead on.
    let s = dodge_side(mid, [10.5, 64.9, 0.5], None).expect("a dodge");
    assert!(s[0].abs() < 0.1 && s[1].abs() > 0.9, "{s:?}");
}
