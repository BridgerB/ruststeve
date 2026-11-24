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
