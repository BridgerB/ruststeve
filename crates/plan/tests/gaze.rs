//! Issue 20 (agent E7, docs/problems/end-gaze.md): no look the planner sends comes inside an
//! enderman's stare cone, nor within the guard round it (`look::GUARD` tolerances), for the look
//! of every End act: a strike at a crystal, a dragon look, a dodge's heading, a walk, a look
//! down from a pillar, a clutch's bucket while falling, an eat. On hand-built End worlds.

use std::sync::Arc;

use data::{BlockKind, EntityKind};
use physics::Controls;
use plan::look::{self, ENDERMAN_EYE, GUARD, Kept};
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

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

/// A world in `dimension`, end stone in the lowest 4 sections (its top at y 63), air above,
/// chunk columns -2 to 2 each way; the feet at `pos`.
fn world_in(dimension: &str, pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::EndStone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 2,
        dimension: dimension.to_owned(),
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
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

fn end_world(pos: [f64; 3]) -> World {
    world_in(plan::end::END, pos)
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

fn enderman(w: &mut World, id: i32, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: EntityKind::Enderman.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
}

fn feet(w: &World) -> [f64; 3] {
    w.body.map(|b| b.pos).unwrap_or_default()
}

fn eye_of(w: &World) -> [f64; 3] {
    let f = feet(w);
    [f[0], f[1] + physics::EYE_HEIGHT, f[2]]
}

fn eyes_of(w: &World) -> Vec<[f64; 3]> {
    let b = w.body.unwrap_or_default();
    look::eyes(b.pos, b.vel)
}

/// The look at a point from the standing eye, as the acts aim it.
fn aim(w: &World, to: [f64; 3]) -> (f64, f32) {
    plan::walk::look_at(eye_of(w), to)
}

fn controls(yaw: f64, pitch: Option<f32>) -> Controls {
    Controls {
        yaw,
        pitch,
        ..Controls::default()
    }
}

/// The closest the sent look comes to any enderman's eyes, in stare tolerances, from every eye
/// the server may judge it from (no line test: the race's own count).
fn closest(w: &World, c: &Controls) -> f64 {
    #[allow(clippy::cast_possible_truncation)]
    let yaw = c.yaw as f32;
    let pitch = c.pitch.unwrap_or(w.player.pitch);
    w.entities
        .values()
        .filter(|e| e.kind == EntityKind::Enderman)
        .flat_map(|e| {
            eyes_of(w)
                .into_iter()
                .filter_map(move |eye| look::tolerances(eye, yaw, pitch, e.pos).map(|(k, _)| k))
        })
        .fold(f64::INFINITY, f64::min)
}

/// The act's look kept: outside the guard from every eye, and no stare by the jar's test.
fn assert_kept(w: &World, asked: Controls, what: &str) -> Controls {
    let (c, why) = look::keep_off_why(w, asked, None);
    assert!(
        why.is_some(),
        "{what}: the asked look should have been kept"
    );
    assert!(
        closest(w, &c) >= GUARD,
        "{what}: sent {:?} comes {:.2} tolerances off",
        c.pitch,
        closest(w, &c)
    );
    #[allow(clippy::cast_possible_truncation)]
    let none = eyes_of(w)
        .iter()
        .all(|e| look::stares(w, *e, c.yaw as f32, c.pitch.unwrap_or(0.0)).is_none());
    assert!(none, "{what}: a stare left");
    assert!(
        (c.yaw - asked.yaw).abs() < f64::EPSILON,
        "{what}: the heading is the yaw"
    );
    c
}

/// A crystal strike: the look at a crystal 3 off, an enderman standing right behind it on the
/// line 20 off.
#[test]
fn a_strike_at_a_crystal_with_an_enderman_behind_it_is_kept_off() {
    let mut w = end_world([0.5, 64.0, 0.5]);
    enderman(&mut w, 41, [20.5, 64.0 + 1.0, 0.5]);
    // The crystal's middle lies on the line from the eye to the enderman's eyes.
    let e = eye_of(&w);
    let target = [20.5, 64.0 + 1.0 + ENDERMAN_EYE, 0.5];
    let crystal: Vec<f64> = (0..3).map(|i| e[i] + (target[i] - e[i]) * 0.15).collect();
    let (yaw, pitch) = aim(&w, [crystal[0], crystal[1], crystal[2]]);
    assert_kept(&w, controls(yaw, Some(pitch)), "strike");
}

/// A dragon look: at the dragon's head over the fountain, an enderman's eyes 2 degrees off the
/// line at 40.
#[test]
fn a_look_at_the_dragon_past_an_endermans_eyes_is_kept_off() {
    let mut w = end_world([0.5, 64.0, 0.5]);
    enderman(&mut w, 42, [40.5, 64.0, 1.9]);
    let (yaw, pitch) = aim(&w, [80.5, 64.0 + ENDERMAN_EYE * 2.0, 0.5]);
    let c = assert_kept(&w, controls(yaw, Some(pitch)), "dragon look");
    // The nearest clear pitch, not a jump to straight down.
    assert!(c.pitch.unwrap() < 60.0, "{c:?}");
}

/// A dodge or a walk: the heading straight at an enderman, the pitch level (the server's held
/// one: no pitch asked for).
#[test]
fn a_dodge_heading_at_an_enderman_is_kept_off() {
    let mut w = end_world([0.5, 64.0, 0.5]);
    enderman(&mut w, 43, [12.5, 63.0, 0.5]);
    let (yaw, pitch) = aim(&w, [12.5, 63.0 + ENDERMAN_EYE, 0.5]);
    w.player.pitch = pitch;
    let mut c = controls(yaw, None);
    c.forward = true;
    let k = assert_kept(&w, c, "dodge");
    assert!(k.forward, "the dodge goes on");
}

/// A pillar top 20 up: the look straight down at the pillar finds an enderman at its foot. The
/// jar's line is stopped by the pillar's own blocks, but in the End no line is trusted.
#[test]
fn a_look_down_from_a_pillar_over_an_enderman_is_bent_a_little() {
    let mut w = end_world([0.5, 84.0, 0.5]);
    for y in 64..84 {
        set(&mut w, (0, y, 0), BlockKind::EndStone);
    }
    enderman(&mut w, 44, [1.2, 64.0, 0.8]);
    let c = assert_kept(&w, controls(0.0, Some(90.0)), "pillar look down");
    let p = c.pitch.unwrap();
    assert!(p > 45.0, "still near the feet: {p}");
}

/// A clutch drop: falling 2 a tick past an enderman on a ledge 15 off, its eyes level with the
/// eye after the tick's move: the look along the level meets them only from there (from the eye
/// now they are 7.6 degrees under it, outside the cone's 3.3 and the guard's 6.6).
#[test]
fn a_clutch_look_while_falling_is_judged_from_the_eyes_after_the_move() {
    let mut w = end_world([0.5, 90.0, 0.5]);
    if let Some(b) = w.body.as_mut() {
        b.vel = [0.0, -2.0, 0.0];
        b.on_ground = false;
    }
    // From the feet 2 lower, the look at pitch p meets its eyes.
    let next_eye = [0.5, 88.0 + physics::EYE_HEIGHT, 0.5];
    let at = [15.5, 88.0 + physics::EYE_HEIGHT - ENDERMAN_EYE, 0.5];
    let (_, p) = plan::walk::look_at(next_eye, [at[0], at[1] + ENDERMAN_EYE, at[2]]);
    let (yaw, _) = plan::walk::look_at(next_eye, [at[0], next_eye[1], at[2]]);
    enderman(&mut w, 45, at);
    #[allow(clippy::cast_possible_truncation)]
    let y = yaw as f32;
    // The look is outside the cone from the feet now: the old filter let it go.
    assert_eq!(look::stares(&w, eye_of(&w), y, p), None);
    let (c, why) = look::keep_off_why(&w, controls(yaw, Some(p)), None);
    assert_eq!(why, Some((45, Kept::Guard)));
    assert!(closest(&w, &c) >= GUARD, "{c:?}");
}

/// An eat or any act that asks for no look: the server's held pitch is level at an enderman's
/// eyes 30 off, so the look is set, not left.
#[test]
fn an_eat_with_the_held_look_on_an_enderman_is_kept_off() {
    let mut w = end_world([0.5, 64.0, 0.5]);
    enderman(&mut w, 46, [0.5, 64.0 - 1.0, 30.5]);
    let (yaw, pitch) = aim(&w, [0.5, 63.0 + ENDERMAN_EYE, 30.5]);
    w.player.pitch = pitch;
    assert_kept(&w, controls(yaw, None), "eat");
}

/// Outside the End a seen full block still stops the line (a cave's wall), so a look is not bent
/// for an enderman behind it; in the End it is.
#[test]
fn a_seen_wall_stops_the_line_outside_the_end_only() {
    for (dim, bent) in [("minecraft:overworld", false), (plan::end::END, true)] {
        let mut w = world_in(dim, [0.5, 64.0, 0.5]);
        for y in 64..68 {
            for z in -2..=2 {
                set(&mut w, (4, y, z), BlockKind::Stone);
            }
        }
        // The wall's face toward the bot is seen (air beside it).
        let at = [10.5, 64.0, 0.5];
        enderman(&mut w, 47, at);
        let (yaw, pitch) = aim(&w, [at[0], at[1] + ENDERMAN_EYE, at[2]]);
        let (_, why) = look::keep_off_why(&w, controls(yaw, Some(pitch)), None);
        assert_eq!(why.is_some(), bent, "{dim}");
    }
}

/// Every look, from every placement: endermen on a ring round a bot on the island and below a
/// pillar, the asked look every 15 degrees of yaw and every 10 of pitch. What `Plan::step`'s
/// last filter sends never comes within the guard of any of them.
#[test]
fn no_look_in_a_sweep_comes_within_the_guard() {
    for (feet, ring_y) in [([0.5, 64.0, 0.5], 64.0), ([0.5, 90.0, 0.5], 64.0)] {
        for r in [3.0f64, 8.0, 20.0, 50.0] {
            let mut w = end_world(feet);
            for i in 0..6 {
                let a = f64::from(i) * 60.0_f64.to_radians() + r / 10.0;
                enderman(
                    &mut w,
                    100 + i,
                    [feet[0] + r * a.cos(), ring_y, feet[2] + r * a.sin()],
                );
            }
            for yi in 0..24 {
                for pi in 0..=18 {
                    let yaw = f64::from(yi) * 15.0;
                    #[allow(clippy::cast_precision_loss)]
                    let pitch = -90.0 + pi as f32 * 10.0;
                    let (c, _) = look::keep_off_why(&w, controls(yaw, Some(pitch)), None);
                    let k = closest(&w, &c);
                    assert!(
                        k >= GUARD,
                        "feet {feet:?} ring {r}: asked {yaw},{pitch} sent {:?} at {k:.2}",
                        c.pitch
                    );
                }
            }
        }
    }
}

fn table() -> Arc<make::Table> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")))
}

/// The whole planner in the End, an enderman under the pillar the bot stands on: the look it
/// sends (straight down by default) is bent off it, and the bend is noted.
#[test]
fn the_planner_in_the_end_never_sends_a_look_within_the_guard() {
    let mut w = end_world([0.5, 84.0, 0.5]);
    for y in 64..84 {
        set(&mut w, (0, y, 0), BlockKind::EndStone);
    }
    enderman(&mut w, 48, [1.0, 64.0, 0.9]);
    let mut p = Plan::with_table(Aim::Race, table());
    for t in 0..40 {
        w.ticks = 1000 + t;
        w.session.server_ticks = 1000 + t;
        let c = p.step(&w);
        assert!(closest(&w, &c) >= GUARD, "tick {t}: {c:?}");
    }
    assert!(
        p.events
            .iter()
            .any(|e| e.what.starts_with("look kept off enderman 48")),
        "{:?}",
        p.events.iter().map(|e| e.what.clone()).collect::<Vec<_>>()
    );
}
