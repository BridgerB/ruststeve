//! Issue 08a: the server model's bytes are a server's bytes. Every packet the model sends decodes
//! back to itself, and a column packs and decodes to the same states cell for cell.

use std::sync::Arc;

use proptest::prelude::*;
use protocol::chunk::{Column, Paletted, Section};
use protocol::{S2c, State, decode};

fn slot() -> impl Strategy<Value = protocol::slot::Slot> {
    prop_oneof![
        Just(protocol::slot::Slot::Empty),
        (0u32..1500, 1i32..65).prop_map(|(item, count)| protocol::slot::Slot::Item { item, count }),
    ]
}

fn finite() -> impl Strategy<Value = f64> {
    -3.0e7..3.0e7f64
}

fn name() -> impl Strategy<Value = String> {
    "[a-z0-9_:]{1,16}"
}

fn sections(n: usize) -> impl Strategy<Value = Vec<Section>> {
    proptest::collection::vec(
        (
            // a few distinct states per section, sometimes one
            proptest::collection::vec(0u32..30_000, 1..6),
            proptest::collection::vec(any::<prop::sample::Index>(), 4096),
            0u32..64,
        )
            .prop_map(|(states, picks, biome)| {
                let cells: Vec<u32> = picks
                    .iter()
                    .map(|i| states[i.index(states.len())])
                    .collect();
                Section {
                    block_count: 0,
                    fluid_count: 0,
                    blocks: Paletted::pack(&cells, 4),
                    biomes: Paletted::Single(biome),
                }
            }),
        n..=n,
    )
}

/// Every packet the model sends, with arbitrary fields, and the state it is read in.
fn modelled() -> impl Strategy<Value = (State, S2c)> {
    prop_oneof![
        (any::<[u8; 16]>(), name())
            .prop_map(|(uuid, username)| (State::Login, S2c::LoginFinished { uuid, username })),
        Just((State::Configuration, S2c::SelectKnownPacks)),
        any::<i64>().prop_map(|id| (State::Configuration, S2c::ConfigKeepAlive { id })),
        Just((State::Configuration, S2c::FinishConfiguration)),
        (any::<i32>(), 2i32..32, 2i32..32, 0i32..4, name(), 0u8..4).prop_map(
            |(
                entity_id,
                view_distance,
                simulation_distance,
                dimension_type,
                dimension,
                game_type,
            )| (
                State::Play,
                S2c::Login {
                    entity_id,
                    view_distance,
                    simulation_distance,
                    dimension_type,
                    dimension,
                    game_type
                }
            )
        ),
        any::<i64>().prop_map(|id| (State::Play, S2c::KeepAlive { id })),
        (
            0i32..i32::MAX,
            finite(),
            finite(),
            finite(),
            -180.0f32..180.0,
            -90.0f32..90.0,
            0u32..32
        )
            .prop_map(|(teleport_id, x, y, z, yaw, pitch, flags)| (
                State::Play,
                S2c::PlayerPosition {
                    teleport_id,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                    flags
                }
            )),
        Just((State::Play, S2c::SetTime)),
        (any::<i32>(), any::<i32>())
            .prop_map(|(x, z)| (State::Play, S2c::ForgetLevelChunk { x, z })),
        (
            -33_554_432i32..33_554_432,
            -2048i32..2048,
            -33_554_432i32..33_554_432,
            0u32..30_000
        )
            .prop_map(|(x, y, z, state)| (
                State::Play,
                S2c::BlockUpdate {
                    pos: (x, y, z),
                    state
                }
            )),
        (0.0f32..20.0, 0i32..20, 0.0f32..20.0).prop_map(|(health, food, saturation)| (
            State::Play,
            S2c::SetHealth {
                health,
                food,
                saturation
            }
        )),
        (0i32..1000).prop_map(|size| (State::Play, S2c::ChunkBatchFinished { size })),
        any::<i32>().prop_map(|sequence| (State::Play, S2c::BlockChangedAck { sequence })),
        (0i32..9).prop_map(|slot| (State::Play, S2c::SetHeldSlot { slot })),
        (any::<i32>(), any::<i32>(), 1i32..65).prop_map(|(item, collector, amount)| (
            State::Play,
            S2c::TakeItemEntity {
                item,
                collector,
                amount
            }
        )),
        (0i32..10).prop_map(|window| (State::Play, S2c::ContainerClose { window })),
        (0i32..3, any::<i32>(), 0i16..46, slot()).prop_map(|(window, state_id, slot, item)| (
            State::Play,
            S2c::ContainerSetSlot {
                window,
                state_id,
                slot,
                item
            }
        )),
        (0i32..41, slot())
            .prop_map(|(slot, item)| (State::Play, S2c::SetPlayerInventory { slot, item })),
        (
            0i32..3,
            any::<i32>(),
            proptest::collection::vec(slot(), 0..46),
            slot()
        )
            .prop_map(|(window, state_id, slots, carried)| (
                State::Play,
                S2c::ContainerSetContent {
                    window,
                    state_id,
                    slots,
                    carried: Some(carried)
                }
            )),
        (
            any::<i32>(),
            0i32..50,
            proptest::option::of(0i32..100_000),
            proptest::option::of(0i32..100_000)
        )
            .prop_map(|(entity_id, source_type, cause, direct)| (
                State::Play,
                S2c::DamageEvent {
                    entity_id,
                    source_type,
                    cause,
                    direct
                }
            )),
        (
            any::<i32>(),
            "death\\.attack\\.[a-z]{1,8}",
            "[a-z0-9_-]{1,16}",
            proptest::option::of("[a-z_]{1,12}")
        )
            .prop_map(|(player_id, key, victim, by)| {
                let text = match &by {
                    Some(b) => format!("{key} {victim} entity.minecraft.{b}"),
                    None => format!("{key} {victim}"),
                };
                (
                    State::Play,
                    S2c::PlayerCombatKill {
                        player_id,
                        death: protocol::nbt::Death {
                            text,
                            key: Some(key),
                            victim: Some(victim),
                            by,
                        },
                    },
                )
            }),
    ]
}

proptest! {
    #[test]
    fn every_modelled_packet_decodes_to_itself((state, p) in modelled()) {
        let (id, body) = p.encode().unwrap();
        prop_assert_eq!(decode(state, id, &body).unwrap(), p);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(16))]
    #[test]
    fn a_chunk_column_decodes_to_itself(x in -1000i32..1000, z in -1000i32..1000, secs in sections(24)) {
        let column = Arc::new(Column { min_y: -64, sections: secs });
        let p = S2c::LevelChunkWithLight { x, z, column };
        let (id, body) = p.encode().unwrap();
        prop_assert_eq!(decode(State::Play, id, &body).unwrap(), p);
    }
}

#[test]
fn a_packed_container_reads_back_every_cell() {
    let cells: Vec<u32> = (0..4096u32)
        .map(|i| [1, 9, 33, 7000, 2][(i % 5) as usize])
        .collect();
    let p = Paletted::pack(&cells, 4);
    for (i, want) in cells.iter().enumerate() {
        assert_eq!(p.get(i), Some(*want));
    }
    assert_eq!(Paletted::pack(&[5; 4096], 4), Paletted::Single(5));
}

#[test]
fn a_click_predicts_nothing() {
    let (id, body) = protocol::C2s::ContainerClick {
        window: 0,
        state_id: 7,
        slot: 36,
        button: 0,
        mode: 0,
    }
    .encode()
    .unwrap();
    assert_eq!(
        id,
        data::generated::packets::PlayServerbound::ContainerClick.id()
    );
    // window 0, state id 7, slot 36 (i16), button 0, mode 0, no changed slots, an empty cursor.
    assert_eq!(body, [0, 7, 0, 36, 0, 0, 0, 0]);
}

#[test]
fn a_dig_carries_its_sequence() {
    let (_, body) = protocol::C2s::PlayerAction {
        status: 2,
        pos: (1, 64, -1),
        face: 1,
        sequence: 5,
    }
    .encode()
    .unwrap();
    assert_eq!(body[0], 2);
    assert_eq!(*body.last().unwrap(), 5);
    assert_eq!(body.len(), 1 + 8 + 1 + 1);
}
