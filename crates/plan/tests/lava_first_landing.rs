//! Issue 83 wall 7 (docs/problems/lava-portal.md): lava first (wall 1) filled from where the
//! cast began, before the landing and the room existed, and gave sites up the room-first order
//! would have filled (fix arm race 46160462536: 5 failures in 4 bots against 2 fills). It now
//! waits for the landing's ops, walks onto the landing, fills from there with the cheap rungs
//! under `LAVA_FIRST_TICKS`, and a failed fill sends the cast on room first instead of failing it.
//!
//! Hand-built: stone to y 63, air from y 64; a frame at (0, 64, 0) facing south, h 0, so its
//! landing is (2, 64, 2); a pool of six lava sources at x 1 to 3, z 5 and 6, in the stone's top
//! layer, three blocks in front of the landing. The cast begins with the feet at (-44.5, 64,
//! -24.5), 55 blocks from the pool: beyond `LAVA_RADIUS`, so from there no source is in play.

use std::sync::Arc;

use data::{BlockKind, ItemKind};
use plan::build::{Facing, Frame};
use plan::cast::{Cast, CastStatus, LAVA_FIRST_TICKS, LavaFirst, Site};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use world::{Act, Event, World};

type Cell = (i32, i32, i32);

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

fn set(w: &mut World, c: Cell, k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

const LANDING: Cell = (2, 64, 2);
const START: [f64; 3] = [-44.5, 64.0, -24.5];
/// The empty bucket's inventory slot (hotbar 2).
const BUCKET: i16 = 38;

fn pool() -> Vec<Cell> {
    (1..=3).flat_map(|x| [(x, 63, 5), (x, 63, 6)]).collect()
}

fn site() -> Site {
    Site {
        frame: Frame {
            origin: (0, 64, 0),
            facing: Facing::South,
        },
        lava: (2, 63, 5),
        h: 0,
    }
}

/// The feet put at `pos`, on the ground (the walk's arrival, by hand).
fn stand_at(w: &mut World, pos: [f64; 3]) {
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
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
    for x in -3..=3 {
        for z in -3..=3 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    for c in pool() {
        set(&mut w, c, BlockKind::Lava);
    }
    stand_at(&mut w, START);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w.inventory.slots.insert(36, (ItemKind::IronPickaxe, 1));
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(BUCKET, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(39, (ItemKind::FlintAndSteel, 1));
    w.inventory.slots.insert(40, (ItemKind::Cobblestone, 32));
    w.held = 0;
    w
}

fn landing_pos() -> [f64; 3] {
    [
        f64::from(LANDING.0) + 0.5,
        f64::from(LANDING.1),
        f64::from(LANDING.2) + 0.5,
    ]
}

#[test]
fn the_pool_is_out_of_play_from_the_start_and_in_reach_from_the_landing() {
    let mut w = world();
    assert!(plan::cast::lava_sources(&w).is_empty());
    stand_at(&mut w, landing_pos());
    let seen = plan::cast::lava_sources(&w);
    assert!(pool().iter().all(|c| seen.contains(c)), "{seen:?}");
}

#[test]
fn from_the_cast_s_start_the_fill_fails_and_the_cast_goes_room_first() {
    // Wall 1's fill, from where the cast began: no source in play, the ladder fails at once. It
    // no longer fails the cast.
    let w = world();
    let mut k = Cast::fresh(site(), 0);
    k.lava_first = LavaFirst::Filling { since: 0 };
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running, "{}", k.refill_text());
    assert_eq!(k.lava_first, LavaFirst::Failed { at: 0 });
    let (what, why) = k.take_note().expect("the room-first note");
    assert_eq!(what, "lava first failed: room first (0 ticks, NoRefill)");
    assert!(why.contains("refill:"), "the refill trail: {why}");
    assert!(k.uses_text().contains("room first"));
    // Room first: the plan's ops are worked, and lava first never comes back.
    for t in 1..10 {
        let (_, _, status) = k.step(&w, t);
        assert_eq!(status, CastStatus::Running, "tick {t}");
        assert_eq!(k.lava_first, LavaFirst::Failed { at: 0 });
    }
    assert!(k.current().is_some());
}

#[test]
fn from_the_landing_the_fill_succeeds() {
    let mut w = world();
    let mut k = Cast::fresh(site(), 0);
    // The landing's cells are open (its ops done): the cast walks onto it, no fill yet.
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running);
    assert_eq!(k.lava_first, LavaFirst::ToLanding { since: 0 });
    assert!(k.take_note().is_none());
    stand_at(&mut w, landing_pos());
    let mut notes = Vec::new();
    for t in 1..200 {
        let (_, act, status) = k.step(&w, t);
        assert_eq!(status, CastStatus::Running, "tick {t}: {}", k.refill_text());
        notes.extend(k.take_note());
        if matches!(k.lava_first, LavaFirst::Filled { .. }) {
            break;
        }
        assert!(
            !matches!(k.lava_first, LavaFirst::Failed { .. }),
            "tick {t}: {notes:?} {}",
            k.refill_text()
        );
        // The server's side, by hand: the slot held, a dug block gone, the bucket filled.
        match act {
            Some(Act::Hold { slot }) => w.held = slot,
            Some(Act::Dig { pos, .. }) => set(&mut w, (pos.x, pos.y, pos.z), BlockKind::Air),
            Some(Act::UseItem) => {
                w.inventory.slots.insert(BUCKET, (ItemKind::LavaBucket, 1));
            }
            _ => {}
        }
    }
    assert!(
        matches!(k.lava_first, LavaFirst::Filled { .. }),
        "{:?}: {}",
        k.lava_first,
        k.refill_text()
    );
    assert_eq!(k.done_ops, 0);
    let (what, _) = notes
        .iter()
        .find(|(what, _)| what.starts_with("lava first: filled at "))
        .expect("the filled note");
    assert!(what.contains(&format!("{LANDING:?}")), "{what}");
    assert!(!k.refill_text().contains("tunnel"), "{}", k.refill_text());
}

#[test]
fn a_fill_that_never_comes_goes_room_first_within_its_budget() {
    // At the landing, the pool in sight, but the bucket never shows filled: the scoops fail or
    // the budget runs out, and the cast goes on room first, never failed.
    let mut w = world();
    stand_at(&mut w, landing_pos());
    let mut k = Cast::fresh(site(), 0);
    let mut notes = Vec::new();
    for t in 0..=LAVA_FIRST_TICKS + 2 {
        let (_, act, status) = k.step(&w, t);
        assert_eq!(status, CastStatus::Running, "tick {t}: {}", k.refill_text());
        notes.extend(k.take_note());
        if let Some(Act::Hold { slot }) = act {
            w.held = slot;
        }
        if matches!(k.lava_first, LavaFirst::Failed { .. }) {
            break;
        }
    }
    assert!(
        matches!(k.lava_first, LavaFirst::Failed { at } if at <= LAVA_FIRST_TICKS + 1),
        "{:?}",
        k.lava_first
    );
    assert!(
        notes
            .iter()
            .any(|(what, _)| what.starts_with("lava first failed: room first (")),
        "{notes:?}"
    );
}

#[test]
fn the_fill_waits_for_the_landing_s_ops() {
    // The landing still rock: its digs are worked first, lava first stays armed.
    let mut w = world();
    for y in 64..=66 {
        set(&mut w, (LANDING.0, y, LANDING.2), BlockKind::Stone);
    }
    // Beside the landing's column, at its height.
    stand_at(&mut w, [3.5, 64.0, 2.5]);
    let mut k = Cast::fresh(site(), 0);
    let (_, _, status) = k.step(&w, 0);
    assert_eq!(status, CastStatus::Running);
    assert_eq!(k.lava_first, LavaFirst::Armed);
    assert_eq!(
        k.current().map(|o| o.step),
        Some(plan::build::Step::Landing)
    );
}
