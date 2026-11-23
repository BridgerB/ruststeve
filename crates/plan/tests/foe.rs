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
