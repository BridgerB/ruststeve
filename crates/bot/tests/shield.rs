//! The shield (docs/problems/shield.md): when it goes up, where it faces, when it comes down and
//! what the ledger hears of it, on a hand-built world.

use data::{BlockKind, EntityKind, ItemKind};
use plan::shield::{self, Shield, ThreatKind};
use protocol::S2c;
use protocol::metadata::MetaValue;
use sim::{Config, Sim, flat_world};
use world::{Act, Event, World};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

fn state(k: BlockKind) -> u32 {
    k.info().unwrap().default_state
}

/// The bot at (0.5, 64, 0.5) on a flat floor at y 63; `wall`: a wall three high at x 4, east of
/// it.
fn floor(wall: bool) -> World {
    let mut m = flat_world(2, 63).unwrap();
    if wall {
        for y in 64..=67 {
            for z in -3..=3 {
                m.set_block(4, y, z, state(BlockKind::NetherBricks))
                    .unwrap();
            }
        }
    }
    bot::lockstep::run(Sim::new(CONFIG, m), "rs-sh", 1, 30, 20, |_, _| {
        physics::Controls::default()
    })
    .world
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

fn charge(w: &mut World, id: i32, on: bool) {
    w.entities
        .get_mut(&id)
        .unwrap()
        .metadata
        .insert(16, MetaValue::Byte(u8::from(on)));
}

fn with_shield(mut w: World) -> World {
    w.inventory
        .slots
        .insert(shield::OFF_HAND, (ItemKind::Shield, 1));
    w
}

fn yaw_near(got: f64, want: f64) -> bool {
    let d = (got - want).rem_euclid(360.0);
    d.min(360.0 - d) < 1.0
}

#[test]
fn nothing_to_raise_against_in_an_empty_world() {
    let w = floor(false);
    assert!(!shield::should_raise(&w));
    assert_eq!(shield::facing(&w), None);
}

#[test]
fn a_charged_blaze_in_sight_raises_it_and_is_faced() {
    let mut w = floor(false);
    add(&mut w, 7, EntityKind::Blaze, [8.5, 64.0, 0.5]);
    assert!(!shield::should_raise(&w), "a blaze not charged");
    charge(&mut w, 7, true);
    assert!(shield::should_raise(&w));
    let t = shield::threats(&w);
    assert_eq!((t[0].id, t[0].kind), (7, ThreatKind::Blaze));
    // East (+x) is yaw 270, as the game counts it.
    assert!(yaw_near(shield::facing(&w).unwrap(), 270.0));
}

#[test]
fn a_charged_blaze_behind_a_wall_does_not() {
    let mut w = floor(true);
    add(&mut w, 7, EntityKind::Blaze, [8.5, 64.0, 0.5]);
    charge(&mut w, 7, true);
    assert!(!shield::should_raise(&w));
}

#[test]
fn a_fireball_coming_raises_it_and_one_flying_away_does_not() {
    let mut w = floor(false);
    add(&mut w, 7, EntityKind::Blaze, [8.5, 64.0, 0.5]);
    charge(&mut w, 7, true);
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 6.5]);
    // With no velocity yet it counts, and it is faced ahead of the blaze: south is yaw 0.
    let t = shield::threats(&w);
    assert_eq!((t[0].id, t[0].kind), (9, ThreatKind::Fireball));
    assert!(yaw_near(shield::facing(&w).unwrap(), 0.0));
    w.entities.get_mut(&9).unwrap().velocity = Some([0.0, 0.0, -0.8]);
    assert_eq!(shield::threats(&w)[0].id, 9, "flying at the bot");
    w.entities.get_mut(&9).unwrap().velocity = Some([0.0, 0.0, 0.8]);
    assert_eq!(shield::threats(&w)[0].id, 7, "flying away: the blaze only");
    charge(&mut w, 7, false);
    assert!(!shield::should_raise(&w));
}

#[test]
fn the_holder_waits_out_the_charge_raises_holds_and_lowers() {
    let mut w = with_shield(floor(false));
    add(&mut w, 7, EntityKind::Blaze, [8.5, 64.0, 0.5]);
    charge(&mut w, 7, true);
    let mut s = Shield::default();
    let first = s.step(&w, 100, true);
    assert_eq!((first.yaw, first.act), (None, None), "the charge is new");
    let early = s.step(&w, 100 + shield::RAISE_AFTER - 1, true);
    assert_eq!(early.act, None);
    let up = s.step(&w, 100 + shield::RAISE_AFTER, true);
    assert_eq!(up.act, Some(Act::UseOffHand));
    assert!(yaw_near(up.yaw.unwrap(), 270.0));
    assert!(
        up.notes[0].starts_with("shield raised against blaze 7"),
        "{:?}",
        up.notes
    );
    // Held: no second raise, and `use_item` again only after `RESEND_TICKS`.
    let held = s.step(&w, 100 + shield::RAISE_AFTER + 1, true);
    assert!(held.yaw.is_some() && held.act.is_none() && held.notes.is_empty());
    let again = s.step(&w, 100 + shield::RAISE_AFTER + shield::RESEND_TICKS, true);
    assert_eq!(again.act, Some(Act::UseOffHand));
    assert!(again.notes.is_empty());
    assert_eq!(s.raises, 1);
    // The charge ends: held `HOLD_TICKS` more, then lowered with a note.
    charge(&mut w, 7, false);
    let at = 100 + shield::RAISE_AFTER + shield::RESEND_TICKS;
    let hold = s.step(&w, at + 1, true);
    assert!(hold.yaw.is_some() && hold.act.is_none());
    let down = s.step(&w, at + shield::HOLD_TICKS, true);
    assert_eq!((down.yaw, down.act), (None, Some(Act::ReleaseUse)));
    assert!(
        down.notes[0].starts_with("shield lowered"),
        "{:?}",
        down.notes
    );
    assert_eq!(s.step(&w, at + shield::HOLD_TICKS + 1, true).act, None);
}

#[test]
fn no_shield_in_the_off_hand_no_raise_and_busy_hands_lower_it() {
    let mut w = floor(false);
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 6.5]);
    let mut s = Shield::default();
    assert_eq!(s.step(&w, 10, true).act, None, "no shield held");
    let w = with_shield(w);
    assert_eq!(s.step(&w, 11, false).act, None, "the hands are busy");
    assert_eq!(s.step(&w, 12, true).act, Some(Act::UseOffHand));
    let down = s.step(&w, 13, false);
    assert_eq!(down.act, Some(Act::ReleaseUse));
    assert!(down.notes[0].contains("the hands are needed"));
}

#[test]
fn a_fireball_that_ends_beside_the_bot_is_judged() {
    let mut w = with_shield(floor(false));
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 2.5]);
    let mut s = Shield::default();
    assert_eq!(s.step(&w, 10, true).act, Some(Act::UseOffHand));
    s.step(&w, 16, true);
    // Gone with the shield up five ticks or more, and no hit: blocked.
    w.entities.remove(&9);
    assert!(s.step(&w, 17, true).notes.is_empty(), "not judged yet");
    w.session.server_ticks += shield::JUDGE_TICKS;
    let notes = s.step(&w, 21, true).notes;
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("fireball 9 vanished with no damage, the shield up")),
        "{notes:?}"
    );
    assert_eq!((s.blocked, s.hits_up, s.hits_down), (1, 0, 0));
    // The next one lands with the shield down.
    let mut s = Shield::default();
    add(&mut w, 11, EntityKind::SmallFireball, [0.5, 65.0, 2.5]);
    s.step(&w, 30, false);
    w.entities.remove(&11);
    s.step(&w, 31, false);
    w.hits.push(world::Hit {
        at_tick: w.session.server_ticks,
        damage_type: "fireball".to_owned(),
        cause: Some(EntityKind::Blaze),
        direct: Some(EntityKind::SmallFireball),
    });
    w.session.server_ticks += shield::JUDGE_TICKS;
    let notes = s.step(&w, 35, false).notes;
    assert!(
        notes
            .iter()
            .any(|n| n.starts_with("fireball 11 hit the bot with the shield down")),
        "{notes:?}"
    );
    assert_eq!(s.hits_down, 1);
}

#[test]
fn a_held_shield_is_swapped_into_the_off_hand() {
    let mut w = floor(false);
    assert_eq!(shield::to_off_hand(&w), None, "none held");
    w.inventory.slots.insert(20, (ItemKind::Shield, 1));
    assert_eq!(
        shield::to_off_hand(&w),
        Some((
            20,
            Act::Click {
                slot: 20,
                button: 40,
                mode: 2
            }
        ))
    );
    assert_eq!(shield::to_off_hand(&with_shield(w)), None, "one is there");
}

/// The one call site: the planner's tick with a fireball coming stands, faces it, raises the
/// shield and says so; a strike is not sent with it.
#[test]
fn the_planner_raises_it_and_stands() {
    use plan::{Aim, Plan};
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::Hold, table);
    let mut w = with_shield(floor(false));
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 6.5]);
    let c = p.step(&w);
    let acts = p.take_acts();
    assert!(acts.contains(&Act::UseOffHand), "{acts:?}");
    assert!(!acts.iter().any(|a| matches!(a, Act::Attack { .. })));
    assert!(!c.forward && yaw_near(c.yaw, 0.0), "{c:?}");
    assert!(
        p.events
            .iter()
            .any(|e| e.what.starts_with("shield raised against fireball 9")),
        "{:#?}",
        p.events
    );
    assert!(p.shield.raised_at.is_some());
}
