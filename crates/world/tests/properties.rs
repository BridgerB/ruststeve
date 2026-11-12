//! Issue 04's second done-condition: folding any sequence of events never panics, no entity is in
//! two places, no count is negative. Plus the memory's index stays a fold of its log, and the
//! no-X-ray rule holds on a buried log.

use std::sync::Arc;

use data::BlockKind;
use proptest::prelude::*;
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use protocol::metadata::MetaValue;
use protocol::slot::Slot;
use world::{BlockPos, Event, Inventory, World};

fn state(kind: BlockKind) -> u32 {
    kind.info().map(|i| i.default_state).unwrap()
}

/// An overworld column of solid stone (24 sections from y -64).
fn stone_column() -> Column {
    let section = Section {
        block_count: 4096,
        fluid_count: 0,
        blocks: Paletted::Single(state(BlockKind::Stone)),
        biomes: Paletted::Single(0),
    };
    Column {
        min_y: -64,
        sections: vec![section; 24],
    }
}

/// A fixture column from issue 04a's oracle test (a real overworld chunk).
fn fixture_column() -> Column {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");
    let path = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("overworld_"))
        })
        .unwrap();
    Column::decode(&std::fs::read(path).unwrap()).unwrap()
}

fn login() -> Event {
    Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    })
}

fn slots() -> impl Strategy<Value = Slot> {
    prop_oneof![
        Just(Slot::Empty),
        (0u32..1600, -3i32..100).prop_map(|(item, count)| Slot::Item { item, count }),
        (0u32..1600, -3i32..100).prop_map(|(item, count)| Slot::Truncated { item, count }),
    ]
}

/// Any f64, NaN and infinities included: the world must never hold one.
fn coord() -> impl Strategy<Value = f64> {
    prop_oneof![-1.0e4f64..1.0e4, any::<f64>()]
}

fn events(fixture: Arc<Column>) -> impl Strategy<Value = Event> {
    // Entity ids from a small range so adds, moves and removes collide; 1 is the player's own.
    let id = 0i32..8;
    prop_oneof![
        (id.clone(), 0u32..160, coord(), coord(), coord()).prop_map(|(id, kind, x, y, z)| {
            Event::Packet(S2c::AddEntity {
                id,
                uuid: [0; 16],
                kind,
                x,
                y,
                z,
            })
        }),
        proptest::collection::vec(id.clone(), 0..4)
            .prop_map(|ids| Event::Packet(S2c::RemoveEntities { ids })),
        (id.clone(), -8.0f64..8.0, -8.0f64..8.0, -8.0f64..8.0)
            .prop_map(|(id, dx, dy, dz)| Event::Packet(S2c::MoveEntity { id, dx, dy, dz })),
        (id.clone(), coord(), coord(), coord())
            .prop_map(|(id, x, y, z)| Event::Packet(S2c::EntityPositionSync { id, x, y, z })),
        (id.clone(), coord(), coord(), coord(), 0u32..32).prop_map(|(id, x, y, z, flags)| {
            Event::Packet(S2c::TeleportEntity { id, x, y, z, flags })
        }),
        (id.clone(), any::<f32>()).prop_map(|(id, h)| Event::Packet(S2c::SetEntityData {
            id,
            entries: vec![(9, MetaValue::Float(h)), (0, MetaValue::Byte(1))],
        })),
        (coord(), coord(), coord(), 0u32..32).prop_map(|(x, y, z, flags)| {
            Event::Packet(S2c::PlayerPosition {
                teleport_id: 0,
                x,
                y,
                z,
                yaw: 0.0,
                pitch: 0.0,
                flags,
            })
        }),
        (
            proptest::collection::vec(slots(), 0..46),
            proptest::option::of(slots())
        )
            .prop_map(|(slots, carried)| Event::Packet(S2c::ContainerSetContent {
                window: 0,
                state_id: 1,
                slots,
                carried,
            })),
        (-2i16..50, slots()).prop_map(|(slot, item)| Event::Packet(S2c::ContainerSetSlot {
            window: 0,
            state_id: 2,
            slot,
            item,
        })),
        (-2i32..45, slots())
            .prop_map(|(slot, item)| Event::Packet(S2c::SetPlayerInventory { slot, item })),
        (-2i32..3, -2i32..3).prop_map(move |(x, z)| Event::Packet(S2c::LevelChunkWithLight {
            x,
            z,
            column: Arc::clone(&fixture),
        })),
        (-32i32..48, -64i32..120, -32i32..48, 0u32..30_000).prop_map(|(x, y, z, s)| Event::Packet(
            S2c::BlockUpdate {
                pos: (x, y, z),
                state: s
            }
        )),
        (-2i32..3, -2i32..3).prop_map(|(x, z)| Event::Packet(S2c::ForgetLevelChunk { x, z })),
    ]
}

proptest! {
    #[test]
    fn folding_any_events_keeps_the_world_sound(seq in proptest::collection::vec(events(Arc::new(fixture_column())), 0..120)) {
        let mut w = World::new();
        w.step(&login());
        for e in &seq {
            w.step(e);
        }
        // No entity in two places: one entry per id, and never the player's own id.
        prop_assert!(!w.entities.contains_key(&1));
        // Every position is a real place.
        for e in w.entities.values() {
            prop_assert!(e.pos.iter().all(|v| v.is_finite()), "{:?}", e.pos);
        }
        if let Some(p) = w.player.pos {
            prop_assert!(p.iter().all(|v| v.is_finite()), "{:?}", p);
        }
        // No count is negative (an empty slot is absent, a held one counts at least 1).
        for (item, n) in w.inventory.slots.values() {
            prop_assert!(*n >= 1, "{:?} x{}", item, n);
        }
        if let Some((_, n)) = w.inventory.carried {
            prop_assert!(n >= 1);
        }
        // Memory's index is the fold of its log.
        prop_assert_eq!(&w.memory.refold_latest(), w.memory.latest_index());
    }
}

#[test]
fn a_buried_log_is_seen_only_once_uncovered() {
    let mut w = World::new();
    w.step(&login());
    w.step(&Event::Packet(S2c::LevelChunkWithLight {
        x: 0,
        z: 0,
        column: Arc::new(stone_column()),
    }));
    let log = (5, 10, 5);
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: log,
        state: state(BlockKind::OakLog),
    }));
    assert!(w.memory.log().is_empty(), "a log inside stone is X-ray");
    // Dig the stone above it: the log now has an air face.
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (5, 11, 5),
        state: state(BlockKind::Air),
    }));
    let seen = w.memory.latest("minecraft:overworld", log).map(|s| s.state);
    assert_eq!(seen, Some(state(BlockKind::OakLog)));
    // Dig the log: a newer fact of air at the same place.
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: log,
        state: state(BlockKind::Air),
    }));
    let now = w.memory.latest("minecraft:overworld", log).map(|s| s.state);
    assert_eq!(now, Some(state(BlockKind::Air)));
    assert_eq!(w.memory.log().len(), 2);
    assert_eq!(w.block_at(BlockPos::new(5, 10, 5)), Some(BlockKind::Air));
}

#[test]
fn inventory_indexes_map_to_menu_slots() {
    assert_eq!(Inventory::menu_slot(0), Some(36));
    assert_eq!(Inventory::menu_slot(8), Some(44));
    assert_eq!(Inventory::menu_slot(9), Some(9));
    assert_eq!(Inventory::menu_slot(36), Some(8));
    assert_eq!(Inventory::menu_slot(39), Some(5));
    assert_eq!(Inventory::menu_slot(40), Some(45));
    assert_eq!(Inventory::menu_slot(41), None);
}
