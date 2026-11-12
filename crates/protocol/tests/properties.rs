//! Issue 03's properties: decoding never panics on any bytes; every keep-alive is answered
//! exactly once; the state only moves along the transition table. Plus round trips of what we
//! encode.

use proptest::prelude::*;
use protocol::bytes::{Reader, pack_position, unpack_position, write_varint};
use protocol::{C2s, FrameDecoder, Out, S2c, Session, State, decode, encode_frame, split_id};

fn states() -> impl Strategy<Value = State> {
    prop_oneof![
        Just(State::Handshake),
        Just(State::Login),
        Just(State::Configuration),
        Just(State::Play),
    ]
}

proptest! {
    /// Any bytes, in any state, under any id: a result, never a panic.
    #[test]
    fn decode_never_panics(state in states(), id in 0u32..200, body in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = decode(state, id, &body);
    }

    /// Any byte stream through the frame decoder, compressed or not: frames or an error, never a
    /// panic, and never an endless loop.
    #[test]
    fn frames_never_panic(compressed in any::<bool>(), chunks in proptest::collection::vec(proptest::collection::vec(any::<u8>(), 0..64), 0..16)) {
        let mut d = FrameDecoder::new();
        if compressed {
            d.set_compression(256);
        }
        for chunk in chunks {
            d.push(&chunk);
            for _ in 0..64 {
                match d.next_packet() {
                    Ok(Some(p)) => { let _ = split_id(&p); }
                    Ok(None) | Err(_) => break,
                }
            }
        }
    }

    /// Every frame we write reads back as the same packet, with or without compression.
    #[test]
    fn frames_round_trip(id in 0u32..200, body in proptest::collection::vec(any::<u8>(), 0..2000), threshold in prop_oneof![Just(None), (0i32..1024).prop_map(Some)]) {
        let frame = encode_frame(id, &body, threshold).unwrap();
        let mut d = FrameDecoder::new();
        if let Some(t) = threshold {
            d.set_compression(t);
        }
        d.push(&frame);
        let packet = d.next_packet().unwrap().unwrap();
        prop_assert_eq!(split_id(&packet).unwrap(), (id, body.as_slice()));
    }

    #[test]
    fn varints_round_trip(v in any::<i32>()) {
        let mut out = Vec::new();
        write_varint(&mut out, v);
        prop_assert!(out.len() <= 5);
        prop_assert_eq!(Reader::new(&out).varint(), Ok(v));
    }

    #[test]
    fn positions_round_trip(x in -33_554_432i32..33_554_432, y in -2048i32..2048, z in -33_554_432i32..33_554_432) {
        prop_assert_eq!(unpack_position(pack_position(x, y, z)), (x, y, z));
    }
}

/// A server packet as the session might meet it, in a random order.
fn server_packets() -> impl Strategy<Value = S2c> {
    prop_oneof![
        any::<i64>().prop_map(|id| S2c::KeepAlive { id }),
        any::<i64>().prop_map(|id| S2c::ConfigKeepAlive { id }),
        any::<i32>().prop_map(|id| S2c::Ping { id }),
        any::<i32>().prop_map(|id| S2c::ConfigPing { id }),
        any::<i32>().prop_map(|teleport_id| S2c::PlayerPosition {
            teleport_id,
            x: 0.0,
            y: 64.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            flags: 0
        }),
        (0i32..512).prop_map(|threshold| S2c::LoginCompression { threshold }),
        Just(S2c::LoginFinished {
            uuid: [0; 16],
            username: "rs-001".to_owned()
        }),
        Just(S2c::SelectKnownPacks),
        Just(S2c::FinishConfiguration),
        Just(S2c::StartConfiguration),
        Just(S2c::SetTime),
        (0i32..64).prop_map(|size| S2c::ChunkBatchFinished { size }),
        (0u32..140).prop_map(|id| S2c::Unknown {
            state: State::Play,
            id,
            len: 0
        }),
    ]
}

proptest! {
    /// Every keep-alive gets exactly one echo of its id, in its own state's packet, and nothing
    /// else sends a keep-alive.
    #[test]
    fn keep_alives_answered_exactly_once(packets in proptest::collection::vec(server_packets(), 0..200)) {
        let mut s = Session::new();
        for p in &packets {
            let outs = s.on_packet(p);
            let echoes: Vec<C2s> = outs
                .iter()
                .filter_map(|o| match o {
                    Out::Send(c @ (C2s::KeepAlive { .. } | C2s::ConfigKeepAlive { .. })) => Some(c.clone()),
                    Out::Send(_) | Out::SetCompression(_) => None,
                })
                .collect();
            match p {
                S2c::KeepAlive { id } => prop_assert_eq!(echoes, vec![C2s::KeepAlive { id: *id }]),
                S2c::ConfigKeepAlive { id } => {
                    prop_assert_eq!(echoes, vec![C2s::ConfigKeepAlive { id: *id }]);
                }
                _ => prop_assert!(echoes.is_empty()),
            }
        }
        let expected = packets
            .iter()
            .filter(|p| matches!(p, S2c::KeepAlive { .. } | S2c::ConfigKeepAlive { .. }))
            .count() as u64;
        prop_assert_eq!(s.keep_alives_answered, expected);
    }

    /// The session's state only ever moves along a listed edge.
    #[test]
    fn state_moves_only_along_the_table(packets in proptest::collection::vec(server_packets(), 0..200)) {
        let mut s = Session::new();
        for p in &packets {
            let before = s.state;
            s.on_packet(p);
            prop_assert!(before == s.state || State::is_edge(before, s.state), "{:?} -> {:?} on {:?}", before, s.state, p);
        }
    }
}

#[test]
fn every_c2s_encodes_in_its_state() {
    let all = [
        C2s::Intention {
            protocol: 775,
            host: "127.0.0.1".to_owned(),
            port: 25565,
            next_state: 2,
        },
        C2s::Hello {
            username: "rs-001".to_owned(),
            uuid: protocol::uuid::offline_uuid("rs-001"),
        },
        C2s::LoginAcknowledged,
        C2s::ClientInformation {
            locale: "en_us".to_owned(),
            view_distance: 8,
        },
        C2s::SelectKnownPacks,
        C2s::ConfigKeepAlive { id: 1 },
        C2s::ConfigPong { id: 2 },
        C2s::FinishConfiguration,
        C2s::KeepAlive { id: 3 },
        C2s::Pong { id: 4 },
        C2s::AcceptTeleportation { teleport_id: 5 },
        C2s::PlayerLoaded,
        C2s::ChunkBatchReceived {
            chunks_per_tick: 20.0,
        },
        C2s::ConfigurationAcknowledged,
    ];
    for c in &all {
        let (id, body) = c.encode().unwrap();
        let frame = encode_frame(id, &body, Some(256)).unwrap();
        let mut d = FrameDecoder::new();
        d.set_compression(256);
        d.push(&frame);
        let packet = d.next_packet().unwrap().unwrap();
        assert_eq!(split_id(&packet).unwrap(), (id, body.as_slice()), "{c:?}");
    }
    // The transition packets move the writer's state; the rest keep it.
    assert_eq!(State::Handshake.after_send(&all[0]), State::Login);
    assert_eq!(State::Login.after_send(&all[2]), State::Configuration);
    assert_eq!(State::Configuration.after_send(&all[7]), State::Play);
    assert_eq!(State::Play.after_send(&all[13]), State::Configuration);
}

#[test]
fn the_play_login_decodes() {
    // ClientboundLoginPacket: entity id, hardcore, levels, max players, view and simulation
    // distance, reduced debug, death screen, limited crafting, dimension type, dimension, seed,
    // game type, then a tail this layer ignores.
    let mut w = protocol::Writer::new();
    w.i32(42);
    w.bool(false);
    w.varint(1);
    w.string("minecraft:overworld").unwrap();
    w.varint(20);
    w.varint(8);
    w.varint(3);
    w.bool(false);
    w.bool(true);
    w.bool(false);
    w.varint(0);
    w.string("minecraft:overworld").unwrap();
    w.i64(7);
    w.u8(0);
    w.bytes(&[1, 2, 3, 4]);
    let id = data::generated::packets::PlayClientbound::Login.id();
    assert_eq!(
        decode(State::Play, id, &w.buf).unwrap(),
        S2c::Login {
            entity_id: 42,
            view_distance: 8,
            simulation_distance: 3,
            dimension_type: 0,
            dimension: "minecraft:overworld".to_owned(),
            game_type: 0,
        }
    );
}
