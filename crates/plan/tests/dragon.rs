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
        phase_ticks: 0,
        way_done: None,
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
    assert_eq!(
        decide(&w, &ask()),
        Want::Go([-9, FLOOR, 0], "to the head stand"),
        "scanning, it turns its head onto the bot: the stand on the bot's side"
    );
    // Attacking, its yaw held: round its head to the stand it points at.
    let mut w = end_at([-12.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 7, 90.0);
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
            radius: 3.0,
            grows: true,
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
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 7, 90.0);
    let Want::Go(held, "round the head") = decide(&w, &ask()) else {
        panic!("round the head");
    };
    let mut w = end_at([-12.5, 64.0, 2.0]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 7, 90.0);
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
    with_dragon(&mut there, [0.5, PERCH_Y, 0.5], 7, 90.0);
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
        grows: true,
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
fn a_walk_is_held_clear_of_a_clouds_end_reach_and_backs_off_inside_it() {
    // Wall 47, frontier race 46063334187 lanes 1, 2 and 8: walks waited at a breath's edge that
    // grew over them (5.0 to 7.0).
    let toward = physics::Controls {
        forward: true,
        // Yaw -90 faces +x (Notchian: south 0, west +90): toward the cloud.
        yaw: -90.0,
        ..physics::Controls::default()
    };
    // Outside the end reach (7 + 1.5 + 1 from 8.0) with the way ahead in it: held there.
    let mut far = end_at([-1.8, 64.0, 0.5]);
    cloud(&mut far, 50, [8.0, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    assert!(!p.breath_ahead(&far, toward).forward, "held");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "walk held clear of a breath's end reach")
    );
    // Inside its end reach (not yet its edge): backed off, away from it (-x, yaw 90).
    let mut near = end_at([2.5, 64.0, 0.5]);
    cloud(&mut near, 50, [8.0, 64.0, 0.5], 3.0);
    let mut q = Plan::new(Aim::Race);
    let c = q.breath_ahead(&near, toward);
    assert!(c.forward && (c.yaw - 90.0).abs() < 1.0, "{c:?}");
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what == "walk backs off a breath's growth")
    );
    // Walking away from it, clear: on.
    let away = physics::Controls {
        yaw: 90.0,
        ..toward
    };
    assert!(p.breath_ahead(&far, away).forward, "away from it: on");
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

#[test]
fn the_flame_is_waited_out_of_its_reach() {
    // Races 45873410249 and 45876898172: 3.0 a flame, walking out of its cloud from the head stand.
    let mut w = end_at(feet_of([9, FLOOR, 0]));
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 7, 90.0);
    assert!(
        matches!(decide(&w, &ask()), Want::Strike { head: true, .. }),
        "the attacking turn's first ticks: the strike"
    );
    let late = Ask {
        phase_ticks: 30,
        ..ask()
    };
    let v = dragon::view(&w).unwrap().1;
    let h = head_point(&v);
    match decide(&w, &late) {
        Want::Go(c, "out of the flame's way") => {
            let d = dragon::flat(feet_of(c), h);
            assert!(d >= dragon::FLAME_CLEAR, "{c:?} {d}");
            assert!(
                dragon::flat(feet_of(c), v.pos) >= dragon::BITE_OUT + 1.0,
                "out of the turning head's sweep: {c:?}"
            );
            assert!(!dragon::before_head(&v, feet_of(c)), "{c:?}");
        }
        other => panic!("out of the flame's way before it comes, not {other:?}"),
    }
    let mut flaming = end_at(feet_of([9, FLOOR, 0]));
    with_dragon(&mut flaming, [0.5, PERCH_Y, 0.5], 5, 90.0);
    assert!(
        matches!(
            decide(&flaming, &ask()),
            Want::Go(_, "out of the flame's way")
        ),
        "and while it burns"
    );
}

#[test]
fn a_cell_out_of_the_breath_is_never_the_feets_own_and_well_clear() {
    // Race 45882663266: lane 0 held 370 ticks at 6.4 off a reach of 6.5, the walk to its own cell
    // done each tick; lane 2 walked 211 times to the cell under it on the bunker's stair.
    let mut w = end_at([13.9, 64.0, 0.5]);
    cloud(&mut w, 50, [9.5, 64.0, 0.5], 3.0);
    let b = breaths(&w);
    assert!(
        dragon::breath_on(&b, [13.9, 64.0, 0.5]).is_some(),
        "4.4 off a reach of 4.5"
    );
    let c = dragon::out_of_breath(&w, [13.9, 64.0, 0.5]).unwrap();
    assert_ne!([c[0], c[2]], [13, 0], "not its own column");
    assert!(
        dragon::flat(feet_of(c), [9.5, 64.0, 0.5]) > 5.5,
        "a block past the reach: {c:?}"
    );
}

#[test]
fn a_charge_is_waited_out_crouched_or_in_the_bunker() {
    // E4's race 4: a charge hit a bot digging down two over the ground for 10 and 5 and threw it
    // 48 high.
    let feet = [12.5, 64.0, 0.5];
    // Yaw 90 faces +x: from x -17.5 the dragon's facing is on the line to the bot, 30 off.
    let mut w = end_at(feet);
    with_dragon(&mut w, [-17.5, 64.0, 0.5], 8, 90.0);
    assert!(dragon::charging_at(&w, feet).is_some());
    // Dodged sideways like a fireball, never a notch (wall 36).
    let mut p = Plan::new(Aim::Race);
    assert!(p.charge_step(&w, 0).is_some());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("charge: dodge to")),
        "{:?}",
        p.events_since(0)
    );
    assert!(p.take_acts().iter().all(|a| !matches!(a, Act::Dig { .. })));
    let mut away = end_at(feet);
    with_dragon(&mut away, [-17.5, 64.0, 0.5], 8, 270.0);
    assert!(dragon::charging_at(&away, feet).is_none(), "facing away");
    let mut far = end_at(feet);
    with_dragon(&mut far, [-40.5, 64.0, 0.5], 8, 90.0);
    assert!(dragon::charging_at(&far, feet).is_none(), "53 off");
    let mut holding = end_at(feet);
    with_dragon(&mut holding, [-17.5, 64.0, 0.5], 0, 90.0);
    assert!(
        dragon::charging_at(&holding, feet).is_none(),
        "not charging"
    );
}

#[test]
fn a_scanning_turn_that_does_not_come_is_not_waited_for() {
    // E6's end_loop: a dragon that never turned held the bot 15,000 ticks behind it.
    let mut w = end_at([-12.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    assert_eq!(
        decide(&w, &ask()),
        Want::Go([-9, FLOOR, 0], "to the head stand"),
        "the turn's first ticks: where the head will come"
    );
    let long = Ask {
        phase_ticks: 60,
        ..ask()
    };
    assert!(
        matches!(decide(&w, &long), Want::Go(_, "round the head")),
        "no turn in 40: round to the head as it points"
    );
}

#[test]
fn the_takeoff_count_starts_again_when_a_blow_sends_it_off() {
    // Race 45895541443 lane 0: the blow over 50 was seen with the phase already takeoff, the count
    // never reset, and no strike came after 100 dealt.
    let mut w = end_at([30.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    let mut p = Plan::new(Aim::Race);
    p.step(&w);
    data(&mut w, DRAGON, vec![(9, MetaValue::Float(156.0))]);
    p.step(&w);
    assert!(
        (p.dragon.sat_damage - 44.0).abs() < 1e-3,
        "{}",
        p.dragon.sat_damage
    );
    data(
        &mut w,
        DRAGON,
        vec![(9, MetaValue::Float(150.0)), (16, MetaValue::Int(4))],
    );
    p.step(&w);
    assert_eq!(
        p.dragon.sat_damage, 0.0,
        "the server's count went over: it starts again"
    );
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("dragon takeoff count over"))
    );
}

#[test]
fn a_charge_is_dodged_sideways_never_met_in_a_notch() {
    // Race 45895541443 lane 2: crouched on the surface, the charge took 10 and two fly-bys 5.
    // Dev race 45964846590: three bots died in a notch under a cloud (wall 36).
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    with_dragon(&mut w, [-17.5, 64.0, 0.5], 8, 90.0);
    assert!(dragon::notch_diggable(&w, [12, 64, 0]));
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "charge: dodge to [12, 64, 12]"),
        "{:?}",
        p.events_since(0)
    );
    assert_eq!(p.dragon.dodge_id, Some(dragon::CHARGE));
    assert!(
        p.take_acts().iter().all(|a| !matches!(a, Act::Dig { .. })),
        "no dig"
    );
    // Two down, the step out is beside it one under the surface, with air over it.
    let mut down = end_at([12.5, 62.0, 0.5]);
    for y in [62, 63] {
        down.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    assert_eq!(dragon::notch_step(&down, [12, 64, 0]), Some([13, 63, 0]));
    assert!(
        !dragon::notch_diggable(&end_at([12.5, 1.0, 0.5]), [12, 1, 0]),
        "never a notch into the void"
    );
}

#[test]
fn scanning_the_bot_never_goes_far_enough_to_draw_a_charge() {
    // Race 45895541443 lane 2: out of reach 19 off during a scanning turn drew a charge.
    let mut w = end_at([17.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
    w.player.health = Some(12.0);
    match decide(&w, &ask()) {
        Want::Go(c, "under the perch line") => {
            let d = dragon::flat(feet_of(c), [0.5, 0.0, 0.5]);
            assert!(d <= dragon::SCAN_KEEP + 1.0, "{c:?} {d}");
        }
        other => panic!("in to the scan's reach, not {other:?}"),
    }
    // Not scanning: the ring out at 16.
    let mut flying = end_at([17.5, 64.0, 0.5]);
    with_dragon(&mut flying, [0.5, PERCH_Y, 0.5], 5, 90.0);
    flying.player.health = Some(12.0);
    assert_eq!(decide(&flying, &ask()), Want::Hold("under the perch line"));
}

#[test]
fn the_notch_is_roofed_a_side_block_first() {
    // A fireball's cloud on an open notch is on the body.
    let mut down = end_at([12.5, 62.0, 0.5]);
    for y in [62, 63] {
        down.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    // The enderman's notch (the one notch left, wall 36), an angry one 5 off.
    add(&mut down, 70, EntityKind::Enderman, [17.5, 64.0, 0.5]);
    data(&mut down, 70, vec![(17, MetaValue::Bool(true))]);
    down.inventory
        .slots
        .insert(36, (data::ItemKind::EndStone, 64));
    down.held = 0;
    let mut p = Plan::new(Aim::Race);
    p.dragon.notch_from = Some([12, 64, 0]);
    p.dragon.ender_notch = true;
    assert!(p.end_enderman_step(&down, 0).is_some());
    let acts = p.take_acts();
    assert!(
        acts.contains(&Act::UseOn {
            pos: world::BlockPos::new(13, 63, 0),
            face: 1
        }),
        "the side block on the wall's top first, the roof's cell having no face: {acts:?}"
    );
    assert_eq!(p.dragon.notch_side, Some([13, 64, 0]));
    // The side laid: the roof against it.
    down.step(&Event::Packet(S2c::BlockUpdate {
        pos: (13, 64, 0),
        state: state(BlockKind::EndStone),
    }));
    assert!(p.end_enderman_step(&down, 10).is_some());
    assert!(p.take_acts().contains(&Act::UseOn {
        pos: world::BlockPos::new(13, 64, 0),
        face: 4
    }));
    // The step out is never under the side block.
    assert_ne!(
        dragon::notch_step_except(&down, [12, 64, 0], Some([13, 64, 0])),
        Some([13, 63, 0])
    );
}

#[test]
fn a_stand_under_the_heads_blow_strikes_from_under_it() {
    // Race 45901376114 lane 0: feet at 62 under a dragon at 67; 2.0 off the box was out of reach.
    let v = DragonView {
        pos: [0.5, 67.0, 0.5],
        yaw: 90.0,
        phase: 6,
        health: 200.0,
    };
    assert!(
        dragon::clear_of_blow(&v, [7.5, 62.0, 0.5]),
        "under the blow box"
    );
    assert!(
        !dragon::clear_of_blow(&v, [7.5, 64.0, 0.5]),
        "in it on the surface"
    );
    let eye = [7.5, 62.0 + physics::EYE_HEIGHT, 0.5];
    assert!(dragon::head_reach(&v, eye) <= dragon::STRIKE_REACH);
}

#[test]
fn sealed_in_cover_the_wait_holds_there() {
    // Race 45901376114 lane 0: sealed in a pocket, the walk to the bunker failed 656 times.
    let mut w = end_at([12.5, 64.0, 0.5]);
    for c in [
        (12, 66, 0),
        (13, 64, 0),
        (11, 64, 0),
        (12, 64, 1),
        (12, 64, -1),
    ] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::EndStone),
        }));
    }
    with_dragon(&mut w, [40.5, 90.0, 0.5], 0, 0.0);
    let a = Ask {
        bunker: Some([14, FLOOR - 3, 0]),
        ..ask()
    };
    assert_eq!(decide(&w, &a), Want::Hold("in cover"));
}

#[test]
fn walled_in_a_breath_the_wall_toward_the_cell_out_is_dug() {
    // Race 45901376114 lane 2: sealed in a pocket, 140 walks out of a flame failed NoPath, dead.
    let mut w = end_at([12.5, 64.0, 0.5]);
    for c in [
        (13, 64, 0),
        (13, 65, 0),
        (11, 64, 0),
        (11, 65, 0),
        (12, 64, 1),
        (12, 65, 1),
        (12, 64, -1),
        (12, 65, -1),
        (12, 66, 0),
    ] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::EndStone),
        }));
    }
    assert_eq!(
        dragon::wall_toward(&w, [12.5, 64.0, 0.5], [20, 64, 0]),
        Some([13, 64, 0]),
        "the wall at the feet toward the cell"
    );
    cloud(&mut w, 50, [12.5, 64.0, 0.5], 5.0);
    let mut p = Plan::new(Aim::Race);
    let mut dug = false;
    for t in 0..200 {
        let _ = p.breath_step(&w, t);
        dug |= p.take_acts().iter().any(|a| matches!(a, Act::Dig { .. }));
    }
    assert!(dug, "{:?}", p.events_since(0));
}

#[test]
fn in_the_end_an_angry_enderman_never_gets_a_pocket() {
    // Pockets held bots in the dragon's reach, shut the bunker's stair, and a breath came through
    // their walls (races 45895541443, 45901376114).
    let feet = [12.5, 64.0, 0.5];
    let angry = |w: &mut World| {
        add(w, 70, EntityKind::Enderman, [14.0, 64.0, 0.5]);
        data(w, 70, vec![(17, MetaValue::Bool(true))]);
    };
    let mut w = end_at(feet);
    angry(&mut w);
    let mut p = Plan::new(Aim::Race);
    // No bunker near, over the danger line, in reach: struck with the sword, never a notch (wall
    // 42: dev race 46048455555 lane 0 was killed in one).
    assert!(p.end_enderman_step(&w, 0).is_some());
    let acts = p.take_acts();
    assert!(
        acts.iter().any(|a| matches!(a, Act::Attack { entity: 70 })),
        "{acts:?}"
    );
    assert!(acts.iter().all(|a| !matches!(a, Act::Dig { .. })));
    assert!(
        p.events_since(0).iter().any(|e| e.what
            == "end: no pocket for enderman 70 (struck with the sword over the danger line)")
    );
    // Under the danger line: walked away to open ground, never a fight, never a notch.
    let mut low = end_at(feet);
    angry(&mut low);
    low.player.health = Some(6.0);
    let mut u = Plan::new(Aim::Race);
    assert!(u.end_enderman_step(&low, 0).is_some());
    assert!(u.dragon.ender_walk.is_some(), "a walk away begun");
    assert!(
        u.take_acts()
            .iter()
            .all(|a| !matches!(a, Act::Attack { .. } | Act::Dig { .. }))
    );
    assert!(
        u.events_since(0).iter().any(|e| e.what
            == "end: no pocket for enderman 70 (under the danger line, walked away to open ground)"),
        "{:?}",
        u.events_since(0)
    );
    // The bunker within reach: into it.
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([20, FLOOR, 0]);
    assert!(q.end_enderman_step(&w, 0).is_some());
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what == "end: no pocket for enderman 70 (to the bunker)")
    );
    // In a roofed 2-high cell: the work goes on.
    let mut c = end_at(feet);
    for b in [
        (12, 66, 0),
        (13, 64, 0),
        (11, 64, 0),
        (12, 64, 1),
        (12, 64, -1),
    ] {
        c.step(&Event::Packet(S2c::BlockUpdate {
            pos: b,
            state: state(BlockKind::EndStone),
        }));
    }
    angry(&mut c);
    let mut r = Plan::new(Aim::Race);
    assert!(r.end_enderman_step(&c, 0).is_some(), "in cover, in reach");
    assert!(
        r.take_acts()
            .iter()
            .any(|a| matches!(a, Act::Attack { entity: 70 })),
        "struck from the covered cell over the danger line"
    );
    // And Plan::step never seals one in the End.
    let mut s = Plan::new(Aim::Race);
    s.step(&w);
    assert!(
        !s.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("pocket block") || e.what.contains("into a sealed pocket")),
        "{:?}",
        s.events_since(0)
    );
}

#[test]
fn a_breath_by_the_bunker_is_answered_down_into_it_never_out_sideways() {
    // E4's race 45912100302 lane 0: nine walks out of a fireball's cloud by the bunker failed
    // NoPath, then dead.
    let mut w = end_at([12.5, 64.0, 0.5]);
    cloud(&mut w, 50, [12.5, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    // A bunker 4 down: clear of the cloud on the surface.
    p.bunker = Some([15, FLOOR - 4, 0]);
    p.end.door = Some((1, 0));
    assert!(p.breath_step(&w, 0).is_some());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == format!("breath: into the bunker at {:?}", [15, FLOOR - 4, 0])),
        "{:?}",
        p.events_since(0)
    );
    // In the bunker with the breath on the body: a notch down, under the cloud's box (wall 38).
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([12, FLOOR, 0]);
    assert!(q.breath_step(&w, 0).is_some());
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("breath: in the bunker at")),
        "{:?}",
        q.events_since(0)
    );
    assert!(q.take_acts().iter().any(|a| matches!(a, Act::Dig { .. })));
    assert!(q.dragon.breath_notch);
    // A crystal alive: never into the bunker (frontier race 45978279345 lane 7).
    let mut c = end_at([12.5, 64.0, 0.5]);
    cloud(&mut c, 50, [12.5, 64.0, 0.5], 3.0);
    add(&mut c, 80, EntityKind::EndCrystal, [40.5, 90.0, 0.5]);
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([15, FLOOR - 4, 0]);
    r.end.door = Some((1, 0));
    let _ = r.breath_step(&c, 0);
    assert!(
        !r.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("breath: into the bunker")),
        "{:?}",
        r.events_since(0)
    );
}

#[test]
fn a_notch_with_no_step_out_is_risen_out_of_a_block_at_a_jump() {
    // Race 45918669138 lane 1: 2 under the bunker's floor, no step to dig, "no walk to the
    // bunker" for 3,000 ticks.
    let mut w = end_at([12.5, 62.0, 0.5]);
    for y in [62, 63] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    for c in [(13, 64, 0), (11, 64, 0), (12, 64, 1), (12, 64, -1)] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::EndStone),
        }));
    }
    assert_eq!(dragon::notch_step(&w, [12, 64, 0]), None, "walled round");
    w.inventory.slots.insert(36, (data::ItemKind::EndStone, 64));
    w.held = 0;
    let mut p = Plan::new(Aim::Race);
    p.dragon.breath_notch = true;
    p.dragon.notch_from = Some([12, 64, 0]);
    let c = p.breath_step(&w, 0).expect("rising");
    assert!(c.jump, "a jump from the notch's floor");
    assert_eq!(p.dragon.notch_rise_to, Some(64));
    // In the air over the cell it left: the block under it.
    w.body = Some(physics::Body {
        pos: [12.5, 63.2, 0.5],
        on_ground: false,
        ..physics::Body::default()
    });
    let _ = p.breath_step(&w, 10);
    assert!(p.take_acts().contains(&Act::UseOn {
        pos: world::BlockPos::new(12, 61, 0),
        face: 1
    }));
}

#[test]
fn in_the_bunker_under_a_breath_the_bot_stays_there() {
    // Race 45918669138 lane 2: in and out of the bunker 40 times at the stair's top, 20 to dead.
    let mut w = end_at([12.5, 60.0, 0.5]);
    cloud(&mut w, 50, [12.5, 64.0, 0.5], 3.0);
    assert!(
        dragon::breath_on(&breaths(&w), [12.5, 60.0, 0.5]).is_none(),
        "under it"
    );
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([12, 60, 0]);
    assert!(
        p.breath_step(&w, 0).is_none(),
        "nothing in hand: the End's own rows keep it shut"
    );
    p.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    let c = p
        .breath_step(&w, 0)
        .expect("a task in hand: held in the bunker");
    assert!(c.sneak && !c.forward);
}

#[test]
fn out_of_cover_at_a_perch_by_a_shaft_up_never_under_a_breath() {
    // Race 45918669138 lane 1: held covered the whole race, 0 strikes.
    let roofed = || {
        let mut w = end_at([12.5, 64.0, 0.5]);
        for c in [
            (12, 66, 0),
            (13, 64, 0),
            (11, 64, 0),
            (12, 64, 1),
            (12, 64, -1),
        ] {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: c,
                state: state(BlockKind::EndStone),
            }));
        }
        with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 90.0);
        w
    };
    let w = roofed();
    let mut p = Plan::new(Aim::Race);
    p.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    p.dragon.climb_out = true;
    p.step(&w);
    assert!(
        p.take_acts()
            .iter()
            .any(|a| matches!(a, Act::Dig { pos, .. } if *pos == world::BlockPos::new(12, 66, 0))),
        "the roof over the head dug first"
    );
    // A breath over the shaft's top: no dig, held in cover.
    let mut b = roofed();
    cloud(&mut b, 50, [12.5, 67.0, 0.5], 3.0);
    let mut q = Plan::new(Aim::Race);
    q.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    q.dragon.climb_out = true;
    let c = q.step(&b);
    assert!(c.sneak && !c.forward, "held in cover");
    assert!(
        !q.take_acts().iter().any(|a| matches!(a, Act::Dig { .. })),
        "no dig under a breath"
    );
}

#[test]
fn near_the_head_stand_but_out_of_reach_the_bot_steps_onto_its_middle() {
    // Race 45924968115 lane 1: 0.7 off the stand's middle, 3.2 from the head's box, "no head stand
    // open" every turn.
    let mut w = end_at([10.3, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, 68.5, 0.5], 7, 90.0);
    let a = Ask {
        stand: Some([9, FLOOR, 0]),
        ..ask()
    };
    assert_eq!(decide(&w, &a), Want::Centre([9, FLOOR, 0]));
    let mut there = end_at([9.5, 64.0, 0.5]);
    with_dragon(&mut there, [0.5, 68.5, 0.5], 7, 90.0);
    assert!(matches!(
        decide(&there, &a),
        Want::Strike { head: true, .. }
    ));
}

#[test]
fn hidden_through_a_strafe_the_bot_comes_out_to_be_seen_after_the_cap() {
    // Dev race 45937087645: the strafe never ends while the bot is out of the dragon's sight.
    let hidden = |extra: &dyn Fn(&mut World)| {
        let mut w = end_at([12.5, 62.0, 0.5]);
        for y in [62, 63] {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: (12, y, 0),
                state: state(BlockKind::Air),
            }));
        }
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, 64, 0),
            state: state(BlockKind::EndStone),
        }));
        extra(&mut w);
        let mut p = Plan::new(Aim::Race);
        p.bunker = Some([60, FLOOR - 4, 60]);
        p.dragon.notch_from = Some([12, 64, 0]);
        p.dragon.notch_roof = Some([12, 64, 0]);
        p.dragon.charging = true;
        (w, p)
    };
    let out = |p: &Plan| {
        p.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("strafe: out of the notch to be seen"))
    };
    let (w, mut p) = hidden(&|w| with_dragon(w, [-30.5, 80.0, 0.5], 1, 90.0));
    let c = p.charge_step(&w, 100).expect("hidden");
    assert!(c.sneak && !c.forward);
    let _ = p.charge_step(&w, 100 + dragon::STRAFE_HIDE_TICKS - 1);
    assert!(!out(&p), "within the cap");
    assert!(p.take_acts().is_empty());
    let _ = p.charge_step(&w, 100 + dragon::STRAFE_HIDE_TICKS);
    assert!(out(&p), "{:?}", p.events_since(0));
    assert_eq!(
        p.dragon.notch_rise_to,
        Some(64),
        "risen out a block at a jump"
    );
    assert_eq!(p.dragon.notch_from, None);
    assert!(!p.dragon.charging);
    assert!(
        p.take_acts().iter().any(|a| matches!(a, Act::Dig { .. })),
        "the roof dug back first"
    );
    // A cloud within reach: never out, past the cap too.
    let off = 3.0 + dragon::BREATH_MARGIN + 0.5;
    let (w, mut p) = hidden(&|w| {
        with_dragon(w, [-30.5, 80.0, 0.5], 1, 90.0);
        cloud(w, 50, [12.5 + off, 64.0, 0.5], 3.0);
    });
    let _ = p.charge_step(&w, 0);
    let c = p
        .charge_step(&w, dragon::STRAFE_HIDE_TICKS + 50)
        .expect("hidden");
    assert!(c.sneak && !c.forward);
    assert!(!out(&p));
    // A charge: never out either.
    let (w, mut p) = hidden(&|w| with_dragon(w, [-17.5, 64.0, 0.5], 8, 90.0));
    let _ = p.charge_step(&w, 0);
    let _ = p.charge_step(&w, dragon::STRAFE_HIDE_TICKS + 50);
    assert!(!out(&p));
    assert_eq!(p.dragon.notch_rise_to, None);
}

#[test]
fn the_low_stand_for_the_takeoff_strike_is_dug_through_the_flame() {
    // Race 45931228063 lane 2: the takeoff count at 44, "no head stand open" every turn, the
    // dragon at 156 for 9,000 ticks.
    let mut w = end_at([-8.5, 64.0, 0.5]);
    with_dragon(&mut w, [0.5, 67.0, 0.5], 5, 90.0);
    let a = Ask {
        sat_damage: 45.0,
        ..ask()
    };
    let v = dragon::view(&w).unwrap().1;
    let c = dragon::low_prep_cell(&w, &v, [-8.5, 64.0, 0.5]).expect("a cell behind it");
    assert!(
        dragon::ahead(&v, feet_of(c)).0 <= 0.0,
        "away from the head: {c:?}"
    );
    assert_eq!(decide(&w, &a), Want::Dig([-9, FLOOR - 1, 0]));
    // Not the crossing strike: out of the flame's way as before.
    assert!(matches!(
        decide(&w, &ask()),
        Want::Go(_, "out of the flame's way") | Want::Hold("out of the flame's way")
    ));
}

#[test]
fn idle_in_the_end_with_no_note_for_600_ticks_is_noted_and_handed_back() {
    // Dev race 45940964839 lane 1: idle in the bunker 13,000 ticks with no note at all.
    let w = end_at([12.5, 64.0, 0.5]);
    let mut p = Plan::new(Aim::Race);
    p.dragon.charging = true;
    p.dragon.notch_roof = Some([12, 66, 0]);
    let idle = |p: &Plan| {
        p.events_since(0)
            .iter()
            .filter(|e| e.what.starts_with("end: idle"))
            .count()
    };
    p.end_idle_watch(&w, 100);
    p.end_idle_watch(&w, 100 + dragon::IDLE_NOTE_TICKS - 1);
    assert_eq!(idle(&p), 0);
    p.end_idle_watch(&w, 100 + dragon::IDLE_NOTE_TICKS);
    assert_eq!(idle(&p), 1, "{:?}", p.events_since(0));
    let e = p.events_since(0).last().unwrap();
    assert_eq!(e.what, "end: idle 600 ticks at [12, 64, 0]");
    assert!(e.why.starts_with("task idle, rows held none"), "{}", e.why);
    assert!(
        !p.dragon.charging && p.dragon.notch_roof.is_none(),
        "E5's holds dropped"
    );
    // Its own note starts the count again: not again until another 600.
    p.end_idle_watch(&w, 100 + 2 * dragon::IDLE_NOTE_TICKS - 1);
    assert_eq!(idle(&p), 1);
    p.end_idle_watch(&w, 100 + 2 * dragon::IDLE_NOTE_TICKS);
    assert_eq!(idle(&p), 2);
}

#[test]
fn a_strafe_is_met_standing_in_sight() {
    // Dev race 45937087645: hidden in a notch, the strafe held 11,000 ticks (its fireball needs
    // the bot in sight, DragonStrafePlayerPhase). Dev race 45940964839: cover within 4 of the
    // fireball's hit is none (wall 28), so no notch is dug for it.
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    // Facing across the bot (yaw 0, toward -z): its fire is not due.
    with_dragon(&mut w, [-30.5, 80.0, 0.5], 1, 0.0);
    assert!(dragon::strafing_at(&w, feet).is_some());
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert!(p.take_acts().is_empty(), "nothing dug");
    // Paced in sight, sideways to the dragon's bearing (frontier race 45947478869: stood, the
    // fireball's cloud was put on the bot).
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "strafe: paced in sight from [12, 64, 0] to [12, 64, 6]"),
        "{:?}",
        p.events_since(0)
    );
    assert!(p.dragon.pace_walk.is_some());
    assert!(!p.dragon.charging);
    // The strafe over with a cloud within reach of a bot in its notch: stayed in.
    let mut after = end_at([12.5, 62.0, 0.5]);
    for y in [62, 63] {
        after.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    with_dragon(&mut after, [-30.5, 80.0, 0.5], 0, 90.0);
    cloud(&mut after, 50, [12.5, 64.0, 0.5], 3.0);
    p.dragon.charging = true;
    p.dragon.notch_from = Some([12, 64, 0]);
    let c = p.charge_step(&after, 60).expect("stays sheltered");
    assert!(c.sneak && !c.forward);
    // A charge: dodged sideways, never a notch (wall 36).
    let mut charge = end_at(feet);
    with_dragon(&mut charge, [-17.5, 64.0, 0.5], 8, 90.0);
    let mut q = Plan::new(Aim::Race);
    assert!(q.charge_step(&charge, 0).is_some());
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("charge: dodge to"))
    );
    // Far off: no strafe counted.
    let mut far = end_at(feet);
    with_dragon(&mut far, [-80.5, 80.0, 0.5], 1, 90.0);
    assert!(dragon::strafing_at(&far, feet).is_none());
}

/// A strafing dragon at `dragon`, its fireball at `at` flying along `v`, the bot at `feet`.
fn fireball_world(feet: [f64; 3], dragon_at: [f64; 3], at: [f64; 3], v: [f64; 3]) -> World {
    let mut w = end_at(feet);
    with_dragon(&mut w, dragon_at, 1, 90.0);
    add(&mut w, 51, EntityKind::DragonFireball, at);
    if let Some(e) = w.entities.get_mut(&51) {
        e.velocity = Some(v);
    }
    w
}

fn dodged_to(p: &Plan) -> Option<String> {
    p.events_since(0)
        .iter()
        .find(|e| e.what.starts_with("fireball: dodge to"))
        .map(|e| e.what.clone())
}

#[test]
fn a_fireball_aimed_near_is_dodged_sideways_six_off_never_into_cover() {
    // Wall 28 (DragonFireball.onHit): the cloud is put on a body within 4 of the hit, walls or
    // not; dev race 45940964839: all three bots died under it in the bunker or a notch.
    let feet = [12.5, 64.0, 0.5];
    let w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([14, FLOOR - 4, 0]);
    p.end.door = Some((1, 0));
    let _ = p.charge_step(&w, 0).expect("dodging");
    assert!(
        p.dragon
            .dodge_walk
            .as_ref()
            .is_some_and(|wk| wk.target == (12, 64, 6)),
        "the walk there begun"
    );
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, 6]")
    );
    let to = p.dragon.dodge_to.expect("a cell");
    let off = f64::from((to[0] - 12).pow(2) + (to[2]).pow(2)).sqrt();
    assert!(off >= f64::from(dragon::DODGE_MIN), "{off}");
    // Never cover for it: no notch, no shelter, no bunker, no dig.
    assert!(p.take_acts().iter().all(|a| !matches!(a, Act::Dig { .. })));
    assert!(p.dragon.notch_from.is_none() && !p.dragon.charging);
    assert!(!p.events_since(0).iter().any(|e| {
        e.what.starts_with("strafe: shelter")
            || e.what.starts_with("charge: dig")
            || e.what.starts_with("breath:")
    }));
    // The fireball on the feet: the breath answer leaves it to the dodge (no bunker, no notch).
    let mut on = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [12.5, 65.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    on.step(&Event::Packet(S2c::BlockUpdate {
        pos: (14, 62, 0),
        state: state(BlockKind::Air),
    }));
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([14, FLOOR - 4, 0]);
    r.end.door = Some((1, 0));
    assert_eq!(r.breath_step(&on, 0), None);
    // No velocity told: taken as flying straight at the bot.
    let mut nv = fireball_world(feet, [-30.5, 80.0, 0.5], [-10.5, 74.0, 0.5], [0.0; 3]);
    if let Some(e) = nv.entities.get_mut(&51) {
        e.velocity = None;
    }
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    assert!(q.charge_step(&nv, 0).is_some());
    assert!(dodged_to(&q).is_some());
    // Aimed well wide (20 off its line) after the strafe: no dodge, on as it was.
    let mut wide = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 20.5],
        [1.0, 0.0, 0.0],
    );
    with_dragon(&mut wide, [-30.5, 80.0, 0.5], 0, 90.0);
    let mut s = Plan::new(Aim::Race);
    s.bunker = Some([60, FLOOR - 4, 60]);
    assert_eq!(s.charge_step(&wide, 0), None);
    assert!(dodged_to(&s).is_none());
}

#[test]
fn the_dodge_goes_away_from_the_dragon_off_the_edge_and_out_of_the_bunker() {
    let feet = [12.5, 64.0, 0.5];
    // The dragon on the +z side of the fireball's line: the dodge to -z.
    let w = fireball_world(
        feet,
        [-30.5, 80.0, 20.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, -6]")
    );
    // By the edge of what is known (z 32 and on not loaded): never within 3 of it, the far side.
    let near_edge = [12.5, 64.0, 26.5];
    let w = fireball_world(
        near_edge,
        [-30.5, 80.0, 26.5],
        [-10.5, 74.0, 26.5],
        [1.0, -0.3, 0.0],
    );
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    assert!(q.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&q).as_deref(),
        Some("fireball: dodge to [12, 64, 20]")
    );
    // Never into the bunker's column: one block further.
    let w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([12, FLOOR - 4, 6]);
    r.end.door = Some((1, 0));
    assert!(r.bunker_columns().contains(&(12, 6)));
    assert!(r.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&r).as_deref(),
        Some("fireball: dodge to [12, 64, 7]")
    );
}

#[test]
fn an_angry_endermans_hold_is_near_only_and_capped() {
    // E4's race 45942992471 lane 0: 12,000 ticks covered from an enderman whose anger never ends.
    let feet = [12.5, 64.0, 0.5];
    let covered_at = |at: [f64; 3]| {
        let mut c = end_at(feet);
        for b in [
            (12, 66, 0),
            (13, 64, 0),
            (11, 64, 0),
            (12, 64, 1),
            (12, 64, -1),
        ] {
            c.step(&Event::Packet(S2c::BlockUpdate {
                pos: b,
                state: state(BlockKind::EndStone),
            }));
        }
        add(&mut c, 70, EntityKind::Enderman, at);
        data(&mut c, 70, vec![(17, MetaValue::Bool(true))]);
        c
    };
    let noted = |p: &Plan, what: &str| p.events_since(0).iter().any(|e| e.what == what);
    // Covered, within 8 but out of reach: held there.
    let w = covered_at([17.5, 64.0, 0.5]);
    let mut p = Plan::new(Aim::Race);
    let c = p.end_enderman_step(&w, 0).expect("held");
    assert!(c.sneak && !c.forward);
    assert!(noted(
        &p,
        "end: no pocket for enderman 70 (covered within 8: held)"
    ));
    // Held to the cap, then the choice again: out of reach, on with the task.
    assert!(
        p.end_enderman_step(&w, dragon::ENDER_HOLD_CAP - 1)
            .is_some()
    );
    assert_eq!(p.end_enderman_step(&w, dragon::ENDER_HOLD_CAP), None);
    assert!(
        noted(&p, "end: the hold from enderman 70 capped"),
        "{:?}",
        p.events_since(0)
    );
    assert!(noted(
        &p,
        "end: no pocket for enderman 70 (the hold capped: on with the task)"
    ));
    assert_eq!(p.end_enderman_step(&w, dragon::ENDER_HOLD_CAP + 400), None);
    // Covered, beyond 8: on with the task.
    let far = covered_at([24.5, 64.0, 0.5]);
    let mut q = Plan::new(Aim::Race);
    assert_eq!(q.end_enderman_step(&far, 0), None);
    assert!(noted(
        &q,
        "end: no pocket for enderman 70 (beyond 8: on with the task)"
    ));
    // On open ground beyond 8: no notch, no walk; on with the task.
    let mut open = end_at(feet);
    add(&mut open, 70, EntityKind::Enderman, [24.5, 64.0, 0.5]);
    data(&mut open, 70, vec![(17, MetaValue::Bool(true))]);
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([20, FLOOR, 0]);
    assert_eq!(r.end_enderman_step(&open, 0), None);
    assert!(r.take_acts().is_empty());
    // Within 8 on open ground, a walk to the bunker that fails is never tried again for it.
    let mut near = end_at(feet);
    add(&mut near, 70, EntityKind::Enderman, [17.5, 64.0, 0.5]);
    data(&mut near, 70, vec![(17, MetaValue::Bool(true))]);
    let mut t = Plan::new(Aim::Race);
    t.bunker = Some([20, FLOOR, 0]);
    t.dragon.ender_walk_failed = Some(70);
    assert!(t.end_enderman_step(&near, 0).is_some());
    assert!(
        noted(
            &t,
            "end: no pocket for enderman 70 (over the danger line, met in the open)"
        ),
        "{:?}",
        t.events_since(0)
    );
}

#[test]
fn unseen_in_a_strafe_the_bot_rises_into_the_dragons_sight() {
    // Frontier race 45943073264 lane 1: 10 under the surface, the strafe held 23,500 ticks.
    let pit = |roofed: bool| {
        let mut w = end_at([12.5, 61.0, 0.5]);
        for y in [61, 62, 63] {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: (12, y, 0),
                state: state(BlockKind::Air),
            }));
        }
        if roofed {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: (12, 64, 0),
                state: state(BlockKind::EndStone),
            }));
        }
        with_dragon(&mut w, [-30.5, 66.0, 0.5], 1, 0.0);
        w.inventory.slots.insert(36, (data::ItemKind::EndStone, 64));
        w.held = 0;
        w
    };
    let w = pit(false);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    let c = p.charge_step(&w, 0).expect("stood");
    assert!(!c.jump, "not yet");
    let c = p
        .charge_step(&w, dragon::STRAFE_UNSEEN_TICKS)
        .expect("rising");
    assert!(c.jump, "{c:?}");
    assert!(
        p.events_since(0).iter().any(|e| e.what
            == format!(
                "strafe: unseen {} ticks at [12, 61, 0], risen a block at a jump",
                dragon::STRAFE_UNSEEN_TICKS
            )),
        "{:?}",
        p.events_since(0)
    );
    // No sky over it: noted, left to the End's own rows.
    let w = pit(true);
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    let _ = q.charge_step(&w, 0);
    let _ = q.charge_step(&w, dragon::STRAFE_UNSEEN_TICKS);
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what.ends_with("at [12, 61, 0], no sky")),
        "{:?}",
        q.events_since(0)
    );
}

#[test]
fn a_fireball_on_a_pace_leg_is_dodged_on_the_way_the_bot_walks_and_never_by_the_fountain() {
    let feet = [12.5, 64.0, 0.5];
    // The dragon on +z would send a standing bot to -z; on a leg to +z it goes on to +z.
    let w = fireball_world(
        feet,
        [-30.5, 80.0, 20.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    p.dragon.pace_to = Some([12, 64, 6]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, 6]")
    );
    assert!(p.dragon.pace_to.is_none(), "the leg given up for the dodge");
    // Never within 5 of the fountain's axis: the other side.
    let by = [6.5, 64.0, 0.5];
    let w = fireball_world(by, [40.5, 80.0, 0.5], [6.5, 74.0, -20.5], [0.0, -0.3, 1.0]);
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    assert!(q.charge_step(&w, 0).is_some());
    let to = q.dragon.dodge_to.expect("a dodge");
    let axis = (f64::from(to[0]) + 0.5).hypot(f64::from(to[2]) + 0.5);
    assert!(axis >= dragon::FOUNTAIN_CLEAR, "{to:?} {axis}");
}

#[test]
fn a_fireball_is_dodged_ahead_of_the_walk_out_of_a_cloud_on_the_feet() {
    // Dev race 45947481217 lane 2: dodging, the bot stood on an old cloud's edge; the walk out of
    // it went the other way and the new fireball's cloud was put on it.
    let feet = [12.5, 64.0, 0.5];
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    cloud(&mut w, 60, [12.5, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, 10]")
    );
}

#[test]
fn the_dodge_goes_only_where_a_way_is_found_clear_of_every_cloud() {
    // Frontier race 45954274931: lanes 2 and 3's dodge walks failed NoPath at once and the cloud
    // was put on the bots; lane 5's dodge walked across an older cloud.
    let feet = [12.5, 64.0, 0.5];
    // A wall two high across the whole +z side at z 3: no way there, the dodge to -z.
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    for x in -32..32 {
        for y in [64, 65] {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: (x, y, 3),
                state: state(BlockKind::EndStone),
            }));
        }
    }
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, -6]")
    );
    // A small cloud across the +z way: never the cells in its reach, never a way through it.
    let mut c = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    cloud(&mut c, 60, [12.5, 64.0, 3.5], 1.0);
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    assert!(q.charge_step(&c, 0).is_some());
    let to = q.dragon.dodge_to.expect("a dodge");
    let off = dragon::flat(feet_of(to), [12.5, 64.0, 3.5]);
    assert!(off > 1.0 + dragon::BREATH_MARGIN + 1.0, "{to:?} {off}");
    assert_ne!(to, [12, 64, 6]);
}

#[test]
fn in_water_the_dodge_is_pressed_straight_on() {
    // Frontier race 45954274931 lanes 2 and 3: a layer of water over the ground, no walk planned
    // from it (NoPath), the bots stood and the cloud was put on them.
    let feet = [12.5, 64.0, 0.5];
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    for x in 10..15 {
        for z in -2..3 {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: (x, 64, z),
                state: state(BlockKind::Water),
            }));
        }
    }
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    let c = p.charge_step(&w, 0).expect("dodging");
    assert!(dodged_to(&p).is_some(), "{:?}", p.events_since(0));
    assert!(p.dragon.dodge_straight && p.dragon.dodge_walk.is_none());
    assert!(c.forward, "pressed on: {c:?}");
}

#[test]
fn the_strafing_dragon_facing_the_bot_is_dodged_before_its_fireball() {
    // Frontier race 45959448248 lanes 1 and 2: fireballs first seen 5 and 6 ticks before they
    // hit; the strafe fires within 10 degrees of its facing (DragonStrafePlayerPhase).
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    // At -30.5 facing +x (yaw 90): the bot dead ahead.
    with_dragon(&mut w, [-30.5, 80.0, 0.5], 1, 90.0);
    assert!(dragon::fire_imminent(&w, feet).is_some());
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_eq!(
        dodged_to(&p).as_deref(),
        Some("fireball: dodge to [12, 64, 6]")
    );
    assert_eq!(p.dragon.dodge_id, Some(dragon::IMMINENT));
    let e = p
        .events_since(0)
        .iter()
        .find(|e| e.what.starts_with("fireball: dodge to"))
        .unwrap();
    assert!(e.why.starts_with("its fire due"), "{}", e.why);
    // Its fireball comes: the same dodge, kept, not chosen again from wherever the bot is now.
    add(&mut w, 51, EntityKind::DragonFireball, [-10.5, 74.0, 0.5]);
    assert!(p.charge_step(&w, 5).is_some());
    assert_eq!(p.dragon.dodge_id, Some(51));
    assert_eq!(p.dragon.dodge_to, Some([12, 64, 6]));
    assert_eq!(
        p.events_since(0)
            .iter()
            .filter(|e| e.what.starts_with("fireball: dodge to"))
            .count(),
        1
    );
    // Facing away: not due.
    let mut away = end_at(feet);
    with_dragon(&mut away, [-30.5, 80.0, 0.5], 1, 270.0);
    assert!(dragon::fire_imminent(&away, feet).is_none());
}

#[test]
fn from_the_bunkers_stair_the_dodge_finds_the_ground_three_up() {
    // Frontier race 45964846590: "no dodge cell from [13, 59, 0]", the stair 3 under the ground.
    let feet = [12.5, 61.0, 0.5];
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    // A pit three down at (12, 61, 0) and a stair up it along +x to the ground at y 64.
    for c in [
        (12, 61, 0),
        (12, 62, 0),
        (12, 63, 0),
        (13, 62, 0),
        (13, 63, 0),
        (14, 63, 0),
    ] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::Air),
        }));
    }
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    let to = p.dragon.dodge_to.expect("a dodge cell on the ground");
    assert_eq!(to[1], FLOOR, "{to:?}");
}

#[test]
fn a_cloud_on_the_body_is_walked_out_of_never_a_notch_under_it() {
    // Dev race 45964846590 lanes 4, 5, 6: the walk out failed NoPath, a notch was dug under the
    // cloud, and the bots died in it (wall 36).
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    cloud(&mut w, 50, [12.5, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    let _ = p.breath_step(&w, 0).expect("out");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("out of the breath to"))
    );
    assert!(p.take_acts().iter().all(|a| !matches!(a, Act::Dig { .. })));
    // The walk refused (water, a stair): pressed straight on toward the cell out.
    let target = [12, 64, 6];
    p.dragon.breath_straight = Some(target);
    let c = p.breath_step(&w, 1).expect("pressed on");
    assert!(c.forward && !c.sneak, "{c:?}");
    assert!(p.take_acts().iter().all(|a| !matches!(a, Act::Dig { .. })));
    assert!(!p.dragon.breath_notch);
    // A strafe with a cloud near: no cover taken; the walk out is the breath's.
    let mut s = end_at(feet);
    with_dragon(&mut s, [-30.5, 80.0, 0.5], 1, 0.0);
    cloud(&mut s, 50, [17.5, 64.0, 0.5], 3.0);
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    let _ = q.charge_step(&s, 0);
    assert!(
        !q.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("strafe: shelter") || e.what.contains("notch")),
        "{:?}",
        q.events_since(0)
    );
}

#[test]
fn at_the_takeoff_the_bot_walks_out_of_the_bodys_knockback() {
    // E4's read of race 45964846590 lane 3: 10 off the fountain at the takeoff, two blows flung
    // the bot 85 east over the void.
    let feet = [6.5, 64.0, 0.5];
    let mut w = end_at(feet);
    with_dragon(&mut w, [0.5, 67.0, 0.5], 4, 0.0);
    assert!(dragon::knock_risk(&w, feet).is_some());
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("knock: dodge to")),
        "{:?}",
        p.events_since(0)
    );
    let to = p.dragon.dodge_to.expect("a cell out");
    assert!(
        dragon::flat(feet_of(to), [0.5, 0.0, 0.5]) > dragon::KNOCK_CLEAR,
        "{to:?}"
    );
    // Sitting: no knock, the strike's time.
    let mut sat = end_at(feet);
    with_dragon(&mut sat, [0.5, 67.0, 0.5], 6, 0.0);
    assert!(dragon::knock_risk(&sat, feet).is_none());
    // Far enough already: none.
    let far = [14.5, 64.0, 0.5];
    let mut out = end_at(far);
    with_dragon(&mut out, [0.5, 67.0, 0.5], 4, 0.0);
    assert!(dragon::knock_risk(&out, far).is_none());
}

#[test]
fn a_strike_stand_has_ground_behind_and_a_breath_walk_goes_off_the_fountain_at_a_perch() {
    // Ground behind, away from the body: none past the loaded ground (z and x over 31 here).
    let w = end_at([12.5, 64.0, 0.5]);
    assert!(dragon::ground_behind(&w, [0.5, 67.0, 0.5], [9, FLOOR, 0]));
    assert!(!dragon::ground_behind(&w, [0.5, 67.0, 0.5], [20, FLOOR, 0]));
    // A cloud on the feet by the perch, the dragon sitting: the cell out is off the fountain.
    let feet = [8.5, 64.0, 0.5];
    let mut b = end_at(feet);
    with_dragon(&mut b, [0.5, PERCH_Y, 0.5], 6, 270.0);
    cloud(&mut b, 50, [8.5, 64.0, 0.5], 3.0);
    let c = dragon::out_of_breath(&b, feet).expect("a cell out");
    assert!(
        dragon::flat(feet_of(c), [0.5, 0.0, 0.5]) >= dragon::KNOCK_CLEAR,
        "{c:?}"
    );
}

#[test]
fn with_no_way_out_on_foot_a_fireball_is_answered_by_rising_out() {
    // Frontier race 45969992660 lane 2: in the bunker's shaft, its stair shut, "no dodge cell".
    let feet = [12.5, 61.0, 0.5];
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    for y in [61, 62, 63] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    w.inventory.slots.insert(36, (data::ItemKind::EndStone, 64));
    w.held = 0;
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    let c = p.charge_step(&w, 0).expect("rising");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "fireball: no dodge cell from [12, 61, 0]"),
        "{:?}",
        p.events_since(0)
    );
    assert!(c.jump, "a jump from the pit's floor: {c:?}");
}

#[test]
fn in_sight_in_a_strafe_the_task_in_hand_goes_on_and_only_idle_is_paced() {
    // Frontier race 45974410172 lane 3: the pace took the planner from the work every strafe,
    // each strafe firing within 100 to 400 ticks of the bot in sight.
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    // Strafing, in sight, facing across the bot (its fire not due).
    with_dragon(&mut w, [-30.5, 80.0, 0.5], 1, 0.0);
    let mut busy = Plan::new(Aim::Race);
    busy.bunker = Some([60, FLOOR - 4, 60]);
    busy.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    assert_eq!(busy.charge_step(&w, 0), None, "the task goes on");
    assert!(busy.dragon.pace_walk.is_none());
    // Nothing in hand: paced.
    let mut idle = Plan::new(Aim::Race);
    idle.bunker = Some([60, FLOOR - 4, 60]);
    assert!(idle.charge_step(&w, 0).is_some());
    assert!(
        idle.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("strafe: paced in sight"))
    );
    // Its fire due (facing the bot): dodged, task or not.
    let mut due = end_at(feet);
    with_dragon(&mut due, [-30.5, 80.0, 0.5], 1, 90.0);
    let mut d = Plan::new(Aim::Race);
    d.bunker = Some([60, FLOOR - 4, 60]);
    d.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    assert!(d.charge_step(&due, 0).is_some());
    assert!(
        d.events_since(0)
            .iter()
            .any(|e| e.what.starts_with("fireball: dodge to"))
    );
}

#[test]
fn with_no_way_out_and_no_block_the_wall_toward_the_open_is_dug() {
    // Frontier race 45978279345: five bots on the bunker's stair with no block to rise on,
    // "risen a block at a jump" and never risen; the cloud was put on them.
    let feet = [12.5, 61.0, 0.5];
    let mut w = fireball_world(
        feet,
        [-30.5, 80.0, 0.5],
        [-10.5, 74.0, 0.5],
        [1.0, -0.3, 0.0],
    );
    for y in [61, 62, 63] {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, y, 0),
            state: state(BlockKind::Air),
        }));
    }
    w.inventory.slots.clear();
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some());
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "fireball: no block to rise on, the wall dug at [12, 61, 1]"),
        "{:?}",
        p.events_since(0)
    );
    assert!(p.take_acts().iter().any(|a| matches!(a, Act::Dig { .. })));
}

#[test]
fn no_perch_is_fought_while_a_crystal_stands() {
    // Wall 45, dev race 46050674145 lane 0: with only the far crystals standing, 28 strikes and
    // every perch refilled to 200 in flight (EnderDragon.checkCrystals: the nearest in its box
    // grown by 32 heals it 1 every 10 ticks, its holding pattern passing within 40 of them).
    let feet = [9.5, 64.0, 0.5];
    let sat = |crystals: &[[f64; 3]]| {
        let mut w = end_at(feet);
        with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 270.0);
        for (i, c) in crystals.iter().enumerate() {
            add(&mut w, 80 + i as i32, EntityKind::EndCrystal, *c);
        }
        w
    };
    let spike = [42.5, 83.0, 0.5];
    let other = [12.5, 77.0, 39.5];
    assert!(dragon::heals_from(dragon::PERCH_POINT, other));
    assert!(
        !dragon::heals_from(dragon::PERCH_POINT, spike),
        "(42, 0): 42 on x, past 40"
    );
    for crystals in [
        &[spike][..],
        &[spike, other][..],
        &[spike, [-41.5, 89.0, -0.5]][..],
    ] {
        let w = sat(crystals);
        assert!(dragon::fight_with_crystals(&w).is_none(), "{crystals:?}");
        let mut p = Plan::new(Aim::Race);
        p.perch_fight_step(&w, 0);
        assert!(matches!(p.task, plan::Task::Idle), "{crystals:?}");
        assert!(p.dragon_task(&w, 0).is_none(), "{crystals:?}");
    }
    // None standing: the fight is the task.
    let mut p = Plan::new(Aim::Race);
    assert!(p.dragon_task(&sat(&[]), 0).is_some());
}
#[test]
fn a_heal_is_written_to_the_ledger_and_a_meal_waits_for_the_dragon_far_off() {
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    with_dragon(&mut w, [0.5, 90.0, 40.5], 0, 0.0);
    add(&mut w, 80, EntityKind::EndCrystal, [12.5, 77.0, 39.5]);
    data(&mut w, DRAGON, vec![(9, MetaValue::Float(180.0))]);
    let mut p = Plan::new(Aim::Race);
    let _ = p.dragon_watch(&w, 0);
    data(&mut w, DRAGON, vec![(9, MetaValue::Float(185.0))]);
    let _ = p.dragon_watch(&w, 20);
    let e = p
        .events_since(0)
        .iter()
        .find(|e| e.what == "dragon healed")
        .expect("noted");
    assert!(e.why.starts_with("5.0 in all"), "{}", e.why);
    // Holding 40 off (about 43 here): a meal may be had; nearer, or any other phase: not.
    assert!(!p.dragon_holds_meal(&w));
    let mut near = end_at(feet);
    with_dragon(&mut near, [20.5, 80.0, 0.5], 0, 0.0);
    assert!(p.dragon_holds_meal(&near));
    let mut strafe = end_at(feet);
    with_dragon(&mut strafe, [0.5, 90.0, 60.5], 1, 0.0);
    assert!(p.dragon_holds_meal(&strafe));
    // A cloud on the feet, the dragon holding far: no meal (race 46038776204 lane 8).
    let mut cloudy = end_at(feet);
    with_dragon(&mut cloudy, [0.5, 90.0, 60.5], 0, 0.0);
    cloud(&mut cloudy, 50, [12.5, 64.0, 0.5], 3.0);
    assert!(p.dragon_holds_meal(&cloudy));
}

#[test]
fn with_the_dragon_coming_down_the_bot_gets_onto_ground_it_can_dodge_a_charge_from() {
    // Race 46038776204 lanes 1, 3 and 4: a charge 80 to 160 ticks after the scan began, no
    // dodge cell (on a pillar, in a surface pit), knocked off.
    let coming = |w: &mut World| with_dragon(w, [-40.5, 90.0, 0.5], 2, 0.0);
    let blocks = |w: &mut World| {
        w.inventory.slots.insert(36, (data::ItemKind::EndStone, 64));
        w.held = 0;
    };
    let set = |w: &mut World, cells: &[(i32, i32, i32)], k: BlockKind| {
        for c in cells {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: *c,
                state: state(k),
            }));
        }
    };
    // A pit two deep: no walk out; risen a block at a jump.
    let mut pit = end_at([12.5, 62.0, 0.5]);
    set(&mut pit, &[(12, 62, 0), (12, 63, 0)], BlockKind::Air);
    coming(&mut pit);
    blocks(&mut pit);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    let c = p.charge_step(&pit, 0).expect("out of the pit");
    assert!(c.jump, "{c:?}");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "charge: no dodge from [12, 62, 0] with the dragon coming down")
    );
    // A pit one deep: a step out, a dodge to be had; nothing to do.
    let mut shallow = end_at([12.5, 63.0, 0.5]);
    set(&mut shallow, &[(12, 63, 0)], BlockKind::Air);
    coming(&mut shallow);
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    assert_eq!(q.charge_step(&shallow, 0), None);
    // A short pillar (two high): a drop to the ground, a dodge to be had; nothing to do.
    let mut low = end_at([12.5, 66.0, 0.5]);
    set(&mut low, &[(12, 64, 0), (12, 65, 0)], BlockKind::EndStone);
    coming(&mut low);
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([60, FLOOR - 4, 60]);
    assert_eq!(r.charge_step(&low, 0), None);
    // A pillar five high: dug down, never risen.
    let mut tall = end_at([12.5, 69.0, 0.5]);
    set(
        &mut tall,
        &[
            (12, 64, 0),
            (12, 65, 0),
            (12, 66, 0),
            (12, 67, 0),
            (12, 68, 0),
        ],
        BlockKind::EndStone,
    );
    coming(&mut tall);
    blocks(&mut tall);
    let mut t = Plan::new(Aim::Race);
    t.bunker = Some([60, FLOOR - 4, 60]);
    let c = t.charge_step(&tall, 0).expect("down");
    assert!(!c.jump);
    assert!(t.take_acts().iter().any(|a| matches!(a, Act::Dig { .. })));
}

#[test]
fn while_the_dragon_scans_the_bot_paces_wider_than_its_wings_sweep() {
    // Race 46038776204 lane 6: a 6-cell dodge on the charge's line, knocked 70 off the island;
    // the wings knock within about 10.5 of the body's line.
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    with_dragon(&mut w, [0.5, PERCH_Y, 0.5], 6, 270.0);
    // Crystals standing, one in heal reach (the perch not fought): paced while it scans.
    add(&mut w, 80, EntityKind::EndCrystal, [12.5, 77.0, 39.5]);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&w, 0).is_some(), "paced");
    let to = p.dragon.pace_to.expect("a leg");
    let off = to[2].abs();
    assert!(off >= dragon::CHARGE_SPAN.0, "{to:?}");
    assert!(p.dragon.pace_walk.is_some());
    // The next tick keeps the leg (not chosen again).
    assert!(p.charge_step(&w, 1).is_some());
    assert_eq!(p.dragon.pace_to, Some(to));
    // Fighting it (the perch's strikes): no pace.
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    q.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    assert_eq!(q.charge_step(&w, 0), None);
    // No crystal standing: the fight is the next choice, never held off by a pace.
    let mut none = end_at(feet);
    with_dragon(&mut none, [0.5, PERCH_Y, 0.5], 6, 270.0);
    let mut r = Plan::new(Aim::Race);
    r.bunker = Some([60, FLOOR - 4, 60]);
    assert_eq!(r.charge_step(&none, 0), None);
}

#[test]
fn a_meal_is_cut_when_the_dragon_comes_and_capped_at_100_ticks() {
    // Dev race 46048455555 lane 1: an eat begun on a pillar ran 2,360 ticks through two perches.
    let feet = [12.5, 64.0, 0.5];
    let eating = |w: &World, at: u64| {
        let mut p = Plan::new(Aim::Race);
        p.task = plan::Task::Eat(plan::eat::Eat::new(w, data::ItemKind::CookedBeef, at));
        p
    };
    let eat = Some(plan::make::Always::Eat);
    // The dragon strafing: cut at once.
    let mut near = end_at(feet);
    with_dragon(&mut near, [0.5, 80.0, 20.5], 1, 0.0);
    let mut p = eating(&near, 0);
    assert_eq!(p.meal_cut(&near, 5, eat), None);
    assert!(matches!(p.task, plan::Task::Idle));
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "eat cut: the dragon near or a cloud")
    );
    // Holding far off: kept to the cap, then cut.
    let mut far = end_at(feet);
    with_dragon(&mut far, [0.5, 90.0, 60.5], 0, 0.0);
    let mut q = eating(&far, 0);
    assert_eq!(q.meal_cut(&far, 50, eat), eat);
    assert!(matches!(q.task, plan::Task::Eat(_)));
    assert_eq!(q.meal_cut(&far, dragon::MEAL_CAP_TICKS + 1, eat), None);
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what == "eat cut: over the meal's cap")
    );
}

#[test]
fn two_angry_endermen_are_answered_with_water_at_the_feet_taken_back_after() {
    // Race 46063334187 lane 7: four angry at once after a charge, fought with the sword, lost.
    let feet = [12.5, 64.0, 0.5];
    let angry = |w: &mut World, id: i32, at: [f64; 3]| {
        add(w, id, EntityKind::Enderman, at);
        data(w, id, vec![(17, MetaValue::Bool(true))]);
    };
    let bucket = |w: &mut World, k: data::ItemKind| {
        w.inventory.slots.insert(36, (k, 1));
        w.held = 0;
    };
    let mut w = end_at(feet);
    angry(&mut w, 70, [15.5, 64.0, 0.5]);
    angry(&mut w, 71, [12.5, 64.0, 5.5]);
    bucket(&mut w, data::ItemKind::WaterBucket);
    let mut p = Plan::new(Aim::Race);
    let c = p.end_enderman_step(&w, 0).expect("pouring");
    assert!(c.pitch.is_some_and(|x| x > 60.0), "looking down: {c:?}");
    assert!(p.take_acts().contains(&Act::UseItem));
    assert!(
        p.take_acts()
            .iter()
            .all(|a| !matches!(a, Act::Attack { .. })),
        "never two fought"
    );
    // The source at the feet: stood in, noted.
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (12, 64, 0),
        state: state(BlockKind::Water),
    }));
    let _ = p.end_enderman_step(&w, 20);
    assert_eq!(p.dragon.ender_water, Some([12, 64, 0]));
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "end: water poured at [12, 64, 0] against 2 angry endermen")
    );
    // None angry near: the water taken back with the empty bucket.
    let mut calm = end_at(feet);
    calm.step(&Event::Packet(S2c::BlockUpdate {
        pos: (12, 64, 0),
        state: state(BlockKind::Water),
    }));
    bucket(&mut calm, data::ItemKind::Bucket);
    let mut lost = p.clone();
    assert!(p.ender_water_step(&calm, 40).is_some());
    assert!(p.take_acts().contains(&Act::UseItem));
    // Noted only once the bucket is full again (wall 49: 67 take-back notes for 9 pours).
    let taken = |p: &Plan| {
        p.events_since(0)
            .iter()
            .any(|e| e.what == "end: the water against endermen taken back at [12, 64, 0]")
    };
    assert!(!taken(&p));
    let mut full = end_at(feet);
    bucket(&mut full, data::ItemKind::WaterBucket);
    assert_eq!(p.ender_water_step(&full, 60), None);
    assert!(taken(&p));
    assert_eq!(p.dragon.ender_water, None);
    // A take-back that fails 3 times: the water lost there, left for the refill.
    for t in [40, 60, 80, 100] {
        let _ = lost.ender_water_step(&calm, t);
        let _ = lost.take_acts();
    }
    assert!(
        lost.spikes.left_water.contains(&(12, 64, 0)),
        "{:?}",
        lost.spikes.left_water
    );
    assert!(
        lost.events_since(0)
            .iter()
            .any(|e| e.what == "end: the water against endermen lost at [12, 64, 0]")
    );
    // One angry: water all the same (wall 48: a single one beat a full-iron bot with the sword).
    let mut one = end_at(feet);
    angry(&mut one, 70, [13.5, 64.0, 0.5]);
    bucket(&mut one, data::ItemKind::WaterBucket);
    let mut q = Plan::new(Aim::Race);
    assert!(q.ender_water_step(&one, 0).is_some());
    assert!(q.take_acts().contains(&Act::UseItem));
    // No water bucket held: the sword is the fallback.
    let mut dry = end_at(feet);
    angry(&mut dry, 70, [13.5, 64.0, 0.5]);
    let mut r = Plan::new(Aim::Race);
    assert_eq!(r.ender_water_step(&dry, 0), None);
}

#[test]
fn a_pace_leg_a_cloud_reaches_is_given_up_and_recovery_comes_first_at_low_health() {
    // Race 46063334187 lane 9: paced into a cloud; lane 4 walked through clouds at 1.7 health.
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    with_dragon(&mut w, [-30.5, 80.0, 0.5], 1, 0.0);
    cloud(&mut w, 50, [12.5, 64.0, 6.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    p.dragon.pace_to = Some([12, 64, 6]);
    assert!(p.charge_step(&w, 0).is_some());
    assert_ne!(
        p.dragon.pace_to,
        Some([12, 64, 6]),
        "the leg into the cloud given up"
    );
    // Under 10: work cut, noted, kept away from the cloud.
    let mut low = end_at(feet);
    cloud(&mut low, 50, [14.5, 64.0, 0.5], 3.0);
    low.player.health = Some(6.0);
    let mut r = Plan::new(Aim::Race);
    r.task = plan::Task::Dragon(Box::new(dragon::Fight::new(0)));
    let c = r.recover_step(&low, 0).expect("recovering");
    assert!(matches!(r.task, plan::Task::Idle));
    assert!(c.forward, "away from the cloud: {c:?}");
    assert!(
        r.events_since(0)
            .iter()
            .any(|e| e.what == "recover: health 6.0 under 10")
    );
    // Back over 16: the work goes on.
    low.player.health = Some(17.0);
    assert_eq!(r.recover_step(&low, 100), None);
    assert!(!r.dragon.recovering);
}

#[test]
fn a_strike_at_an_enderman_waits_a_full_charge_after_the_sword_comes_to_hand() {
    // Race 46071506005 lane 5: strikes of 1.3 between full ones of 6.0, the pickaxe brought back
    // between them by the dig in hand (a swap resets the server's attack charge).
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    add(&mut w, 70, EntityKind::Enderman, [14.0, 64.0, 0.5]);
    data(&mut w, 70, vec![(17, MetaValue::Bool(true))]);
    w.inventory.slots.insert(36, (data::ItemKind::IronSword, 1));
    w.inventory
        .slots
        .insert(37, (data::ItemKind::StonePickaxe, 1));
    // The pickaxe in hand: the sword taken up, no strike.
    w.held = 1;
    let mut p = Plan::new(Aim::Race);
    assert!(p.end_enderman_step(&w, 0).is_some());
    assert!(
        p.take_acts()
            .iter()
            .all(|a| !matches!(a, Act::Attack { .. }))
    );
    // The sword in hand at tick 1: still no strike until a full charge has passed.
    w.held = 0;
    let _ = p.end_enderman_step(&w, 1);
    assert!(
        p.take_acts()
            .iter()
            .all(|a| !matches!(a, Act::Attack { .. }))
    );
    let _ = p.end_enderman_step(&w, 20);
    assert!(
        p.take_acts()
            .iter()
            .any(|a| matches!(a, Act::Attack { entity: 70 }))
    );
}

#[test]
fn walks_out_of_one_breath_are_capped_then_pressed_straight_away() {
    // Wall 50, race 46071506005 lanes 0 and 3: "out of the breath" 120 to 250 times a lane.
    let feet = [12.5, 64.0, 0.5];
    let mut w = end_at(feet);
    cloud(&mut w, 50, [13.5, 64.0, 0.5], 3.0);
    let mut p = Plan::new(Aim::Race);
    p.dragon.breath_episode_outs = dragon::BREATH_OUTS_MAX;
    let c = p.breath_step(&w, 0).expect("pressed away");
    // Away from the cloud's middle at +x: toward -x (yaw 90).
    assert!(c.forward && (c.yaw - 90.0).abs() < 1.0, "{c:?}");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "out of the breath: walks capped")
    );
    // A failed dig's cell is kept for the episode while a cloud's end reach takes the bot in.
    p.dragon.dug_failed.push([13, 64, 0]);
    let mut near = end_at(feet);
    cloud(&mut near, 50, [18.5, 64.0, 0.5], 3.0);
    let _ = p.breath_step(&near, 10);
    assert!(p.dragon.dug_failed.contains(&[13, 64, 0]));
    // Far from every cloud: the episode over, its records cleared.
    let mut far = end_at(feet);
    cloud(&mut far, 50, [-20.5, 64.0, 0.5], 3.0);
    let _ = p.breath_step(&far, 20);
    assert!(p.dragon.dug_failed.is_empty());
    assert_eq!(p.dragon.breath_episode_outs, 0);
}

#[test]
fn a_rise_with_a_block_over_the_head_or_jumps_for_nothing_is_given_up() {
    // Wall 51: dev race 46099602480 lane 0 jumped in the bunker's half-opened stair (x 14.0, the
    // shut cell over one of its two columns) from tick 2020 to 2600, through a whole perch, and
    // E3's open of the bunker stalled under it.
    let coming = |w: &mut World| with_dragon(w, [-40.5, 90.0, 0.5], 2, 0.0);
    let set = |w: &mut World, cells: &[(i32, i32, i32)], k: BlockKind| {
        for c in cells {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: *c,
                state: state(k),
            }));
        }
    };
    let pit = || {
        let mut w = end_at([12.0, 62.0, 0.5]);
        set(
            &mut w,
            &[(11, 62, 0), (11, 63, 0), (12, 62, 0), (12, 63, 0)],
            BlockKind::Air,
        );
        coming(&mut w);
        w.inventory.slots.insert(36, (data::ItemKind::EndStone, 64));
        w.held = 0;
        w
    };
    // The sky open over the feet's own column, a block two over the feet in the other one.
    let mut shut = pit();
    set(&mut shut, &[(11, 64, 0)], BlockKind::EndStone);
    assert!(!dragon::rise_room(&shut, [12.0, 62.0, 0.5]));
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    assert!(p.charge_step(&shut, 0).is_none_or(|c| !c.jump));
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "rise: none from [12, 62, 0]"),
        "{:?}",
        p.events_since(0)
    );
    // Room over the head, but no block goes in: `RISE_JUMPS_MAX` jumps, then given up there.
    let open = pit();
    assert!(dragon::rise_room(&open, [12.0, 62.0, 0.5]));
    let mut q = Plan::new(Aim::Race);
    q.bunker = Some([60, FLOOR - 4, 60]);
    let jumps = (0..10u64)
        .filter(|t| q.charge_step(&open, *t).is_some_and(|c| c.jump))
        .count();
    assert_eq!(jumps, dragon::RISE_JUMPS_MAX as usize);
    assert!(
        q.events_since(0)
            .iter()
            .any(|e| e.what == "rise: given up at [12, 62, 0]")
    );
    assert!(q.dragon.rise_failed.contains(&[12, 62, 0]));
}

#[test]
fn a_pour_gone_a_block_under_is_found_and_a_take_back_is_sent_only_from_the_ground_in_reach() {
    // Wall 52: race 46099479836 lane 4, the water went in a block under the feet's cell and was
    // never found (E4's refill sent to the dry cell over it); lane 2, knocked off by a charge,
    // the take-back's 3 uses went from the air in 40 ticks.
    let feet = [12.5, 64.0, 0.5];
    let angry = |w: &mut World, id: i32, at: [f64; 3]| {
        add(w, id, EntityKind::Enderman, at);
        data(w, id, vec![(17, MetaValue::Bool(true))]);
    };
    let bucket = |w: &mut World, k: data::ItemKind| {
        w.inventory.slots.insert(36, (k, 1));
        w.held = 0;
    };
    let mut w = end_at(feet);
    angry(&mut w, 70, [15.5, 64.0, 0.5]);
    bucket(&mut w, data::ItemKind::WaterBucket);
    let mut p = Plan::new(Aim::Race);
    assert!(p.ender_water_step(&w, 0).is_some());
    assert!(p.take_acts().contains(&Act::UseItem));
    // The source shows a block under the feet's cell.
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (12, 63, 0),
        state: state(BlockKind::Water),
    }));
    let _ = p.ender_water_step(&w, 5);
    assert_eq!(p.dragon.ender_water, Some([12, 63, 0]));
    // None angry, the bot knocked 6 off in the air: no use sent; on the ground, walked back.
    let calm_at = |at: [f64; 3], ground: bool| {
        let mut c = end_at(at);
        c.step(&Event::Packet(S2c::BlockUpdate {
            pos: (12, 63, 0),
            state: state(BlockKind::Water),
        }));
        bucket(&mut c, data::ItemKind::Bucket);
        if let Some(b) = c.body.as_mut() {
            b.on_ground = ground;
        }
        c
    };
    let air = calm_at([18.5, 66.0, 0.5], false);
    let _ = p.ender_water_step(&air, 40);
    assert!(
        !p.take_acts().contains(&Act::UseItem),
        "no use from the air"
    );
    let off = calm_at([18.5, 64.0, 0.5], true);
    let c = p.ender_water_step(&off, 60).expect("back to the source");
    assert!(c.forward, "{c:?}");
    assert!(!p.take_acts().contains(&Act::UseItem));
    assert_eq!(p.dragon.ender_take, None, "no use counted");
    // In reach on the ground: the use goes.
    let near = calm_at([13.5, 64.0, 0.5], true);
    let _ = p.ender_water_step(&near, 80);
    assert!(p.take_acts().contains(&Act::UseItem));
    // Too far to walk back: left there for the refill, noted.
    let far = calm_at([40.5, 64.0, 0.5], true);
    assert_eq!(p.ender_water_step(&far, 100), None);
    assert!(p.spikes.left_water.contains(&(12, 63, 0)));
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "end: the water against endermen lost at [12, 63, 0]")
    );
}

#[test]
fn a_takeoff_with_the_bot_ahead_of_the_head_is_dodged_from_its_first_tick() {
    // Wall 53: dev race 46099602480 lane 0, 13.4 ahead and 1.2 across at the takeoff: the dodge
    // began only inside 10, six ticks in; two blows of 5.
    let feet = [0.5, 64.0, -12.5];
    // Yaw 0 faces -z (sin 0, -cos 0): the bot 13 ahead.
    let mut w = end_at(feet);
    with_dragon(&mut w, [0.5, 66.0, 0.5], 4, 0.0);
    let (flight, d) = dragon::knock_risk(&w, feet).expect("in the takeoff's lane");
    assert!((d - 13.0).abs() < 0.01, "{d}");
    assert!(
        flight[0].abs() < 1e-6 && (flight[1] + 1.0).abs() < 1e-6,
        "{flight:?}"
    );
    // Behind the head, 13 off: out of its wings' reach.
    let mut behind = end_at(feet);
    with_dragon(&mut behind, [0.5, 66.0, 0.5], 4, 180.0);
    assert_eq!(dragon::knock_risk(&behind, feet), None);
    // Ahead but wide of the lane: nothing.
    let wide = [14.5, 64.0, -12.5];
    let mut side = end_at(wide);
    with_dragon(&mut side, [0.5, 66.0, 0.5], 4, 0.0);
    assert_eq!(dragon::knock_risk(&side, wide), None);
    // Still sitting (the scan): not yet.
    let mut scan = end_at(feet);
    with_dragon(&mut scan, [0.5, 66.0, 0.5], 6, 0.0);
    assert_eq!(dragon::knock_risk(&scan, feet), None);
}

#[test]
fn recovery_never_stands_in_a_cloud_it_is_pressed_out_away_from_the_dragon() {
    // Wall 54: race 46099479836 lane 7, at 8 health in a cloud 0.1 off with no dodge cell,
    // recovery stood (and in water, where a body goes nowhere without a jump); 8 to 2, dead.
    let set = |w: &mut World, cells: &[(i32, i32, i32)], k: BlockKind| {
        for c in cells {
            w.step(&Event::Packet(S2c::BlockUpdate {
                pos: *c,
                state: state(k),
            }));
        }
    };
    // In a pit two deep (no dodge cell), the cloud on the feet: pressed, never a stand.
    let feet = [12.5, 62.0, 0.5];
    let mut w = end_at(feet);
    set(&mut w, &[(12, 62, 0), (12, 63, 0)], BlockKind::Air);
    cloud(&mut w, 50, feet, 3.0);
    w.player.health = Some(8.0);
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([60, FLOOR - 4, 60]);
    let c = p.recover_step(&w, 0).expect("recovering");
    assert!(c.forward, "pressed out, never a stand: {c:?}");
    assert!(
        p.events_since(0)
            .iter()
            .any(|e| e.what == "recover: pressed out of the breath at [12, 62, 0]"),
        "{:?}",
        p.events_since(0)
    );
    // The middle on the feet: the way away from the dragon (at -x), so +x (yaw -90); in water,
    // a jump.
    let open = [12.5, 64.0, 0.5];
    let mut o = end_at(open);
    set(&mut o, &[(12, 64, 0)], BlockKind::Water);
    let c = dragon::press_out(&o, open, open, Some([-20.5, 70.0, 0.5]));
    assert!((c.yaw + 90.0).abs() < 1.0, "{c:?}");
    assert!(c.jump, "swims in water: {c:?}");
    // Off the middle: straight away from it (+z, yaw 0).
    let c = dragon::press_out(&o, open, [12.5, 64.0, -1.5], None);
    assert!(c.yaw.abs() < 1.0, "{c:?}");
}

#[test]
fn the_perched_dragons_flame_keeps_its_reach_a_fireballs_cloud_grows() {
    // Wall 55: dev race 46099602480 lane 1, the flame (radius 5 from its first tick, gone when the
    // flaming turn ends) taken as growing to 8.5: "walk held clear of a breath's end reach" and
    // "backs off a breath's growth" held the bot 13 to 15 out through every flaming turn, back at
    // the head stand only as the attacking turn ended; 33 strikes in 12 perches.
    let mut w = end_at([12.5, 64.0, 0.5]);
    cloud(&mut w, 50, [4.5, 64.0, 0.5], 5.0);
    cloud(&mut w, 51, [20.5, 64.0, 0.5], 3.0);
    let b = breaths(&w);
    let flame = b.iter().find(|b| b.id == 50).expect("the flame");
    let fire = b.iter().find(|b| b.id == 51).expect("the cloud");
    assert!(!flame.grows);
    assert!((flame.end_reach() - flame.reach()).abs() < 1e-9);
    assert!(fire.grows);
    assert!(fire.end_reach() > flame.end_reach());
}
