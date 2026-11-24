//! The shield (docs/problems/shield.md): when it goes up, where it faces, when it comes down and
//! what the ledger hears of it, on a hand-built world.

use data::{BlockKind, EntityKind, ItemKind};
use plan::shield::{self, Duty, Shield, ThreatKind};
use protocol::S2c;
use protocol::metadata::MetaValue;
use sim::{Config, Sim, flat_world};
use world::{Act, Event, World};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// Hands and feet free, no click to place, nothing in the fight's reach; and busy.
const FREE: Duty = Duty {
    free: true,
    placing: false,
    striking: None,
};
const BUSY: Duty = Duty {
    free: false,
    placing: false,
    striking: None,
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
    let first = s.step(&w, 100, &FREE);
    assert_eq!((first.yaw, first.act), (None, None), "the charge is new");
    let early = s.step(&w, 100 + shield::RAISE_AFTER - 1, &FREE);
    assert_eq!(early.act, None);
    let up = s.step(&w, 100 + shield::RAISE_AFTER, &FREE);
    assert_eq!(up.act, Some(Act::UseOffHand));
    assert!(yaw_near(up.yaw.unwrap(), 270.0));
    assert!(
        up.notes[0].starts_with("shield raised against blaze 7"),
        "{:?}",
        up.notes
    );
    // Held: no second raise, and `use_item` again only after `RESEND_TICKS`.
    let held = s.step(&w, 100 + shield::RAISE_AFTER + 1, &FREE);
    assert!(held.yaw.is_some() && held.act.is_none() && held.notes.is_empty());
    let again = s.step(&w, 100 + shield::RAISE_AFTER + shield::RESEND_TICKS, &FREE);
    assert_eq!(again.act, Some(Act::UseOffHand));
    assert!(again.notes.is_empty());
    assert_eq!(s.raises, 1);
    // The charge ends: held `HOLD_TICKS` more, then lowered with a note.
    charge(&mut w, 7, false);
    let at = 100 + shield::RAISE_AFTER + shield::RESEND_TICKS;
    let hold = s.step(&w, at + 1, &FREE);
    assert!(hold.yaw.is_some() && hold.act.is_none());
    let down = s.step(&w, at + shield::HOLD_TICKS, &FREE);
    assert_eq!((down.yaw, down.act), (None, Some(Act::ReleaseUse)));
    assert!(
        down.notes[0].starts_with("shield lowered"),
        "{:?}",
        down.notes
    );
    assert_eq!(s.step(&w, at + shield::HOLD_TICKS + 1, &FREE).act, None);
}

#[test]
fn no_shield_in_the_off_hand_no_raise_and_busy_hands_lower_it() {
    let mut w = floor(false);
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 6.5]);
    let mut s = Shield::default();
    assert_eq!(s.step(&w, 10, &FREE).act, None, "no shield held");
    let w = with_shield(w);
    assert_eq!(s.step(&w, 11, &BUSY).act, None, "the hands are busy");
    assert_eq!(s.step(&w, 12, &FREE).act, Some(Act::UseOffHand));
    let down = s.step(&w, 13, &BUSY);
    assert_eq!(down.act, Some(Act::ReleaseUse));
    assert!(down.notes[0].contains("the hands are needed"));
}

#[test]
fn a_fireball_that_ends_beside_the_bot_is_judged() {
    let mut w = with_shield(floor(false));
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 2.5]);
    let mut s = Shield::default();
    assert_eq!(s.step(&w, 10, &FREE).act, Some(Act::UseOffHand));
    s.step(&w, 16, &FREE);
    // Gone with the shield up five ticks or more, and no hit: blocked.
    w.entities.remove(&9);
    assert!(s.step(&w, 17, &FREE).notes.is_empty(), "not judged yet");
    w.session.server_ticks += shield::JUDGE_TICKS;
    let notes = s.step(&w, 21, &FREE).notes;
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
    s.step(&w, 30, &BUSY);
    w.entities.remove(&11);
    s.step(&w, 31, &BUSY);
    w.hits.push(world::Hit {
        at_tick: w.session.server_ticks,
        damage_type: "fireball".to_owned(),
        cause: Some(EntityKind::Blaze),
        direct: Some(EntityKind::SmallFireball),
    });
    w.session.server_ticks += shield::JUDGE_TICKS;
    let notes = s.step(&w, 35, &BUSY).notes;
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

/// Behind a raised shield the bot walks on, one tick in five, when its way is toward the threat;
/// any other way it stands and turns to the threat.
#[test]
fn a_raised_shield_slows_an_advance_and_stops_a_walk_away() {
    let toward = physics::Controls {
        forward: true,
        yaw: 300.0,
        ..physics::Controls::default()
    };
    let go = shield::controls(toward, 270.0, shield::SLOW_EVERY * 4);
    assert!(go.forward && yaw_near(go.yaw, 300.0), "{go:?}");
    let wait = shield::controls(toward, 270.0, shield::SLOW_EVERY * 4 + 1);
    assert!(!wait.forward && yaw_near(wait.yaw, 300.0), "{wait:?}");
    let away = physics::Controls {
        forward: true,
        jump: true,
        yaw: 90.0,
        ..physics::Controls::default()
    };
    let stand = shield::controls(away, 270.0, shield::SLOW_EVERY * 4);
    assert!(!stand.forward && !stand.jump && yaw_near(stand.yaw, 270.0));
}

/// Two blazes charged: the one charged longest is faced, its volley coming first.
#[test]
fn the_blaze_charged_longest_is_faced() {
    let mut w = with_shield(floor(false));
    add(&mut w, 7, EntityKind::Blaze, [8.5, 64.0, 0.5]);
    add(&mut w, 8, EntityKind::Blaze, [0.5, 64.0, 4.5]);
    charge(&mut w, 7, true);
    let mut s = Shield::default();
    s.step(&w, 100, &FREE);
    charge(&mut w, 8, true);
    s.step(&w, 120, &FREE);
    // Both due: the nearer one is 8, the longer charged 7 (east, yaw 270).
    let up = s.step(&w, 120 + shield::RAISE_AFTER, &FREE);
    assert!(yaw_near(up.yaw.unwrap(), 270.0), "{up:?}");
    assert!(up.notes[0].starts_with("shield raised against blaze 7"));
}

/// A shield under a fifth of its uses is said once, with the spares held.
#[test]
fn a_worn_shield_is_said_once() {
    let mut w = with_shield(floor(false));
    w.inventory.slots.insert(20, (ItemKind::Shield, 1));
    let mut s = Shield::default();
    assert!(s.step(&w, 10, &FREE).notes.is_empty(), "a new shield");
    w.inventory.wear.insert(
        shield::OFF_HAND,
        protocol::slot::Wear {
            damage: Some(300),
            max_damage: None,
        },
    );
    let notes = s.step(&w, 11, &FREE).notes;
    assert!(
        notes[0].starts_with("shield worn: 36 of 336 uses left, 1 spare shields held"),
        "{notes:?}"
    );
    assert!(s.step(&w, 12, &FREE).notes.is_empty(), "said once");
}

/// Block sounds are counted once each, though many share one server tick (the server's time
/// comes twenty ticks at a time).
#[test]
fn block_sounds_are_counted_once() {
    let mut w = with_shield(floor(false));
    let block = |w: &mut World| {
        let at_tick = w.session.server_ticks;
        w.sounds.push(world::Heard {
            at_tick,
            name: "item.shield.block".to_owned(),
            pos: None,
            entity: None,
            volume: 1.0,
        });
    };
    let mut s = Shield::default();
    s.step(&w, 10, &FREE);
    block(&mut w);
    s.step(&w, 11, &FREE);
    block(&mut w);
    block(&mut w);
    s.step(&w, 12, &FREE);
    assert_eq!(s.blocks_heard, 3);
    // One more at the old tick after the last step, then the server's time moves on.
    block(&mut w);
    w.session.server_ticks += 20;
    block(&mut w);
    s.step(&w, 13, &FREE);
    s.step(&w, 14, &FREE);
    assert_eq!(s.blocks_heard, 5);
}

fn rules() -> plan::make::SurvivalRules {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    bot::goals_table(goals).unwrap().survival.clone()
}

fn striking() -> Duty {
    Duty {
        striking: Some("WitherSkeleton 5 in reach at 1.5".to_owned()),
        ..FREE
    }
}

/// A mob in the fight's reach is struck, not shielded: no raise against a fireball still far,
/// a shield up comes down, but one up long enough stays up for a fireball about to land in
/// front (review of bba8161: a raise toward a charged blaze turned the bot from a wither
/// skeleton and dropped its strikes).
#[test]
fn a_mob_in_reach_is_struck_not_shielded() {
    let mut w = with_shield(floor(false));
    w.controls.yaw = 0.0;
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 8.5]);
    let mut s = Shield::default();
    let yielded = s.step(&w, 10, &striking());
    assert_eq!((yielded.yaw, yielded.act), (None, None), "{yielded:?}");
    assert!(
        yielded.notes[0].starts_with("shield yields to the strike: WitherSkeleton 5"),
        "{:?}",
        yielded.notes
    );
    assert!(
        s.step(&w, 11, &striking()).notes.is_empty(),
        "one note an episode"
    );
    assert_eq!((s.yields, s.yield_ticks), (1, 2));
    // Up with nothing in reach; then the mob closes: down at once, no hold.
    assert_eq!(s.step(&w, 12, &FREE).act, Some(Act::UseOffHand));
    let down = s.step(&w, 13, &striking());
    assert_eq!((down.yaw, down.act), (None, Some(Act::ReleaseUse)));
    assert!(
        down.notes.iter().any(|n| n.contains("the fight strikes")),
        "{:?}",
        down.notes
    );
    // Up five ticks, a fireball 1.5 off in front flying at the bot: kept up, the look held.
    assert_eq!(s.step(&w, 20, &FREE).act, Some(Act::UseOffHand));
    let e = w.entities.get_mut(&9).unwrap();
    e.pos = [0.5, 64.9, 2.0];
    e.velocity = Some([0.0, 0.0, -0.8]);
    let kept = s.step(&w, 25, &striking());
    assert!(
        kept.act.is_none() && kept.yaw.is_some_and(|y| yaw_near(y, 0.0)),
        "{kept:?}"
    );
    assert!(kept.notes[0].starts_with("shield kept up for fireball 9"));
    // The same fireball behind the look: the strike.
    w.controls.yaw = 180.0;
    let behind = s.step(&w, 26, &striking());
    assert_eq!(behind.act, Some(Act::ReleaseUse), "{behind:?}");
    // A fresh raise could not block it in time: no raise for it.
    w.controls.yaw = 0.0;
    assert_eq!(s.step(&w, 27, &striking()).act, None);
}

/// A click to place a block waits while a threat is due, the shield up instead; with none due it
/// is placed and the shield comes down for it (review of bba8161: a shield lowered for each
/// shelter click and raised the next tick was never up the 5 ticks the server asks).
#[test]
fn a_placing_click_waits_for_a_due_threat() {
    let placing = Duty {
        placing: true,
        ..FREE
    };
    let mut w = with_shield(floor(false));
    add(&mut w, 9, EntityKind::SmallFireball, [0.5, 65.0, 6.5]);
    let mut s = Shield::default();
    let up = s.step(&w, 10, &placing);
    assert!(up.hold_place && up.act == Some(Act::UseOffHand), "{up:?}");
    assert!(up.notes[0].starts_with("placing held for the shield"));
    let held = s.step(&w, 11, &placing);
    assert!(held.hold_place && held.act.is_none() && held.yaw.is_some());
    assert_eq!(s.places_held, 2);
    w.entities.remove(&9);
    let down = s.step(&w, 12, &placing);
    assert!(
        !down.hold_place && down.act == Some(Act::ReleaseUse),
        "{down:?}"
    );
    assert!(down.notes.iter().any(|n| n.contains("a block to place")));
    assert!(!s.step(&w, 13, &placing).hold_place);
    assert!(!s.guarding(13));
}

/// A strike the raised shield dropped is given back to the defend: its count and its cooldown.
#[test]
fn a_dropped_strike_is_given_back() {
    let mut w = floor(false);
    add(&mut w, 5, EntityKind::WitherSkeleton, [0.5, 64.0, 2.0]);
    let mut d = plan::defend::Defend::new(&w, 5, EntityKind::WitherSkeleton, &rules(), 100);
    d.swings = 3;
    d.last_swing = Some(110);
    let before = shield::SwingCount::of(&d);
    d.swings = 4;
    d.last_swing = Some(125);
    assert_eq!(before.restore(&mut d), 1);
    assert_eq!((d.swings, d.last_swing), (3, Some(110)));
    // A defend begun in the tick: from nothing.
    let mut fresh = plan::defend::Defend::new(&w, 5, EntityKind::WitherSkeleton, &rules(), 126);
    fresh.swings = 1;
    fresh.passing_swings = 1;
    fresh.last_swing = Some(126);
    assert_eq!(before.restore(&mut fresh), 1);
    assert_eq!(
        (fresh.swings, fresh.passing_swings, fresh.last_swing),
        (0, 0, None)
    );
    // What the planner names in reach.
    assert!(
        shield::striking(&w, &d, &rules().ranged)
            .is_some_and(|s| s.starts_with("WitherSkeleton 5 in reach")),
        "{:?}",
        shield::striking(&w, &d, &rules().ranged)
    );
    w.entities.get_mut(&5).unwrap().pos = [0.5, 64.0, 9.0];
    assert_eq!(shield::striking(&w, &d, &rules().ranged), None);
    // A blaze in reach is not yielded to (the shield blocks its point-blank volley); in touch it is.
    add(&mut w, 6, EntityKind::Blaze, [0.5, 64.5, 3.0]);
    assert_eq!(shield::striking(&w, &d, &rules().ranged), None);
    w.entities.get_mut(&6).unwrap().pos = [0.5, 64.0, 2.0];
    assert_eq!(
        shield::striking(&w, &d, &rules().ranged).as_deref(),
        Some("a blaze in touch")
    );
}
