//! Issue 83 wall 1 (docs/problems/lava-portal.md): a fresh cast that holds no lava bucket fills
//! one by the refill ladder before its first room op; with no lava a scoop reaches, it fails
//! `NoRefill` at 0 ops, and the planner's fetch and give-up apply as before. A cast resumed at a
//! kept site (`Cast::new`) works its first op as before. On a hand-built world: stone to y 63,
//! air from y 64, a frame at (0, 64, 0) facing south, a pool of four lava sources at x 20 to 21,
//! z 20 to 21 in the stone's top layer.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::build::{Facing, Frame};
use plan::cast::{Cast, CastFailure, CastStatus, LavaFirst, Site};
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

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

const POOL: [(i32, i32, i32); 4] = [(20, 63, 20), (21, 63, 20), (20, 63, 21), (21, 63, 21)];

/// The overworld: stone to y 63, air from y 64; the feet at (18.5, 64, 20.5), two blocks west
/// of the pool; the cast's kit held (water, an empty bucket, flint and steel, a pickaxe, stone).
fn world(pool: bool) -> World {
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
    for x in -3..=3 {
        for z in -3..=3 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    if pool {
        for c in POOL {
            set(&mut w, c, BlockKind::Lava);
        }
    }
    let pos = [18.5, 64.0, 20.5];
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w.inventory.slots.insert(36, (ItemKind::IronPickaxe, 1));
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(39, (ItemKind::FlintAndSteel, 1));
    w.inventory.slots.insert(40, (ItemKind::Cobblestone, 32));
    w.held = 0;
    w
}

fn site() -> Site {
    Site {
        frame: Frame {
            origin: (0, 64, 0),
            facing: Facing::South,
        },
        lava: POOL[0],
        h: 0,
    }
}

#[test]
fn the_pool_is_remembered_exposed() {
    let w = world(true);
    let seen = plan::cast::lava_sources(&w);
    assert!(POOL.iter().all(|c| seen.contains(c)), "{seen:?}");
}

#[test]
fn a_fresh_cast_with_no_lava_bucket_fills_one_first() {
    let mut w = world(true);
    let mut k = Cast::fresh(site(), 0);
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running);
    // The first work is the fill, not the room's first op.
    assert_eq!(k.lava_first, LavaFirst::Filling { since: 0 });
    assert_eq!(k.done_ops, 0);
    assert!(k.take_note().is_none());
    for t in 1..20 {
        let (_, _, status) = k.step(&w, t);
        assert_eq!(status, CastStatus::Running, "tick {t}");
        assert!(
            matches!(k.lava_first, LavaFirst::Filling { .. }),
            "tick {t}"
        );
    }
    // The bucket shows filled: the fill is noted once and the room's ops come next.
    w.inventory.slots.insert(38, (ItemKind::LavaBucket, 1));
    let (_, _, status) = k.step(&w, 40);
    assert_eq!(status, CastStatus::Running);
    assert_eq!(k.lava_first, LavaFirst::Filled { at: 40, ticks: 40 });
    let (what, _) = k.take_note().expect("the lava-first note");
    assert!(what.starts_with("lava first: filled at "), "{what}");
    assert!(what.ends_with("after 40 ticks"), "{what}");
    assert!(k.take_note().is_none());
    assert!(k.uses_text().contains("lava first filled at 40"));
    let _ = k.step(&w, 41);
    assert_eq!(k.lava_first, LavaFirst::Filled { at: 40, ticks: 40 });
}

#[test]
fn a_site_with_no_lava_to_reach_is_given_up_at_0_ops() {
    // No lava seen anywhere: the refill ladder has no source, no station and no tunnel's aim.
    let w = world(false);
    let mut k = Cast::fresh(site(), 0);
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Failed(CastFailure::NoRefill));
    assert_eq!(k.done_ops, 0);
    assert_eq!(k.lava_first, LavaFirst::Failed { at: 0 });
    let (what, why) = k.take_note().expect("the lava-first failed note");
    assert!(
        what.starts_with("lava first failed: NoRefill at 0 ops"),
        "{what}"
    );
    assert!(why.contains("refill:"), "the refill trail: {why}");
}

#[test]
fn a_fresh_cast_holding_lava_works_its_first_op() {
    let mut w = world(true);
    w.inventory.slots.insert(38, (ItemKind::LavaBucket, 1));
    let mut k = Cast::fresh(site(), 0);
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running);
    assert_eq!(k.lava_first, LavaFirst::Held);
    assert!(k.take_note().is_none());
}

#[test]
fn a_resumed_cast_never_fills_first() {
    // `Cast::new`, the cast at a kept site: no lava bucket held and no lava seen, and its first op
    // is worked as before, not a refill that would fail it.
    let w = world(false);
    let mut k = Cast::new(site(), 0);
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running);
    assert_eq!(k.lava_first, LavaFirst::Off);
    assert!(k.take_note().is_none());
}
