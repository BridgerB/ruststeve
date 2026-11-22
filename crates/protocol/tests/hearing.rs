//! Issue 16e: the hearing packets read back what a server writes, by name.

use protocol::{S2c, SoundRef, State, decode};

fn round_trip(p: &S2c) -> S2c {
    let (id, body) = p.encode().unwrap();
    decode(State::Play, id, &body).unwrap()
}

#[test]
fn a_blaze_shot_sound_reads_back_by_name() {
    let id = data::generated::registries::SOUND_EVENT
        .iter()
        .position(|n| *n == "entity.blaze.shoot")
        .unwrap() as u32;
    let p = S2c::Sound {
        sound: SoundRef::Registry(id),
        source: 5,
        pos: [10.5, 64.0, -3.25],
        volume: 1.0,
        pitch: 1.2,
    };
    let got = round_trip(&p);
    assert_eq!(got, p);
    let S2c::Sound { sound, .. } = got else {
        panic!("not a sound: {got:?}")
    };
    assert_eq!(sound.name(), "entity.blaze.shoot");
}

#[test]
fn an_inline_sound_and_an_entity_sound_read_back() {
    let p = S2c::SoundEntity {
        sound: SoundRef::Inline("minecraft:entity.enderman.stare".to_owned()),
        source: 5,
        entity: 77,
        volume: 2.5,
        pitch: 1.0,
    };
    let got = round_trip(&p);
    assert_eq!(got, p);
    let S2c::SoundEntity { sound, .. } = got else {
        panic!("not an entity sound: {got:?}")
    };
    assert_eq!(sound.name(), "entity.enderman.stare");
}

#[test]
fn a_level_event_and_a_stop_sound_read_back() {
    for p in [
        S2c::LevelEvent {
            event: 1018,
            pos: (-120, 70, 33),
            data: 0,
            global: false,
        },
        S2c::StopSound {
            source: Some(5),
            sound: Some("minecraft:music.nether".to_owned()),
        },
        S2c::StopSound {
            source: None,
            sound: None,
        },
    ] {
        assert_eq!(round_trip(&p), p);
    }
    assert_eq!(
        data::generated::events::level_event_name(1018),
        Some("sound_blaze_fireball")
    );
}

/// Issue 16e, sight of mobs: each packet reads back what a server writes.
#[test]
fn the_sight_packets_read_back() {
    use protocol::slot::Slot;
    for p in [
        S2c::RotateHead { id: 3, yaw: 90.0 },
        S2c::SetEquipment {
            id: 3,
            items: vec![(0, Slot::item(841, 1)), (5, Slot::Empty)],
        },
        S2c::Animate { id: 3, action: 0 },
        S2c::HurtAnimation { id: 1, yaw: 45.0 },
        S2c::ProjectilePower { id: 9, power: 0.1 },
        S2c::SetPassengers {
            vehicle: 4,
            passengers: vec![5, 6],
        },
        S2c::Explode {
            center: [1.5, 64.0, -2.5],
            knockback: Some([0.3, 0.1, 0.0]),
        },
        S2c::BlockEntityData {
            pos: (100, 60, -20),
            kind: 9,
            spawns: Some("blaze".to_owned()),
        },
        S2c::BlockEntityData {
            pos: (1, 2, 3),
            kind: 9,
            spawns: None,
        },
        S2c::BlockDestruction {
            breaker: 12,
            pos: (5, 70, 5),
            stage: 4,
        },
    ] {
        assert_eq!(round_trip(&p), p);
    }
}
