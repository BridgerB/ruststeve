//! The bow line (agent E8, docs/problems/end-bow.md): the arrow's flight from the jar, the aim's
//! pitch, the hit test, the stands; and the shot on a hand-built End: an end stone floor at y 63
//! (the feet at 64), a spike round (8, 8) with its bedrock at 70 and its crystal on it.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::bow::{self, Miss, Stage};
use plan::spikes::Spike;
use plan::{Aim, Plan, Task};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Act, Event, World};

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

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

const CRYSTAL: i32 = 7;
const CRYSTAL_AT: [f64; 3] = [8.5, 71.0, 8.5];
const FEET: [f64; 3] = [8.5, 64.0, -15.5];

/// The End: end stone from y 0 to 63, air over it, 5 by 5 chunk columns round the origin; a
/// spike of radius 2 round (8, 8), obsidian 64 to 69, bedrock at 70, its crystal on it; the feet
/// at `pos` on the ground, a bow in hand (hotbar 0) and 16 arrows (hotbar 1).
fn world(pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::EndStone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:the_end".to_owned(),
        game_type: 0,
    }));
    for x in -2..=2 {
        for z in -2..=2 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    for dx in -2i32..=2 {
        for dz in -2i32..=2 {
            if dx * dx + dz * dz <= 5 {
                for y in 64..70 {
                    set(&mut w, (8 + dx, y, 8 + dz), BlockKind::Obsidian);
                }
            }
        }
    }
    set(&mut w, (8, 70, 8), BlockKind::Bedrock);
    w.step(&Event::Packet(S2c::AddEntity {
        id: CRYSTAL,
        uuid: [0; 16],
        kind: EntityKind::EndCrystal.id(),
        x: CRYSTAL_AT[0],
        y: CRYSTAL_AT[1],
        z: CRYSTAL_AT[2],
        velocity: [0.0; 3],
    }));
    w.inventory.slots.insert(36, (ItemKind::Bow, 1));
    w.inventory.slots.insert(37, (ItemKind::Arrow, 16));
    w.held = 0;
    at(&mut w, pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

fn at(w: &mut World, pos: [f64; 3]) {
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

/// Every note with its why, for a failing assertion's message.
fn told(p: &Plan) -> Vec<String> {
    p.events
        .iter()
        .map(|e| format!("{}: {}", e.what, e.why))
        .collect()
}

fn open(_: (i32, i32, i32)) -> bool {
    false
}

#[test]
fn the_arrow_flies_as_the_jar_moves_it() {
    // Level at full power: the first move is the whole 3.0, then the drag and gravity.
    let p = bow::fly([0.0, 0.0, 0.0], [0.0, 0.0, bow::SPEED], 3);
    assert_eq!(p[1], [0.0, 0.0, 3.0]);
    assert!((p[2][2] - (3.0 + 2.97)).abs() < 1e-9, "{:?}", p[2]);
    assert!((p[2][1] - (-0.05)).abs() < 1e-9, "{:?}", p[2]);
    assert!((p[3][1] - (-0.05 - 0.0995)).abs() < 1e-9, "{:?}", p[3]);
    // Notchian yaw: south 0 is +z, west 90 is -x; pitch down positive.
    let s = bow::direction(0.0, 0.0);
    assert!((s[2] - 1.0).abs() < 1e-12 && s[0].abs() < 1e-12);
    let wst = bow::direction(90.0, 0.0);
    assert!((wst[0] + 1.0).abs() < 1e-12);
    let up = bow::direction(0.0, -90.0);
    assert!((up[1] - 1.0).abs() < 1e-12);
    assert!(bow::yaw_gap(359.5, 0.5) < 1.0 + 1e-9);
}

#[test]
fn the_pitch_is_the_low_arc_through_the_point() {
    for (flat, rise) in [(20.0, 0.0), (30.0, 10.0), (40.0, 30.0), (25.0, -5.0)] {
        let p = bow::solve_pitch(flat, rise).expect("in range");
        let y = bow::height_at(flat, p).expect("reaches");
        assert!(
            (y - rise).abs() < 0.01,
            "{flat} {rise}: pitch {p}, height {y}"
        );
        assert!(p < 0.0 || rise < 0.0, "a rise is shot upward: {p}");
        assert!(p > -60.0, "the low arc: {p}");
    }
    assert_eq!(bow::solve_pitch(4000.0, 0.0), None, "out of range");
}

#[test]
fn the_aim_meets_the_crystals_box_from_the_ground() {
    let a = bow::aim(FEET, CRYSTAL_AT, &mut open).expect("a shot");
    assert!(a.range > bow::SAFE_RANGE && a.range < bow::MAX_RANGE);
    assert!(a.pitch < 0.0, "up at it: {}", a.pitch);
    assert!(a.yaw.abs() < 1e-6, "due south: {}", a.yaw);
    // The flight along the sent aim passes inside the bare box (no margin).
    let d = bow::direction(a.yaw, f64::from(a.pitch));
    let v = [d[0] * bow::SPEED, d[1] * bow::SPEED, d[2] * bow::SPEED];
    let pts = bow::fly(bow::start(FEET), v, 40);
    let (lo, hi) = bow::crystal_box(CRYSTAL_AT, 0.0);
    assert!(
        pts.windows(2)
            .any(|s| bow::segment_box(s[0], s[1], lo, hi).is_some()),
        "the bare box is hit"
    );
    assert_eq!(
        bow::aim([8.5, 64.0, -2.5], CRYSTAL_AT, &mut open),
        Err(Miss::Near),
        "inside the blast's reach"
    );
    assert_eq!(
        bow::aim([8.5, 64.0, -60.5], CRYSTAL_AT, &mut open),
        Err(Miss::Far)
    );
    let mut wall = |c: (i32, i32, i32)| c.2 == -5 && c.1 < 75;
    assert!(
        matches!(
            bow::aim(FEET, CRYSTAL_AT, &mut wall),
            Err(Miss::Blocked((_, _, -5)))
        ),
        "a wall across the way"
    );
}

#[test]
fn the_stands_go_toward_the_middle_first() {
    let cols = bow::stand_columns([40, 0], [0.0, 0.0]);
    assert_eq!(cols[0], [22, 0], "18 toward the fountain first");
    assert!(cols.iter().all(|c| {
        let d = f64::from(c[0] - 40).hypot(f64::from(c[1]));
        (16.0..=41.0).contains(&d)
    }));
    assert!(cols.iter().all(|c| c[0] <= 41), "never behind the spike");
}

#[test]
fn a_crystal_is_shot_from_the_ground_drawn_full_and_released_on_the_aim() {
    let mut w = world(FEET);
    let mut p = Plan::new(Aim::Race);
    let t = p.bow_task(&w, 100);
    assert!(matches!(t, Some(Task::Idle)), "{t:?} {:?}", told(&p));
    assert!(
        notes(&p).iter().any(|n| n == "bow: aim at crystal 7"),
        "{:?}",
        told(&p)
    );
    let aim = p.bow.doing.as_ref().map(|s| s.aim).expect("a shot");
    // The bow is in hand: still, then the use.
    let c = p.bow_step(&w, 101).expect("driven");
    assert_eq!((c.yaw, c.pitch), (aim.yaw, Some(aim.pitch)));
    assert!(!c.forward && !c.jump);
    p.take_acts();
    p.bow_step(&w, 102).expect("driven");
    assert_eq!(p.take_acts(), vec![Act::UseItem], "the draw begins");
    // The server heard the aim's look.
    #[allow(clippy::cast_possible_truncation)]
    {
        w.player.yaw = aim.yaw as f32;
    }
    w.player.pitch = aim.pitch;
    for t in 103..102 + bow::DRAW_HOLD {
        p.bow_step(&w, t).expect("drawing");
        assert!(p.take_acts().is_empty(), "held at {t}");
    }
    p.bow_step(&w, 102 + bow::DRAW_HOLD).expect("released");
    assert_eq!(p.take_acts(), vec![Act::ReleaseUse], "let go at full power");
    assert!(matches!(
        p.bow.doing.as_ref().map(|s| s.stage),
        Some(Stage::Flight { .. })
    ));
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![CRYSTAL] }));
    // Gone from the entities: its blast is waited for.
    assert!(p.bow_step(&w, 135).is_some(), "the blast waited for");
    w.step(&Event::Packet(S2c::Explode {
        center: CRYSTAL_AT,
        knockback: None,
    }));
    assert!(p.bow_step(&w, 137).is_none(), "the shot over");
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "bow: crystal 7 gone by arrow")
    );
    assert_eq!((p.bow.shots, p.bow.hits), (1, 1));
}

#[test]
fn a_draw_cut_by_another_row_is_let_go_and_begun_again() {
    let mut w = world(FEET);
    let mut p = Plan::new(Aim::Race);
    p.bow_task(&w, 100);
    p.bow_step(&w, 101);
    p.bow_step(&w, 102);
    assert_eq!(p.take_acts(), vec![Act::UseItem]);
    // Ticks 103 to 110 another row held the controls.
    w.player.health = Some(20.0);
    p.bow_step(&w, 111).expect("driven");
    assert_eq!(p.take_acts(), vec![Act::ReleaseUse]);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "bow: draw on crystal 7 let go")
    );
    // Begun again from the bow in hand (held still): standing still next.
    assert_eq!(p.bow.doing.as_ref().map(|s| s.stage), Some(Stage::Settle));
}

#[test]
fn a_miss_is_counted_and_four_leave_it_to_the_pillar() {
    let w = world(FEET);
    let mut p = Plan::new(Aim::Race);
    for k in 0..bow::MISSES {
        let t0 = 1000 * u64::from(k + 1);
        p.bow_task(&w, t0);
        let s = p.bow.doing.as_mut().expect("a shot");
        s.stage = Stage::Flight { since: t0 };
        p.bow.driven_at = t0;
        for t in t0 + 1..t0 + 100 {
            if p.bow_step(&w, t).is_none() {
                break;
            }
        }
    }
    assert!(p.bow.refused.contains(&CRYSTAL), "{:?}", notes(&p));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "bow: crystal 7 left to the pillar")
    );
    assert_eq!(p.bow_task(&w, 9000), None, "the pillar's turn");
}

#[test]
fn a_caged_crystal_is_left_to_the_pillar() {
    let w = world(FEET);
    let mut p = Plan::new(Aim::Race);
    p.spikes.geography = vec![Spike {
        centre: [8, 8],
        radius: 2,
        top_y: 70,
        caged: true,
    }];
    assert_eq!(p.bow_task(&w, 100), None);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "bow: crystal 7 caged, left to the pillar")
    );
}

#[test]
fn with_no_shot_from_here_the_bot_walks_to_a_stand() {
    let mut w = world(FEET);
    // A wall of end stone across the way, two high over the feet.
    for x in 0..17 {
        for y in 64..80 {
            set(&mut w, (x, y, -12), BlockKind::EndStone);
        }
    }
    let mut p = Plan::new(Aim::Race);
    let t = p.bow_task(&w, 100);
    assert!(matches!(t, Some(Task::Explore(_)) | None), "{t:?}");
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("bow: to the stand at") || n == "bow: no stand for crystal 7"),
        "{:?}",
        told(&p)
    );
    assert!(p.bow.doing.is_none());
}

#[test]
fn no_bow_no_shot() {
    let mut w = world(FEET);
    w.inventory.slots.remove(&37);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.bow_task(&w, 100), None, "no arrows");
}

/// A crystal that leaves the entities after a shot with no blast at its spot (untracked: the
/// server sends crystals again on a resume) is not a hit.
#[test]
fn a_crystal_untracked_after_a_shot_is_no_hit() {
    let mut w = world(FEET);
    let mut p = Plan::new(Aim::Race);
    p.bow_task(&w, 100);
    let s = p.bow.doing.as_mut().expect("a shot");
    s.stage = Stage::Flight { since: 100 };
    p.bow.driven_at = 100;
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![CRYSTAL] }));
    let mut t = 101;
    while p.bow_step(&w, t).is_some() {
        t += 1;
        assert!(t < 200, "never over");
    }
    assert_eq!(p.bow.hits, 0);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "bow: crystal 7 out of sight after the shot")
    );
}
