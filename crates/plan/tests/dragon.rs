//! Issue 20g (docs/problems/end-dragon.md): the dragon's model and the fight's choices on
//! hand-built End worlds, each test named for its clojurecraft `dragon_test.clj` counterpart where
//! one exists: the phase and perch, the head along the yaw, the stand behind the body, the perch
//! line, the blow and the breath answered, the meal in a lull, the exit portal, the death and the
//! credits answered with the respawn.

use std::sync::Arc;

use data::{BlockKind, EntityKind};
use plan::dragon::{
    self, Ask, Breath, DragonView, Want, ahead, beside, breath_on_way, breaths, decide, facing,
    head_point, head_stand, keep_stand, perch_line, perched, stand_behind,
};
use plan::{Aim, Plan};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use protocol::metadata::MetaValue;
use world::{Act, Event, World};

/// The floor: end stone to y 63, the feet at 64.
const FLOOR: i32 = 64;
const DRAGON: i32 = 41;
/// The perch over the pillar: about 4 over the basin's floor (E1: floor 62, pillar to 66, perch
/// about 67).
const PERCH_Y: f64 = 68.0;

fn state(k: BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap_or(0)
}

fn section(k: BlockKind) -> Section {
    Section {
        block_count: if k == BlockKind::Air { 0 } else { 4096 },
        fluid_count: 0,
        blocks: Paletted::Single(state(k)),
        biomes: Paletted::Single(0),
    }
}

fn column() -> Column {
    let mut sections = vec![section(BlockKind::EndStone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    Column { min_y: 0, sections }
}

/// The End with a flat floor round the fountain, the bot's feet at `feet`, full health.
fn end_at(feet: [f64; 3]) -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 8,
        dimension_type: 2,
        dimension: "minecraft:the_end".to_owned(),
        game_type: 0,
    }));
    let col = Arc::new(column());
    for x in -2..=1 {
        for z in -2..=1 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    w.body = Some(physics::Body {
        pos: feet,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(feet);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

fn add(w: &mut World, id: i32, kind: EntityKind, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
}

fn data(w: &mut World, id: i32, entries: Vec<(u8, MetaValue)>) {
    w.step(&Event::Packet(S2c::SetEntityData { id, entries }));
}

/// The dragon at `at` in `phase`, facing `yaw` (told by a turn).
fn with_dragon(w: &mut World, at: [f64; 3], phase: i32, yaw: f32) {
    add(w, DRAGON, EntityKind::EnderDragon, at);
    data(
        w,
        DRAGON,
        vec![(9, MetaValue::Float(200.0)), (16, MetaValue::Int(phase))],
    );
    w.step(&Event::Packet(S2c::MoveEntityRot { id: DRAGON, yaw }));
}

fn cloud(w: &mut World, id: i32, at: [f64; 3], r: f32) {
    add(w, id, EntityKind::AreaEffectCloud, at);
    data(w, id, vec![(8, MetaValue::Float(r))]);
}

fn ask() -> Ask {
    Ask {
        danger: 8.0,
        blow: dragon::HEAD_BLOW,
        perched_ticks: 20,
        stand: None,
        bunker: None,
        dead: false,
        food: true,
        way: None,
        sat_damage: 0.0,
    }
}

fn sitting(yaw: f32) -> DragonView {
    DragonView {
        pos: [0.5, PERCH_Y, 0.5],
        yaw,
        phase: 6,
        health: 200.0,
    }
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
}

fn feet_of(c: [i32; 3]) -> [f64; 3] {
    dragon::mid(c)
}

#[test]
fn the_dragon_perches_on_the_fountain() {
    let mut v = sitting(0.0);
    v.pos = [2.5, 67.0, -1.5];
    assert!(
        perched(&v, dragon::PERCH_TICKS),
        "by its phase, on the pillar"
    );
    assert!(!perched(&v, 3), "not yet held there 10 ticks");
    v.phase = 3;
    assert!(!perched(&v, 20), "landing over the pillar: not yet sat");
    assert!(dragon::grounded(&v), "but its head is over the basin");
    v.phase = -1;
    assert!(perched(&v, 20), "no phase said: by place");
    v.pos[1] = 75.0;
    assert!(!perched(&v, 20), "over y 70: flying");
    v.pos = [40.5, 66.0, 0.5];
    assert!(!perched(&v, 20), "away from the fountain");
    let mut w = end_at([10.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let (id, seen) = dragon::view(&w).unwrap();
    assert_eq!(id, DRAGON);
    assert_eq!(seen.phase, 6, "the phase from entity data 16");
    assert!((seen.yaw - 90.0).abs() < 1e-6);
}

#[test]
fn the_dragon_turns_by_the_servers_word() {
    let mut w = end_at([10.5, 64.0, 0.5]);
    add(&mut w, DRAGON, EntityKind::EnderDragon, [0.5, 66.0, 0.5]);
    assert!(!dragon::yaw_seen(&w), "no move yet");
    w.step(&Event::Packet(S2c::MoveEntityRot {
        id: DRAGON,
        yaw: 270.0,
    }));
    assert_eq!(w.entities[&DRAGON].yaw, Some(270.0));
    w.step(&Event::Packet(S2c::MoveEntity {
        id: DRAGON,
        dx: 1.0,
        dy: 0.0,
        dz: 0.0,
        yaw: Some(45.0),
    }));
    assert_eq!(w.entities[&DRAGON].yaw, Some(45.0));
    w.step(&Event::Packet(S2c::MoveEntity {
        id: DRAGON,
        dx: 1.0,
        dy: 0.0,
        dz: 0.0,
        yaw: None,
    }));
    assert_eq!(
        w.entities[&DRAGON].yaw,
        Some(45.0),
        "a move with no turn keeps it"
    );
    w.step(&Event::Packet(S2c::TeleportEntity {
        id: DRAGON,
        x: 0.0,
        y: 0.0,
        z: 0.0,
        flags: 0b1111,
        yaw: 10.0,
    }));
    assert_eq!(
        w.entities[&DRAGON].yaw,
        Some(55.0),
        "a teleport's yaw relative by its flag"
    );
}

#[test]
fn the_sitting_dragons_head_is_along_its_yaw() {
    assert!(
        near(facing(180.0), [0.0, 1.0]),
        "the head at (sin yaw, -cos yaw)"
    );
    assert!(near(facing(90.0), [1.0, 0.0]));
    let v = DragonView {
        pos: [0.5, 66.0, 0.5],
        yaw: 90.0,
        phase: 6,
        health: 200.0,
    };
    let (along, across) = ahead(&v, [2.5, 64.0, -0.5]);
    assert!(
        near([along, across], [2.0, -1.0]),
        "along, and to the right"
    );
    assert!(near(beside(&v, 2.0, -1.0), [2.5, -0.5]), "and back");
    let h = head_point(&v);
    assert!(near([h[0], h[2]], [7.0, 0.5]), "6.5 out: {h:?}");
    assert!(
        (h[1] - 65.5).abs() < 1e-9,
        "its box from 1 under the feet: {h:?}"
    );
    assert!(
        dragon::before_head(&v, [2.5, 64.0, 0.5]),
        "two out in front"
    );
    assert!(
        !dragon::before_head(&v, [-1.5, 64.0, 0.5]),
        "two out behind"
    );
    assert!(
        !dragon::before_head(&v, [2.5, 64.0, 4.5]),
        "beside, past the neck"
    );
}

#[test]
fn the_wait_is_behind_the_sitting_dragons_head() {
    let mut w = end_at([-12.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let v = dragon::view(&w).unwrap().1;
    assert_eq!(stand_behind(&v, &w), Some([-2, FLOOR, 0]), "behind it");
    assert_eq!(
        head_stand(&v, &w),
        Some([9, FLOOR, 0]),
        "2.0 beyond the head's box"
    );
    match decide(&w, &ask()) {
        Want::Go(c, "round the head") => {
            assert!(
                !dragon::before_head(&v, feet_of(c)),
                "the way round is out of the head's way: {c:?}"
            );
        }
        other => panic!("round the head to the head stand, not {other:?}"),
    }
    // There: the head part, the dragon's id + 1 (E6's e05: a full 6.0, never hurt).
    let mut w = end_at(feet_of([9, FLOOR, 0]));
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    assert_eq!(
        decide(&w, &ask()),
        Want::Strike {
            entity: DRAGON + 1,
            head: true
        }
    );
    // Landing, not yet sat: out of its reach to the ring, never into the basin (race
    // 45873410249 lane 0: the landing head took 10 from a bot walking to a stand behind it).
    let mut w = end_at([-4.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 3, 90.0);
    match decide(&w, &ask()) {
        Want::Go(c, "out of the landing's reach") => {
            let d = dragon::flat(feet_of(c), [0.5, 0.0, 0.5]);
            assert!(d >= 11.0, "{c:?} {d}");
        }
        other => panic!("out to the ring, not {other:?}"),
    }
    let mut w = end_at([-16.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 3, 90.0);
    assert_eq!(decide(&w, &ask()), Want::Hold("the dragon not yet sat"));
    // With a bunker: the wait is there.
    let a = Ask {
        bunker: Some([14, FLOOR - 3, 0]),
        ..ask()
    };
    assert_eq!(
        decide(&w, &a),
        Want::Go([14, FLOOR - 3, 0], "to the bunker")
    );
    // In front of it under the head: out sideways first, never along under it.
    let mut w = end_at([-1.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 7, 270.0);
    let v = dragon::view(&w).unwrap().1;
    match decide(&w, &ask()) {
        Want::Go(c, "round the head") => {
            let (_, across) = dragon::ahead(&v, feet_of(c));
            assert!(across.abs() >= 4.0, "{c:?}");
        }
        other => panic!("out sideways, not {other:?}"),
    }
}

#[test]
fn the_stand_held_stays_the_stand_while_the_dragon_turns_marginally() {
    // At yaw 160 the stand (-2, 0) is 0.68 behind the body: short of 1, within the slack.
    let mut w = end_at([-1.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 160.0);
    let v = dragon::view(&w).unwrap().1;
    let held = Some([-2, FLOOR, 0]);
    assert_ne!(stand_behind(&v, &w), held, "chosen anew it is not");
    assert_eq!(
        keep_stand(&v, &w, held),
        held,
        "held, it stays: still behind it within the slack"
    );
    let mut w = end_at([-1.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 270.0);
    let v = dragon::view(&w).unwrap().1;
    assert_ne!(
        keep_stand(&v, &w, held),
        held,
        "turned round onto it: a new stand"
    );
}

#[test]
fn the_bot_stands_in_the_dragons_reach_only_above_two_hits_and_the_margin() {
    assert_eq!(
        perch_line(8.0, 6.0),
        14.0,
        "two blows through iron with boots is 20: one blow and the line"
    );
    assert_eq!(perch_line(8.0, 4.0), 16.0, "softer blows: two and the line");
    let mut w = end_at(feet_of([9, FLOOR, 0]));
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    w.player.health = Some(13.0);
    match decide(&w, &ask()) {
        Want::Go(c, "under the perch line") => {
            let d = dragon::flat(feet_of(c), [0.5, 0.0, 0.5]);
            assert!(d >= 11.0, "out of reach: {c:?} {d}");
        }
        other => panic!("under the line no strike: {other:?}"),
    }
    w.player.health = Some(14.5);
    assert_eq!(
        decide(&w, &ask()),
        Want::Strike {
            entity: DRAGON + 1,
            head: true
        },
        "over it, the head from the head stand"
    );
}

#[test]
fn the_first_blow_of_the_dragon_sends_the_bot_out_of_its_reach() {
    let mut w = end_at(feet_of([2, FLOOR, 0]));
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 270.0);
    w.hits.push(world::Hit {
        at_tick: w.session.server_ticks,
        damage_type: "mob_attack".to_owned(),
        cause: Some(EntityKind::EnderDragon),
        direct: Some(EntityKind::EnderDragon),
    });
    match decide(&w, &ask()) {
        Want::Go(c, "out of reach after a blow") => {
            let d = dragon::flat(feet_of(c), [0.5, 0.0, 0.5]);
            assert!(d >= 7.5, "{c:?} {d}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn escape_never_paths_through_the_sitting_dragons_head_or_bite_area() {
    let mut w = end_at([4.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 5, 90.0);
    cloud(&mut w, 50, [5.5, 64.0, 0.5], 3.0);
    let v = dragon::view(&w).unwrap().1;
    match decide(&w, &ask()) {
        Want::Go(c, "out of the breath") => {
            let p = feet_of(c);
            assert!(!dragon::before_head(&v, p), "{c:?}");
            assert!(dragon::way_clear(&v, [4.5, 64.0, 0.5], p), "{c:?}");
            assert!(
                dragon::segment_distance([7.0, 0.0, 0.5], [4.5, 64.0, 0.5], p) >= 2.5 - 1e-9,
                "never through the head: {c:?}"
            );
            assert!(dragon::flat(p, [5.5, 64.0, 0.5]) > 4.5, "{c:?}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_breath_on_the_way_to_the_stand_is_waited_out_where_the_feet_are() {
    let mut w = end_at([-12.5, 64.0, 0.5]);
    cloud(&mut w, 50, [-7.5, 64.0, 0.5], 3.0);
    let b = breaths(&w);
    assert_eq!(
        b,
        vec![Breath {
            id: 50,
            pos: [-7.5, 64.0, 0.5],
            radius: 3.0
        }],
        "a cloud's radius from its data (8)"
    );
    assert!(breath_on_way(
        &b,
        [-12.5, 64.0, 0.5],
        [-1.5, 64.0, 0.5],
        0.0
    ));
    assert!(!breath_on_way(
        &b,
        [-12.5, 64.0, 9.5],
        [-1.5, 64.0, 9.5],
        0.0
    ));
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    assert_eq!(
        decide(&w, &ask()),
        Want::Hold("a breath on the way to the head stand")
    );
}

#[test]
fn in_a_lull_the_bot_eats_for_health_back() {
    let mut w = end_at([14.5, 64.0, 0.5]);
    with_dragon(&mut w, [40.5, 90.0, 0.5], 0, 0.0);
    w.player.health = Some(15.0);
    w.player.food = Some(16);
    assert_eq!(decide(&w, &ask()), Want::Eat);
    w.player.food = Some(20);
    assert_ne!(decide(&w, &ask()), Want::Eat, "a full bar heals by itself");
    w.player.food = Some(16);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 0.0);
    assert_ne!(decide(&w, &ask()), Want::Eat, "never with the dragon sat");
}

#[test]
fn with_the_dragon_dead_the_bot_walks_into_the_exit_portal() {
    let mut w = end_at([12.5, 64.0, 0.5]);
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (2, 64, 0),
        state: state(BlockKind::EndPortal),
    }));
    let a = Ask {
        dead: true,
        ..ask()
    };
    assert_eq!(decide(&w, &a), Want::Exit([2, 64, 0]));
}

#[test]
fn the_dragon_is_dead_by_the_servers_word_and_the_credits_are_answered() {
    let mut w = end_at([12.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 0.0);
    let mut p = Plan::new(Aim::Race);
    assert!(
        matches!(p.dragon_task(&w, 0), Some(plan::Task::Dragon(_))),
        "no crystal shown: the fight"
    );
    p.step(&w);
    assert!(!p.dragon.dead);
    w.step(&Event::Packet(S2c::EntityEvent {
        id: DRAGON,
        status: 2,
    }));
    p.step(&w);
    assert!(!p.dragon.dead, "a hurt is not a death");
    w.step(&Event::Packet(S2c::EntityEvent {
        id: DRAGON,
        status: 3,
    }));
    p.step(&w);
    assert!(p.dragon.dead, "entity event 3");
    assert!(p.events_since(0).iter().any(|e| e.what == "dragon dead"));
    let _ = p.take_acts();
    // The credits: one respawn, and every row held until the overworld.
    w.step(&Event::Packet(S2c::GameEvent {
        event: 4,
        param: 1.0,
    }));
    assert!(w.won);
    p.step(&w);
    assert!(p.events_since(0).iter().any(|e| e.what == "credits"));
    assert_eq!(p.take_acts(), vec![Act::Respawn]);
    p.step(&w);
    assert_eq!(
        p.take_acts(),
        Vec::<Act>::new(),
        "asked once until it is due again"
    );
}

#[test]
fn a_crystal_shown_leaves_the_fight_to_the_crystals() {
    let mut w = end_at([12.5, 64.0, 0.5]);
    add(&mut w, 60, EntityKind::EndCrystal, [42.5, 90.0, 0.5]);
    let mut p = Plan::new(Aim::Race);
    assert!(p.dragon_task(&w, 0).is_none());
}

#[test]
fn a_fireballs_cloud_is_never_taken_under_its_first_radius() {
    // Race 45873410249 lane 0: a cloud shown with radius 0.0 on the feet was stood in for 80
    // ticks, 20 health to none.
    let mut w = end_at([8.5, 64.0, 0.5]);
    cloud(&mut w, 50, [9.5, 64.0, 0.5], 0.0);
    let b = breaths(&w);
    assert!((b[0].radius - 3.0).abs() < 1e-9, "{b:?}");
    w.session.server_ticks += 300;
    let b = breaths(&w);
    assert!((b[0].radius - 5.0).abs() < 1e-9, "grown by its age: {b:?}");
}

#[test]
fn the_dragon_comes_before_a_provoked_endermans_pocket() {
    let mut w = end_at([6.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 0.0);
    let mut p = Plan::new(Aim::Race);
    assert!(p.dragon_first_step(&w, 0), "the dragon sat within 16");
    let mut far = end_at([30.5, 64.0, 0.5]);
    with_dragon(&mut far, [0.5, PERCH_Y, 0.5], 6, 0.0);
    assert!(!p.dragon_first_step(&far, 0), "far off: the pocket");
    cloud(&mut far, 50, [30.5, 64.0, 1.5], 3.0);
    assert!(p.dragon_first_step(&far, 0), "a breath on the feet");
}

#[test]
fn a_way_round_the_head_is_kept_until_it_is_reached() {
    // Race 45873410249 lane 0: the way round and the head stand swapped every tick.
    let mut w = end_at([-12.5, 64.0, -1.0]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let Want::Go(held, "round the head") = decide(&w, &ask()) else {
        panic!("round the head");
    };
    let mut w = end_at([-12.5, 64.0, 2.0]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let Want::Go(fresh, "round the head") = decide(&w, &ask()) else {
        panic!("round the head");
    };
    assert_ne!(
        fresh, held,
        "from the other side, chosen anew, the other way round"
    );
    let a = Ask {
        way: Some(held),
        ..ask()
    };
    assert_eq!(
        decide(&w, &a),
        Want::Go(held, "round the head"),
        "held, it is kept"
    );
    let mut there = end_at(feet_of(held));
    with_dragon(&mut there, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let a = Ask {
        way: Some(held),
        ..ask()
    };
    assert_ne!(
        decide(&there, &a),
        Want::Go(held, "round the head"),
        "reached, the next leg"
    );
}

#[test]
fn a_breath_on_the_feet_is_walked_out_of_ahead_of_every_task() {
    // Race 45873410249 lane 0 and E4's race 45875049490 lane 0: stood in a fireball's cloud, dead.
    let mut w = end_at([8.5, 64.0, 0.5]);
    cloud(&mut w, 50, [9.5, 64.0, 0.5], 3.0);
    let b = breaths(&w);
    assert!(dragon::breath_on(&b, [8.5, 64.0, 0.5]).is_some());
    let out = dragon::out_of_breath(&w, [8.5, 64.0, 0.5]).unwrap();
    assert!(
        dragon::flat(feet_of(out), [9.5, 64.0, 0.5]) > 4.5,
        "out of its reach: {out:?}"
    );
    let mut p = Plan::new(Aim::Race);
    let c = p.breath_step(&w, 0).expect("the walk out");
    assert_eq!(c.pitch, Some(90.0), "looking down");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == format!("out of the breath to {out:?}")),
        "noted"
    );
    // Out of it: the walk ends, noted, and what was running runs on.
    let mut clear = end_at(feet_of(out));
    cloud(&mut clear, 50, [9.5, 64.0, 0.5], 3.0);
    assert!(p.breath_step(&clear, 20).is_none());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "out of the breath")
    );
}

#[test]
fn a_breath_over_the_bunker_or_under_a_pillar_is_not_on_the_body() {
    let b = vec![Breath {
        id: 50,
        pos: [8.5, 64.0, 0.5],
        radius: 3.0,
    }];
    assert!(
        dragon::breath_on(&b, [8.5, 64.0, 0.5]).is_some(),
        "on its floor"
    );
    assert!(
        dragon::breath_on(&b, [8.5, 61.0, 0.5]).is_none(),
        "three down in the bunker"
    );
    assert!(
        dragon::breath_on(&b, [8.5, 66.0, 0.5]).is_none(),
        "two up a pillar"
    );
}

#[test]
fn a_walk_waits_at_a_breaths_edge() {
    let mut w = end_at([2.5, 64.0, 0.5]);
    cloud(&mut w, 50, [8.0, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    // Yaw -90 faces +x (Notchian: south 0, west +90): toward the cloud, its reach 4.5 from 8.0.
    let toward = physics::Controls {
        forward: true,
        yaw: -90.0,
        ..physics::Controls::default()
    };
    assert!(!p.breath_ahead(&w, toward).forward, "waits");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "walk waits at a breath's edge")
    );
    let away = physics::Controls {
        yaw: 90.0,
        ..toward
    };
    assert!(p.breath_ahead(&w, away).forward, "away from it: on");
}

#[test]
fn the_strike_that_makes_it_take_off_is_struck_from_under_the_wings() {
    // Race 45876898172 lane 2: the strike over 50 was struck from the surface; the head and wings
    // took 15 and threw the bot 120 blocks off the island.
    assert!(!dragon::crossing(42.0), "one more strike stays under 50");
    assert!(dragon::crossing(44.5));
    let mut w = end_at(feet_of([9, FLOOR, 0]));
    with_dragon(&mut w, [0.5, 67.0, 0.5], 6, 90.0);
    let v = dragon::view(&w).unwrap().1;
    assert!(!dragon::under_wings(&v, 64.0), "feet 64, the wings from 65");
    assert!(dragon::under_wings(&v, 63.0), "a block down: under them");
    let a = Ask {
        sat_damage: 45.0,
        ..ask()
    };
    assert_eq!(
        decide(&w, &a),
        Want::Dig([9, FLOOR - 1, 0]),
        "one dig of end stone down first"
    );
    assert_eq!(
        decide(&w, &ask()),
        Want::Dig([9, FLOOR - 1, 0]),
        "every strike from under the sweep where one dig makes it so"
    );
    let mut low = end_at([9.5, 63.0, 0.5]);
    low.step(&Event::Packet(S2c::BlockUpdate {
        pos: (9, 63, 0),
        state: state(BlockKind::Air),
    }));
    with_dragon(&mut low, [0.5, 67.0, 0.5], 6, 90.0);
    assert!(
        matches!(decide(&low, &a), Want::Strike { head: true, .. }),
        "down in it: the strike"
    );
}
