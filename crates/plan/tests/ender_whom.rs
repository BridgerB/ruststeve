//! Issue 80 (docs/problems/pumpkin.md): endermen angry at whom, in the End, on hand-built worlds.
//! An enderman's target is its latest hurter with an entity or a stare (the jar, probes
//! 46160185304 and 46160754145), so one the dragon hurt is the dragon's: no water poured for it.
//! One the bot struck, or one that comes at the bot, is the bot's: the water goes down.

use data::{EntityKind, ItemKind};
use plan::endermen::{BotsWhy, CREEPY_INDEX, WhomRules, WhomTrack, Whose, came_at, whose};
use plan::{Aim, Plan};
use protocol::S2c;
use protocol::metadata::MetaValue;
use protocol::slot::{Slot, Wear};
use world::{Act, Event, Hurt, World};

const FEET: [f64; 3] = [10.5, 60.0, 10.5];
const ENDER: i32 = 41;
const DRAGON: i32 = 68;

fn table() -> std::sync::Arc<plan::make::Table> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: plan::make::Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    std::sync::Arc::new(plan::make::Table::new(&goals).unwrap())
}

/// The End, the bot on the ground at `FEET` with a water bucket in hand, and an angry enderman
/// at `at`.
fn the_end(at: [f64; 3]) -> World {
    let mut w = World::new();
    w.player.entity_id = Some(1);
    w.dimension = Some(world::Dimension {
        type_id: 2,
        name: plan::dragon::THE_END.to_owned(),
        min_y: Some(0),
        height: Some(256),
    });
    w.body = Some(physics::Body {
        pos: FEET,
        on_ground: true,
        ..physics::Body::default()
    });
    w.step(&Event::Packet(S2c::SetPlayerInventory {
        slot: 0,
        item: Slot::Item {
            item: ItemKind::WaterBucket.id(),
            count: 1,
            wear: Wear {
                damage: None,
                max_damage: None,
            },
        },
    }));
    w.step(&Event::Packet(S2c::AddEntity {
        id: ENDER,
        uuid: [0; 16],
        kind: EntityKind::Enderman.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
    w.entities
        .get_mut(&ENDER)
        .unwrap()
        .metadata
        .insert(CREEPY_INDEX, MetaValue::Bool(true));
    w.session.server_ticks = 1000;
    w
}

fn hurt(w: &mut World, at: u64, by_bot: bool, cause: Option<i32>, kind: &str) {
    w.entities.get_mut(&ENDER).unwrap().hurts.push(Hurt {
        at_tick: at,
        damage_type: kind.to_owned(),
        by_bot,
        cause,
    });
}

fn poured(p: &mut Plan, w: &World, tick: u64) -> bool {
    let _ = p.ender_water_step(w, tick);
    let acts = p.take_acts();
    acts.contains(&Act::UseItem)
        && p.events
            .iter()
            .any(|e| e.what.starts_with("end: water poured at"))
}

#[test]
fn an_enderman_the_dragon_hurt_gets_no_water() {
    let mut w = the_end([16.5, 60.0, 10.5]);
    hurt(&mut w, 990, false, Some(DRAGON), "minecraft:mob_attack");
    assert!(matches!(
        whose(&w, ENDER, None, &[], None),
        Whose::Dragons {
            by: Some(DRAGON),
            ..
        }
    ));
    let mut p = Plan::with_table(Aim::Race, table());
    assert!(!poured(&mut p, &w, 1));
    let ev: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        ev.iter()
            .any(|e| e == "enderman on the dragon: pour skipped"),
        "{ev:#?}"
    );
    assert_eq!(p.ender.dragon_skips.get("pour"), Some(&1));
}

#[test]
fn our_own_water_on_it_after_the_dragon_does_not_make_it_the_bots() {
    let mut w = the_end([16.5, 60.0, 10.5]);
    hurt(&mut w, 980, false, Some(DRAGON), "minecraft:indirect_magic");
    // A drown has no entity: it sets no target.
    hurt(&mut w, 995, false, None, "minecraft:drown");
    assert!(matches!(
        whose(&w, ENDER, None, &[], None),
        Whose::Dragons { at: 980, .. }
    ));
}

#[test]
fn an_enderman_the_bot_struck_gets_the_water() {
    let mut w = the_end([16.5, 60.0, 10.5]);
    hurt(&mut w, 980, false, Some(DRAGON), "minecraft:mob_attack");
    hurt(&mut w, 990, true, Some(1), "minecraft:player_attack");
    assert_eq!(
        whose(&w, ENDER, None, &[], None),
        Whose::Bots(BotsWhy::StruckByBot)
    );
    let mut p = Plan::with_table(Aim::Race, table());
    assert!(poured(&mut p, &w, 1), "{:#?}", p.events);
}

#[test]
fn an_angry_enderman_with_no_hurt_seen_is_the_bots() {
    let w = the_end([16.5, 60.0, 10.5]);
    assert_eq!(
        whose(&w, ENDER, None, &[], None),
        Whose::Bots(BotsWhy::NoHurtSeen)
    );
    let mut p = Plan::with_table(Aim::Race, table());
    assert!(poured(&mut p, &w, 1));
}

#[test]
fn an_enderman_the_dragon_hurt_that_comes_at_the_bot_gets_the_water() {
    let mut w = the_end([24.5, 60.0, 10.5]);
    hurt(&mut w, 990, false, Some(DRAGON), "minecraft:mob_attack");
    let mut p = Plan::with_table(Aim::Race, table());
    // 14 off, then 9 off 20 server ticks later: 5 closer within 40.
    assert!(!poured(&mut p, &w, 1));
    w.session.server_ticks = 1020;
    w.entities.get_mut(&ENDER).unwrap().pos = [19.5, 60.0, 10.5];
    assert!(poured(&mut p, &w, 2), "{:#?}", p.events);
    assert!(
        p.events
            .iter()
            .any(|e| e.what == "enderman 41 taken as the bot's: closing in")
    );
}

#[test]
fn closing_and_a_teleport_landing_near_are_the_bots() {
    let r = WhomRules {
        close: 4.0,
        close_ticks: 40,
        land_near: 8.0,
        jump: 4.0,
        melee: 5.0,
    };
    let track = |seen: Vec<(u64, f64, [f64; 3])>| WhomTrack {
        id: ENDER,
        seen,
        ..WhomTrack::default()
    };
    // Drift: 2 closer in 40 ticks.
    let t = track(vec![
        (0, 20.0, [20.0, 0.0, 0.0]),
        (40, 18.0, [18.0, 0.0, 0.0]),
    ]);
    assert_eq!(came_at(&t, &r), None);
    let t = track(vec![
        (0, 20.0, [20.0, 0.0, 0.0]),
        (30, 15.0, [15.0, 0.0, 0.0]),
    ]);
    assert_eq!(came_at(&t, &r), Some(BotsWhy::Closing));
    // A teleport round the bot (a jump of 10 in a tick, only 2 closer) landing 6 off.
    let t = track(vec![(0, 8.0, [8.0, 0.0, 0.0]), (1, 6.0, [0.0, 0.0, 6.0])]);
    assert_eq!(came_at(&t, &r), Some(BotsWhy::LandedNear));
    let t = track(vec![
        (0, 50.0, [50.0, 0.0, 0.0]),
        (1, 30.0, [30.0, 0.0, 0.0]),
    ]);
    assert_eq!(
        came_at(&t, &r),
        Some(BotsWhy::Closing),
        "20 closer in a tick"
    );
    let t = track(vec![
        (0, 30.0, [0.0, 0.0, 30.0]),
        (1, 30.0, [30.0, 0.0, 0.0]),
    ]);
    assert_eq!(came_at(&t, &r), None, "a jump that lands as far off");
}
