//! Issue 83 wall 5 (docs/problems/lava-portal.md): the lava hunt holds its target. Race
//! 46144128199 lane 0 flipped between (23, -7, 35) and (66, -55, 13) 308 times over 26,000
//! ticks: the feet moved between two cells six blocks apart, and at each the other target lay
//! just inside `LAVA_RADIUS`, so the far filters skipped it and took the first again.
//!
//! On a hand-built world: stone to y 63, air from y 64, chunks -4 to 4; lone lava sources in the
//! stone's top layer at P (-48, 63, 0) and Q (48, 63, 0), R (3, 63, 58), and three decoys near
//! the origin the site choice looks at first. No pool holds ten sources, so no site is taken.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::cast::{HuntEnd, hunt_look};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Event, World};

const P: (i32, i32, i32) = (-48, 63, 0);
const Q: (i32, i32, i32) = (48, 63, 0);
const R: (i32, i32, i32) = (3, 63, 58);
const DECOYS: [(i32, i32, i32); 3] = [(0, 63, 5), (1, 63, 5), (0, 63, 6)];
/// Feet cells: at A, Q is 45 off (inside 48) and P 51; at B the other way round (the flip). At C,
/// R (50.0) is nearer than P (51.6). At D, P is 8 off and among the nearest three.
const A: [f64; 3] = [3.5, 64.0, 0.5];
const B: [f64; 3] = [-2.5, 64.0, 0.5];
const C: [f64; 3] = [3.5, 64.0, 8.5];
const D: [f64; 3] = [-39.5, 64.0, 0.5];

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

fn world() -> World {
    let mut sections = vec![section(BlockKind::Stone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    }));
    for x in -4..=4 {
        for z in -4..=4 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    for c in [P, Q, R].into_iter().chain(DECOYS) {
        w.step(&Event::Packet(S2c::BlockUpdate {
            pos: c,
            state: state(BlockKind::Lava),
        }));
    }
    stand(&mut w, A);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

fn stand(w: &mut World, pos: [f64; 3]) {
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
}

#[test]
fn the_sources_are_remembered_exposed() {
    let w = world();
    let seen = plan::cast::lava_within(&w, plan::cast::FAR_RADIUS);
    for c in [P, Q, R].into_iter().chain(DECOYS) {
        assert!(seen.contains(&c), "{c:?} not in {seen:?}");
    }
}

/// The cause, as read: with no target held, the fresh choice flips with the feet.
#[test]
fn with_no_target_held_the_choice_flips_with_the_feet() {
    let mut w = world();
    assert_eq!(hunt_look(&w, &[], None).target, Some(P));
    stand(&mut w, B);
    assert_eq!(hunt_look(&w, &[], None).target, Some(Q));
    stand(&mut w, A);
    assert_eq!(hunt_look(&w, &[], None).target, Some(P));
}

/// The fix: the target chosen at A is held at B, and its stalls count across the moves, so the
/// stall rule gives it up (`LAVA_AIM_STALLS`, 4) in a handful of looks, not never.
#[test]
fn a_held_target_holds_across_the_flip_and_its_stalls_add_up() {
    let mut w = world();
    let first = hunt_look(&w, &[], None);
    assert_eq!((first.target, first.fresh), (Some(P), true));
    let mut kept = Vec::new();
    let mut stalls = 0;
    for (i, feet) in [B, A, B, A, B, A].into_iter().enumerate() {
        stand(&mut w, feet);
        let look = hunt_look(&w, &[], Some(P));
        assert_eq!(look.target, Some(P), "look {i}");
        assert!(!look.fresh && look.ended.is_none(), "look {i}: {look:?}");
        let f = path_feet(feet);
        let d = f64::from((P.0 - f.0).pow(2) + (P.1 - f.1).pow(2) + (P.2 - f.2).pow(2)).sqrt();
        stalls = plan::landing_stalls(&mut kept, P, d).1;
    }
    assert!(stalls >= 4, "stalls {stalls}");
}

fn path_feet(p: [f64; 3]) -> (i32, i32, i32) {
    (
        p[0].floor() as i32,
        p[1].floor() as i32,
        p[2].floor() as i32,
    )
}

/// A fresher target nearer the feet is passed over, and named.
#[test]
fn a_nearer_target_is_passed_over_and_named() {
    let mut w = world();
    stand(&mut w, C);
    assert_eq!(hunt_look(&w, &[], None).target, Some(R), "fresh at C");
    let look = hunt_look(&w, &[], Some(P));
    assert_eq!(look.target, Some(P));
    assert_eq!(look.passed, Some(R));
}

/// The held target ends when the site choice looks at it (reached), when a site given up lies
/// beside it (refused), or when memory no longer holds it (gone); the next is taken in the look.
#[test]
fn a_held_target_ends_reached_refused_or_gone() {
    let mut w = world();
    stand(&mut w, D);
    let look = hunt_look(&w, &[], Some(P));
    assert_eq!(look.ended, Some((P, HuntEnd::Reached)));
    assert_eq!((look.target, look.fresh), (Some(R), true));

    stand(&mut w, A);
    let look = hunt_look(&w, &[(-40, 64, 0)], Some(P));
    assert_eq!(look.ended, Some((P, HuntEnd::Refused)));
    assert_eq!(look.target, Some(R));

    w.memory.forget_source("minecraft:overworld", P);
    let look = hunt_look(&w, &[], Some(P));
    assert_eq!(look.ended, Some((P, HuntEnd::Gone)));
    assert_eq!(look.target, Some(R));
}

/// Through the planner: with the portal kit held and no site, the first look chooses P and
/// notes it held; Q is never chosen while P holds.
#[test]
fn the_planner_notes_its_held_target() {
    let mut w = world();
    w.inventory.slots.insert(36, (ItemKind::IronPickaxe, 1));
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(39, (ItemKind::FlintAndSteel, 1));
    w.inventory.slots.insert(40, (ItemKind::Cobblestone, 32));
    w.held = 0;
    let mut p = plan::Plan::new(plan::Aim::Portal);
    let _ = p.step(&w);
    let _ = p.step(&w);
    let targets: Vec<String> = p
        .events_since(0)
        .iter()
        .filter(|e| e.what.starts_with("lava target "))
        .map(|e| e.what.clone())
        .collect();
    assert_eq!(targets, [format!("lava target {P:?}")], "{targets:?}");
    assert_eq!(p.lava_hold.map(|h| h.target), Some(P));
}
