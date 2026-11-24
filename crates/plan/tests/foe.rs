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
