//! Issue 05's third done-condition: every recorded type survives MessagePack and back unchanged,
//! a whole file written then read gives the same records, and replay refuses another commit's
//! recording unless forced.

use std::sync::Arc;

use data::ItemKind;
use proptest::prelude::*;
use protocol::chunk::Column;
use protocol::metadata::MetaValue;
use protocol::slot::Slot;
use protocol::{S2c, State};
use record::{EffectRecord, EventRecord, Header, Record, Recorder, Recording, check_commit};
use world::{Event, World};

fn round_trip<T>(v: &T) -> T
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let bytes = rmp_serde::to_vec_named(v).unwrap();
    rmp_serde::from_slice(&bytes).unwrap()
}

fn states() -> impl Strategy<Value = State> {
    prop_oneof![
        Just(State::Handshake),
        Just(State::Login),
        Just(State::Configuration),
        Just(State::Play),
    ]
}

fn event_records() -> impl Strategy<Value = EventRecord> {
    prop_oneof![
        (
            states(),
            any::<u32>(),
            proptest::collection::vec(any::<u8>(), 0..300)
        )
            .prop_map(|(state, id, body)| EventRecord::Packet { state, id, body }),
        (any::<u64>(), any::<u64>()).prop_map(|(now_ms, seed)| EventRecord::Tick { now_ms, seed }),
    ]
}

fn effect_records() -> impl Strategy<Value = EffectRecord> {
    prop_oneof![
        (any::<u32>(), proptest::collection::vec(any::<u8>(), 0..100))
            .prop_map(|(id, body)| EffectRecord::Send { id, body }),
        any::<i32>().prop_map(EffectRecord::SetCompression),
    ]
}

fn headers() -> impl Strategy<Value = Header> {
    (
        ".{0,40}",
        any::<bool>(),
        ".{0,12}",
        any::<u64>(),
        proptest::option::of(".{0,20}"),
    )
        .prop_map(|(commit, dirty, name, seed, previous)| {
            let mut h = Header::new(&name, "127.0.0.1:25565", seed);
            h.commit = commit;
            h.dirty = dirty;
            h.previous_segment = previous;
            h
        })
}

fn fixture_column() -> Arc<Column> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../world/tests/fixtures");
    let path = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("overworld_"))
        })
        .unwrap();
    Arc::new(Column::decode(&std::fs::read(path).unwrap()).unwrap())
}

/// Events that fill every part of the world: chunks, blocks (and so sightings), entities with
/// metadata, the inventory, the player.
fn world_events(column: Arc<Column>) -> impl Strategy<Value = Event> {
    let id = 2i32..8;
    prop_oneof![
        (-2i32..3, -2i32..3).prop_map(move |(x, z)| Event::Packet(S2c::LevelChunkWithLight {
            x,
            z,
            column: Arc::clone(&column)
        })),
        (-32i32..48, -64i32..120, -32i32..48, 0u32..30_000).prop_map(|(x, y, z, s)| Event::Packet(
            S2c::BlockUpdate {
                pos: (x, y, z),
                state: s
            }
        )),
        (
            id.clone(),
            0u32..160,
            -100.0f64..100.0,
            60.0f64..90.0,
            -100.0f64..100.0
        )
            .prop_map(|(id, kind, x, y, z)| Event::Packet(S2c::AddEntity {
                id,
                uuid: [7; 16],
                kind,
                x,
                y,
                z
            })),
        (id, any::<f32>(), any::<u8>()).prop_map(|(id, h, f)| Event::Packet(S2c::SetEntityData {
            id,
            entries: vec![
                (0, MetaValue::Byte(f)),
                (9, MetaValue::Float(h)),
                (8, MetaValue::Item(Slot::Item { item: 3, count: 2 }))
            ],
        })),
        (0i32..46, 0u32..1500, 1i32..65).prop_map(|(slot, item, count)| Event::Packet(
            S2c::SetPlayerInventory {
                slot,
                item: Slot::Item { item, count }
            }
        )),
        (-50.0f64..50.0, 60.0f64..90.0, -50.0f64..50.0).prop_map(|(x, y, z)| Event::Packet(
            S2c::PlayerPosition {
                teleport_id: 1,
                x,
                y,
                z,
                yaw: 1.5,
                pitch: -0.5,
                flags: 0
            }
        )),
        (any::<u64>(), any::<u64>()).prop_map(|(now_ms, seed)| Event::Tick { now_ms, seed }),
    ]
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

proptest! {
    #[test]
    fn event_records_round_trip(e in event_records()) {
        prop_assert_eq!(round_trip(&e), e.clone());
        let r = Record::Event(e);
        prop_assert_eq!(round_trip(&r), r);
    }

    #[test]
    fn effect_records_round_trip(e in proptest::collection::vec(effect_records(), 0..8)) {
        let r = Record::Effects(e);
        prop_assert_eq!(round_trip(&r), r);
    }

    #[test]
    fn headers_round_trip(h in headers()) {
        prop_assert_eq!(round_trip(&h), h.clone());
        let r = Record::Header(h);
        prop_assert_eq!(round_trip(&r), r);
    }

    #[test]
    fn worlds_round_trip(seq in proptest::collection::vec(world_events(fixture_column()), 0..60)) {
        let mut w = World::new();
        w.step(&login());
        for e in &seq {
            w.step(e);
        }
        let back: World = round_trip(&w);
        prop_assert!(back == w, "a world changed in a round trip");
        let r = Record::Checkpoint { event_index: 9, server_ticks: 1200, world: Box::new(w) };
        prop_assert_eq!(round_trip(&r), r);
    }
}

#[test]
fn kinds_round_trip_by_id_even_unknown() {
    for k in [ItemKind::OakLog, ItemKind::Unknown(99_999)] {
        assert_eq!(round_trip(&k), k);
    }
}

#[test]
fn a_file_reads_back_as_written() {
    let path = std::env::temp_dir().join(format!("ruststeve-roundtrip-{}.rsr", std::process::id()));
    let header = Header::new("rs-test", "127.0.0.1:25565", 42);
    let mut w = World::new();
    w.step(&login());
    let records = vec![
        Record::Event(EventRecord::Tick { now_ms: 1, seed: 2 }),
        Record::Effects(vec![EffectRecord::SetCompression(256)]),
        Record::Checkpoint {
            event_index: 1,
            server_ticks: 0,
            world: Box::new(w),
        },
        Record::Note("a note".to_owned()),
    ];
    let mut r = Recorder::create(&path, &header).unwrap();
    for rec in &records {
        r.write(rec).unwrap();
    }
    r.finish().unwrap();
    let (h, back) = Recording::read_all(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(h, header);
    assert_eq!(back, records);
}

#[test]
fn another_commits_recording_is_refused_unless_forced() {
    let mut h = Header::new("rs-test", "127.0.0.1:25565", 1);
    h.commit = "0000000000000000000000000000000000000000".to_owned();
    let err = check_commit(&h, record::COMMIT, false).unwrap_err();
    assert!(err.contains("--force"), "{err}");
    assert!(check_commit(&h, record::COMMIT, true).is_ok());
    h.commit = record::COMMIT.to_owned();
    assert!(check_commit(&h, record::COMMIT, false).is_ok());
}
