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

/// Spawner wall 7 and blaze-fight wall 13: with nothing to heal with (no food held, the food bar
/// under 18) a retreat to cover leads to no rest, so a blaze at 9 to 13 health is fought, and only
/// the danger line backs away. With food held the line of 13 stands.
#[test]
fn with_nothing_to_heal_with_the_blaze_line_is_the_danger_line() {
    use plan::defend::{Defend, Stance, retreat_line};
    let r = rules();
    let mut w = beside(EntityKind::Blaze, 3.5);
    w.player.health = Some(11.0);
    w.player.food = Some(17);
    assert_eq!(retreat_line(&w, &r), r.danger_health);
    let d = Defend::new(&w, 118, EntityKind::Blaze, &r, 0);
    assert_eq!(d.stance, Stance::Fight, "on with what it has");
    w.player.health = Some(r.danger_health);
    let d = Defend::new(&w, 118, EntityKind::Blaze, &r, 0);
    assert_eq!(d.stance, Stance::Retreat, "never under the danger line");
    w.player.health = Some(11.0);
    w.player.food = Some(18);
    assert_eq!(retreat_line(&w, &r), r.shooter_retreat_health);
    let d = Defend::new(&w, 118, EntityKind::Blaze, &r, 0);
    assert_eq!(d.stance, Stance::Retreat, "the food bar heals: to cover");
}

/// Review of bba8161: a blaze left alone after a fight that landed nothing (ignored for 600
/// ticks) stood in touch with no defend. One within `KEEP_OFF` is answered, ignored or not; one
/// farther off still waits.
#[test]
fn an_ignored_blaze_in_touch_is_answered() {
    let r = rules();
    let mut w = beside(EntityKind::Blaze, 2.0);
    w.player.health = Some(20.0);
    assert_eq!(foe(&w, &r, &[118]), Some((118, EntityKind::Blaze)));
    let w = beside(EntityKind::Blaze, 4.0);
    assert_eq!(foe(&w, &r, &[118]), None, "out of touch it waits");
}
