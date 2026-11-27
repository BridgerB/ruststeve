//! Issue 07a: the stage ladder read from the world, the death the server tells, the hits kept.

use data::ItemKind;
use protocol::S2c;
use protocol::nbt::Death;
use protocol::slot::Slot;
use world::{Event, HITS_KEPT_TICKS, Stage, World};

fn login(dimension: &str) -> Event {
    Event::Packet(S2c::Login {
        entity_id: 7,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: dimension.to_owned(),
        game_type: 0,
    })
}

fn teleport() -> Event {
    Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x: 0.5,
        y: 64.0,
        z: 0.5,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    })
}

fn give(slot: i32, item: ItemKind, count: i32) -> Event {
    Event::Packet(S2c::SetPlayerInventory {
        slot,
        item: Slot::item(item.id(), count),
    })
}

fn tick() -> Event {
    Event::Tick { now_ms: 0, seed: 1 }
}

#[test]
fn the_ladder_is_in_the_plans_order_and_named_both_ways() {
    let mut sorted = Stage::ALL;
    sorted.sort();
    assert_eq!(sorted, Stage::ALL);
    for s in Stage::ALL {
        assert_eq!(Stage::from_name(s.name()), Some(s));
    }
}

#[test]
fn a_bot_logged_in_but_not_placed_holds_no_stage() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&tick());
    assert_eq!(w.stage_reached, None);
    w.step(&teleport());
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::InWorld));
}

#[test]
fn the_stage_reached_only_rises() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::OakLog, 4));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Log));
    w.step(&give(1, ItemKind::CraftingTable, 1));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Table));
    // The table is placed and the logs spent: still reached.
    w.step(&Event::Packet(S2c::SetPlayerInventory {
        slot: 1,
        item: Slot::Empty,
    }));
    w.step(&Event::Packet(S2c::SetPlayerInventory {
        slot: 0,
        item: Slot::Empty,
    }));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Table));
}

#[test]
fn counts_and_dimensions_set_the_upper_stages() {
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::BlazeRod, 6));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Nether));
    w.step(&give(0, ItemKind::BlazeRod, 7));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Rods7));
}

#[test]
fn the_kit_needs_both_the_pickaxe_and_the_bucket() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::IronPickaxe, 1));
    w.step(&tick());
    assert!(!Stage::IronKit.holds(&w));
    w.step(&give(1, ItemKind::Bucket, 1));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::IronKit));
}

#[test]
fn only_the_bots_own_death_is_kept() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    let death = |player_id| {
        Event::Packet(S2c::PlayerCombatKill {
            player_id,
            death: Death {
                text: "death.attack.mob rs-001 entity.minecraft.zombie".to_owned(),
                key: Some("death.attack.mob".to_owned()),
                victim: Some("rs-001".to_owned()),
                by: Some("zombie".to_owned()),
            },
        })
    };
    w.step(&death(8));
    assert_eq!(w.death, None);
    w.step(&death(7));
    assert_eq!(w.death.and_then(|d| d.by).as_deref(), Some("zombie"));
}

#[test]
fn hits_on_the_bot_are_named_and_kept_a_minute() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    let hit = |entity_id, source_type| {
        Event::Packet(S2c::DamageEvent {
            entity_id,
            source_type,
            cause: None,
            direct: None,
        })
    };
    let mob_attack = data::generated::registries::DAMAGE_TYPE
        .iter()
        .position(|n| *n == "mob_attack")
        .and_then(|i| i32::try_from(i).ok())
        .unwrap();
    w.step(&hit(99, mob_attack));
    assert!(w.hits.is_empty(), "another entity's hurt is not ours");
    w.step(&hit(7, mob_attack));
    w.step(&hit(7, 9999));
    let names: Vec<&str> = w.hits.iter().map(|h| h.damage_type.as_str()).collect();
    assert_eq!(names, ["mob_attack", "unknown:9999"]);
    for _ in 0..(HITS_KEPT_TICKS / 20 + 2) {
        w.step(&Event::Packet(S2c::SetTime {
            game_time: 0,
            clocks: Vec::new(),
        }));
    }
    w.step(&hit(7, mob_attack));
    assert_eq!(w.hits.len(), 1, "older than a minute is dropped");
}

/// Issue 16's clean checkpoint: the whole kit, logs and safe food held is still not `ready`
/// underground or with no terrain seen (the sky is part of it), and unsafe food never counts.
#[test]
fn ready_needs_the_kit_the_stock_and_the_open_sky() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    let kit = [
        (ItemKind::IronHelmet, 1),
        (ItemKind::IronChestplate, 1),
        (ItemKind::IronLeggings, 1),
        (ItemKind::IronSword, 1),
        (ItemKind::WaterBucket, 1),
        (ItemKind::Bucket, 1),
        (ItemKind::FlintAndSteel, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::OakLog, 4),
        (ItemKind::RottenFlesh, 20),
        (ItemKind::Beef, 11),
    ];
    for (slot, (item, n)) in kit.into_iter().enumerate() {
        w.step(&give(slot as i32, item, n));
    }
    w.step(&tick());
    // Eleven raw beef are 33 points; the rotten flesh adds nothing.
    assert_eq!(world::stage::safe_food(&w), 33);
    assert!(
        !world::stage::open_sky(&w),
        "no column is loaded: no sky seen"
    );
    assert!(!Stage::Ready.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Gear));
}

/// The keys reach the server as vanilla sends them: `player_input` when they change, once,
/// ahead of the tick's movement packet. Without it a server never sees the bot sneak.
#[test]
fn the_keys_held_are_reported_when_they_change() {
    use physics::Controls;
    use protocol::C2s;
    use world::Effect;
    let inputs = |out: &[Effect]| -> Vec<u8> {
        out.iter()
            .filter_map(|e| match e {
                Effect::Send(C2s::PlayerInput { flags }) => Some(*flags),
                _ => None,
            })
            .collect()
    };
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    assert!(inputs(&w.step(&tick())).is_empty(), "nothing pressed");
    w.step(&Event::Controls(Controls {
        sneak: true,
        ..Controls::default()
    }));
    let out = w.step(&tick());
    assert_eq!(inputs(&out), [0x20], "shift");
    assert!(
        matches!(out.first(), Some(Effect::Send(C2s::PlayerInput { .. }))),
        "before the move: {out:?}"
    );
    assert!(inputs(&w.step(&tick())).is_empty(), "sent once");
    w.step(&Event::Controls(Controls {
        forward: true,
        jump: true,
        ..Controls::default()
    }));
    assert_eq!(inputs(&w.step(&tick())), [0x11], "forward and jump");
    w.step(&Event::Controls(Controls::default()));
    assert_eq!(inputs(&w.step(&tick())), [0], "let go");
}

fn enderman(id: i32, x: f64, z: f64) -> Event {
    Event::Packet(S2c::AddEntity {
        id,
        uuid: [id as u8; 16],
        kind: data::EntityKind::Enderman.id(),
        x,
        y: 64.0,
        z,
        velocity: [0.0; 3],
    })
}

/// Issue 17's stage: 7 rods held and the feet within 32 of an enderman that ticks. The bot
/// stands at (0.5, 64, 0.5), chunk (0, 0), simulation distance 3: chunks -3 to 3 tick.
#[test]
fn endermen_holds_within_32_of_one_that_ticks() {
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::BlazeRod, 7));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Rods7));
    // 32.5 off: beyond the edge.
    w.step(&enderman(40, 33.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Endermen.holds(&w));
    // 31.5 off, chunk 2: holds.
    w.step(&enderman(41, 32.0, 0.5));
    w.step(&tick());
    assert_eq!(world::stage::enderman_near(&w).map(|(id, _)| id), Some(41));
    assert_eq!(w.stage_reached, Some(Stage::Endermen));
    assert_eq!(Stage::from_name("endermen"), Some(Stage::Endermen));
    assert_eq!(Stage::ALL.len(), 27);
}

/// An enderman near but frozen (its chunk beyond the simulation distance) does not count, and
/// neither does one near a bot without the rods.
#[test]
fn a_frozen_enderman_or_no_rods_is_not_the_stage() {
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    w.step(&Event::Packet(S2c::SetSimulationDistance { distance: 1 }));
    w.step(&give(0, ItemKind::BlazeRod, 7));
    // 31.5 off on x: chunk 2, beyond distance 1.
    w.step(&enderman(41, 32.0, 0.5));
    w.step(&tick());
    assert!(!world::ticks(&w, [32.0, 64.0, 0.5]));
    assert!(world::ticks(&w, [31.9, 64.0, -16.0]));
    assert!(!world::ticks(&w, [0.5, 64.0, -16.1]));
    assert!(!Stage::Endermen.holds(&w));
    w.step(&Event::Packet(S2c::SetSimulationDistance { distance: 3 }));
    assert!(Stage::Endermen.holds(&w));
    w.step(&give(0, ItemKind::BlazeRod, 6));
    assert!(
        !Stage::Endermen.holds(&w),
        "an enderman before rod 7 is not the stage"
    );
}

#[test]
fn nothing_ticks_with_no_body_or_distance() {
    let mut w = World::new();
    assert!(!world::ticks(&w, [0.0, 64.0, 0.0]));
    w.step(&Event::Packet(S2c::Respawn {
        dimension_type: 3,
        dimension: "minecraft:the_nether".to_owned(),
    }));
    w.step(&teleport());
    assert!(
        !world::ticks(&w, [0.0, 64.0, 0.0]),
        "no simulation distance told"
    );
}

/// A Nether column at chunk (0, 0) whose every cell is biome id 0, with the biome registry sent
/// as `[biome]`: the feet at (0.5, 64, 0.5) stand in it.
fn biome_column(biome: &str) -> [Event; 2] {
    use protocol::chunk::{Column, Paletted, Section};
    let air = Section {
        block_count: 0,
        fluid_count: 0,
        blocks: Paletted::Single(0),
        biomes: Paletted::Single(0),
    };
    [
        Event::Packet(S2c::RegistryData {
            registry: world::BIOME_REGISTRY.to_owned(),
            entries: vec![format!("minecraft:{biome}")],
        }),
        Event::Packet(S2c::LevelChunkWithLight {
            x: 0,
            z: 0,
            column: std::sync::Arc::new(Column {
                min_y: 0,
                sections: vec![air; 16],
            }),
        }),
    ]
}

/// Issue 18's checkpoint `forest`: the 7 rods with the feet in a warped forest. A soul sand
/// valley is not it, and neither is the forest without the rods.
#[test]
fn forest_holds_with_the_rods_in_a_warped_forest() {
    for (biome, rods, holds) in [
        ("warped_forest", 7, true),
        ("soul_sand_valley", 7, false),
        ("warped_forest", 6, false),
    ] {
        let mut w = World::new();
        w.step(&login("minecraft:the_nether"));
        for e in biome_column(biome) {
            w.step(&e);
        }
        w.step(&teleport());
        w.step(&give(0, ItemKind::BlazeRod, rods));
        w.step(&tick());
        assert_eq!(w.feet_biome(), Some(biome));
        assert_eq!(Stage::Forest.holds(&w), holds, "{biome} with {rods} rods");
        if holds {
            assert_eq!(w.stage_reached, Some(Stage::Forest));
        }
    }
    assert_eq!(Stage::from_name("forest"), Some(Stage::Forest));
    assert!(Stage::Endermen < Stage::Forest && Stage::Forest < Stage::Pearls6);
}

/// `pearls_6`, `pearls_9` and `pearls_12` hold with the rods and 6, 9 or 12 pearls; pearls without the rods are
/// neither (the ladder stays a ladder).
#[test]
fn the_pearl_stages_need_the_rods_and_their_pearls() {
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::EnderPearl, 12));
    w.step(&tick());
    assert!(!Stage::Pearls6.holds(&w) && !Stage::Pearls12.holds(&w));
    w.step(&give(1, ItemKind::BlazeRod, 7));
    w.step(&give(0, ItemKind::EnderPearl, 5));
    w.step(&tick());
    assert!(!Stage::Pearls6.holds(&w));
    w.step(&give(0, ItemKind::EnderPearl, 6));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Pearls6));
    assert!(!Stage::Pearls9.holds(&w) && !Stage::Pearls12.holds(&w));
    w.step(&give(0, ItemKind::EnderPearl, 9));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Pearls9));
    assert!(!Stage::Pearls12.holds(&w));
    w.step(&give(0, ItemKind::EnderPearl, 12));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Pearls12));
    assert_eq!(world::stage::PEARLS_HALF, 6);
    assert_eq!(Stage::from_name("pearls_6"), Some(Stage::Pearls6));
    assert!(Stage::Pearls6 < Stage::Pearls9 && Stage::Pearls9 < Stage::Pearls12);
    assert_eq!(world::stage::PEARLS_THREE_QUARTERS, 9);
    assert_eq!(Stage::from_name("pearls_9"), Some(Stage::Pearls9));
}

/// A race names a stage's snapshot `snapshot-<stage>-<run>-race-<run>-lane-<n>.tar.zst` from
/// `Stage::name` (crates/bot/src/bin/race.rs, .github/workflows/race.yml): the new stages' names
/// are asset-safe, so `snapshot-forest-…` and `snapshot-pearls_6-…` are published as any other.
#[test]
fn every_stage_name_is_safe_in_a_snapshot_asset_name() {
    for s in Stage::ALL {
        assert!(
            s.name()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
            "{}",
            s.name()
        );
    }
    assert_eq!(Stage::Forest.name(), "forest");
}

fn teleport_to(x: f64, y: f64, z: f64) -> Event {
    Event::Packet(S2c::PlayerPosition {
        teleport_id: 2,
        x,
        y,
        z,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    })
}

/// An air column (state 0) at chunk `(x, z)`, y 0 to 255.
fn air_column(x: i32, z: i32) -> Event {
    use protocol::chunk::{Column, Paletted, Section};
    let air = Section {
        block_count: 0,
        fluid_count: 0,
        blocks: Paletted::Single(0),
        biomes: Paletted::Single(0),
    };
    Event::Packet(S2c::LevelChunkWithLight {
        x,
        z,
        column: std::sync::Arc::new(Column {
            min_y: 0,
            sections: vec![air; 16],
        }),
    })
}

/// A frame ring (12 frames on a 5 by 5 square less its corners) at y `y` from (x0, z0): centre
/// (x0 + 2.5, z0 + 2.5).
fn ring(x0: i32, y: i32, z0: i32) -> Vec<Event> {
    let frame = data::BlockKind::EndPortalFrame
        .info()
        .map(|i| i.default_state)
        .unwrap();
    (1..4)
        .flat_map(|i| {
            [
                (x0 + i, z0),
                (x0 + i, z0 + 4),
                (x0, z0 + i),
                (x0 + 4, z0 + i),
            ]
        })
        .map(|(x, z)| {
            Event::Packet(S2c::BlockUpdate {
                pos: (x, y, z),
                state: frame,
            })
        })
        .collect()
}

/// A second's ticks: the stages that scan memory are looked at once a second.
fn second(w: &mut World) {
    (0..20).for_each(|_| {
        w.step(&tick());
    });
}

/// Issue 19's checkpoint `overworld`: in the overworld with 12 eyes and pearls together; not in
/// the Nether with them, not with 11.
#[test]
fn overworld_holds_home_with_twelve_eyes_and_pearls() {
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::EnderEye, 12));
    w.step(&tick());
    assert!(!Stage::Overworld.holds(&w), "not in the Nether");
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    w.step(&give(0, ItemKind::EnderEye, 7));
    w.step(&give(1, ItemKind::EnderPearl, 4));
    w.step(&tick());
    assert!(!Stage::Overworld.holds(&w));
    w.step(&give(1, ItemKind::EnderPearl, 5));
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Overworld));
    assert_eq!(world::stage::EYES_FULL, 12);
    assert!(Stage::Pearls12 < Stage::Overworld && Stage::Overworld < Stage::Stronghold);
    assert!(Stage::Stronghold < Stage::PortalRoom && Stage::PortalRoom < Stage::End);
    assert_eq!(Stage::from_name("overworld"), Some(Stage::Overworld));
    assert_eq!(Stage::from_name("stronghold"), Some(Stage::Stronghold));
    assert_eq!(Stage::from_name("portal_room"), Some(Stage::PortalRoom));
}

/// Issue 19's `stronghold` and `portal_room`: a frame ring seen exposed within 128 is the
/// stronghold; the feet in an air cell within 4.5 of the ring's centre at the room's height is the
/// room. Far off on x, or above the room, is not; no frames in memory is neither.
#[test]
fn stronghold_and_portal_room_read_the_frame_ring() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    w.step(&air_column(0, 0));
    w.step(&air_column(1, 0));
    w.step(&air_column(1, 1));
    second(&mut w);
    assert_eq!(w.stage_reached, Some(Stage::InWorld));
    assert!(!Stage::Stronghold.holds(&w));
    // The ring at x 20 to 24, z 20 to 24, y 30: centre (22.5, 22.5).
    for e in ring(20, 30, 20) {
        w.step(&e);
    }
    assert_eq!(world::stage::frames_near(&w).len(), 12);
    assert_eq!(world::stage::frame_ring(&w), Some(([22.5, 22.5], 30)));
    second(&mut w);
    assert_eq!(w.stage_reached, Some(Stage::Stronghold));
    assert!(!Stage::PortalRoom.holds(&w), "22 off on both axes");
    // Above the room (frame y + 3): not in it.
    w.step(&teleport_to(18.5, 33.0, 22.5));
    second(&mut w);
    assert!(!Stage::PortalRoom.holds(&w));
    // On the floor beside the ring (frame y - 2): in it.
    w.step(&teleport_to(18.5, 28.0, 22.5));
    second(&mut w);
    assert_eq!(w.stage_reached, Some(Stage::PortalRoom));
    // 5 off on x: past the room's short half (4.5), maybe in a wall or the corridor beyond.
    w.step(&teleport_to(17.4, 28.0, 22.5));
    w.step(&tick());
    assert!(!Stage::PortalRoom.holds(&w));
    assert_eq!(world::stage::ROOM_HALF, 4.5);
    // 9 off on x: outside the box.
    w.step(&teleport_to(13.4, 28.0, 22.5));
    w.step(&tick());
    assert!(!Stage::PortalRoom.holds(&w));
}

// >>> E1: issue 20's stages.

/// A column at chunk `(x, z)` of air with the section from y 48 to 63 all `state`.
fn column_with(x: i32, z: i32, state: u32) -> Event {
    use protocol::chunk::{Column, Paletted, Section};
    let air = Section {
        block_count: 0,
        fluid_count: 0,
        blocks: Paletted::Single(0),
        biomes: Paletted::Single(0),
    };
    let mut sections = vec![air; 16];
    sections[3] = Section {
        block_count: 4096,
        fluid_count: 0,
        blocks: Paletted::Single(state),
        biomes: Paletted::Single(0),
    };
    Event::Packet(S2c::LevelChunkWithLight {
        x,
        z,
        column: std::sync::Arc::new(Column { min_y: 0, sections }),
    })
}

fn add(id: i32, kind: data::EntityKind, x: f64, y: f64, z: f64) -> Event {
    Event::Packet(S2c::AddEntity {
        id,
        uuid: [id as u8; 16],
        kind: kind.id(),
        x,
        y,
        z,
        velocity: [0.0; 3],
    })
}

fn state(k: data::BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap()
}

/// The ladder's new rungs: `armed` between `portal_room` and `end`; `island`, `crystals` and
/// `win` after `end`, by name both ways.
#[test]
fn the_end_stages_sit_in_order() {
    assert!(Stage::PortalRoom < Stage::Armed && Stage::Armed < Stage::End);
    assert!(Stage::End < Stage::Island && Stage::Island < Stage::Crystals5);
    assert!(Stage::Crystals5 < Stage::Crystals1 && Stage::Crystals1 < Stage::Crystals);
    assert!(Stage::Crystals < Stage::Win);
    for (s, n) in [
        (Stage::Armed, "armed"),
        (Stage::Island, "island"),
        (Stage::Crystals5, "crystals_5"),
        (Stage::Crystals1, "crystals_1"),
        (Stage::Crystals, "crystals"),
        (Stage::Win, "win"),
    ] {
        assert_eq!(s.name(), n);
        assert_eq!(Stage::from_name(n), Some(s));
    }
}

/// `armed`: the whole kit in the overworld; one piece short (one pickaxe, 23 cooked, too few
/// eyes for the empty frames, no water bucket) is not; the Nether is not.
#[test]
fn armed_needs_the_whole_kit_in_the_overworld() {
    let kit = |w: &mut World| {
        w.step(&give(0, ItemKind::IronSword, 1));
        w.step(&give(1, ItemKind::StonePickaxe, 1));
        w.step(&give(2, ItemKind::StonePickaxe, 1));
        w.step(&give(3, ItemKind::CookedPorkchop, 11));
        w.step(&give(4, ItemKind::CookedMutton, 13));
        w.step(&give(5, ItemKind::EnderEye, 12));
        w.step(&give(6, ItemKind::Cobblestone, 64));
        w.step(&give(7, ItemKind::Netherrack, 64));
        w.step(&give(8, ItemKind::WaterBucket, 1));
    };
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    kit(&mut w);
    second(&mut w);
    assert!(Stage::Armed.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Armed));
    assert_eq!(world::stage::armed_pickaxes(&w), (2, 262));
    assert_eq!(
        world::stage::empty_frames(&w),
        12,
        "no frame seen: all empty"
    );
    for (slot, item, n) in [
        (2, ItemKind::Stick, 1),
        (4, ItemKind::CookedMutton, 12),
        (5, ItemKind::EnderEye, 11),
        (8, ItemKind::Bucket, 1),
        (7, ItemKind::Netherrack, 63),
    ] {
        let mut w = World::new();
        w.step(&login("minecraft:overworld"));
        w.step(&teleport());
        kit(&mut w);
        w.step(&give(slot, item, n));
        w.step(&tick());
        assert!(!Stage::Armed.holds(&w), "{item:?} x{n} in slot {slot}");
    }
    let mut w = World::new();
    w.step(&login("minecraft:the_nether"));
    w.step(&teleport());
    kit(&mut w);
    w.step(&tick());
    assert!(!Stage::Armed.holds(&w));
}

// >>> E8 (issue 20, the bow line)
/// `bow_armed` is `armed` with a bow and 16 arrows, between `armed` and `end` on the ladder.
#[test]
fn bow_armed_is_armed_with_a_bow_and_sixteen_arrows() {
    assert!(Stage::Armed < Stage::BowArmed && Stage::BowArmed < Stage::End);
    assert_eq!(Stage::from_name("bow_armed"), Some(Stage::BowArmed));
    let kit = |w: &mut World| {
        w.step(&give(0, ItemKind::IronSword, 1));
        w.step(&give(1, ItemKind::StonePickaxe, 1));
        w.step(&give(2, ItemKind::StonePickaxe, 1));
        w.step(&give(3, ItemKind::CookedPorkchop, 11));
        w.step(&give(4, ItemKind::CookedMutton, 13));
        w.step(&give(5, ItemKind::EnderEye, 12));
        w.step(&give(6, ItemKind::Cobblestone, 64));
        w.step(&give(7, ItemKind::Netherrack, 64));
        w.step(&give(8, ItemKind::WaterBucket, 1));
    };
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    kit(&mut w);
    w.step(&give(9, ItemKind::Bow, 1));
    w.step(&give(10, ItemKind::Arrow, 15));
    second(&mut w);
    assert!(
        Stage::Armed.holds(&w) && !Stage::BowArmed.holds(&w),
        "15 arrows"
    );
    w.step(&give(10, ItemKind::Arrow, 16));
    second(&mut w);
    assert!(Stage::BowArmed.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::BowArmed));
}
// <<< E8

/// The eyes `armed` asks for are the ring's empty frames: a frame on record with an eye is one
/// fewer (the frontier's ring has one, probe 45863427332).
#[test]
fn armed_counts_the_frames_with_an_eye() {
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&teleport());
    w.step(&air_column(0, 0));
    w.step(&air_column(1, 0));
    w.step(&air_column(1, 1));
    for e in ring(20, 30, 20) {
        w.step(&e);
    }
    // One frame with an eye (9469: eye=true, facing north).
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (21, 30, 24),
        state: 9469,
    }));
    w.step(&tick());
    assert_eq!(world::stage::frames_with_eye(&w), 1);
    assert_eq!(world::stage::empty_frames(&w), 11);
}

/// `island`: on end stone within 30 of the fountain, in the End; 31 off, on air, or in the
/// overworld is not.
#[test]
fn island_is_end_stone_near_the_fountain() {
    let end_stone = state(data::BlockKind::EndStone);
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&column_with(0, 0, end_stone));
    w.step(&column_with(1, 0, end_stone));
    w.step(&column_with(-1, 0, 0));
    w.step(&teleport_to(20.5, 64.0, 0.5));
    w.step(&tick());
    assert!(Stage::Island.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Island));
    w.step(&teleport_to(31.0, 64.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Island.holds(&w), "31 off");
    w.step(&teleport_to(-5.0, 64.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Island.holds(&w), "over air");
    let mut w = World::new();
    w.step(&login("minecraft:overworld"));
    w.step(&column_with(0, 0, end_stone));
    w.step(&teleport_to(0.5, 64.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Island.holds(&w));
}

/// `crystals`: near the fountain with every spike's column loaded and no crystal alive. A
/// crystal alive, a spike column not loaded, or the bot far out is not.
#[test]
fn crystals_needs_every_spike_seen_and_none_alive() {
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    for [x, z] in world::stage::SPIKES {
        w.step(&air_column(x.div_euclid(16), z.div_euclid(16)));
    }
    w.step(&add(90, data::EntityKind::EndCrystal, 42.5, 83.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Crystals.holds(&w), "one alive");
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![90] }));
    w.step(&tick());
    assert!(Stage::Crystals.holds(&w));
    w.step(&teleport_to(70.5, 70.0, 0.5));
    w.step(&tick());
    assert!(!Stage::Crystals.holds(&w), "70 off the fountain");
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    for [x, z] in world::stage::SPIKES.iter().skip(1) {
        w.step(&air_column(x.div_euclid(16), z.div_euclid(16)));
    }
    w.step(&tick());
    // The ten spikes' chunks are ten different columns: one missing is a spike not seen.
    assert!(!Stage::Crystals.holds(&w), "a spike's column not loaded");
}

/// `crystals_5` and `crystals_1`: counted the way `crystals` is (near the fountain, every
/// spike's column loaded, the crystals the server sent): 6 alive is neither, 5 is `crystals_5`,
/// 1 is `crystals_1`, 0 is all three and `crystals` is reached. A spike's column not loaded, or
/// the bot far out, is none of them however few were sent.
#[test]
fn crystals_5_and_1_count_the_crystals_left() {
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    for [x, z] in world::stage::SPIKES {
        w.step(&air_column(x.div_euclid(16), z.div_euclid(16)));
    }
    for (i, [x, z]) in world::stage::SPIKES.iter().take(6).enumerate() {
        let id = 90 + i as i32;
        w.step(&add(
            id,
            data::EntityKind::EndCrystal,
            f64::from(*x) + 0.5,
            83.0,
            f64::from(*z) + 0.5,
        ));
    }
    w.step(&tick());
    assert_eq!(world::stage::crystals_left(&w), Some(6));
    assert_eq!(w.stage_reached, Some(Stage::End), "6 left");
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![90] }));
    w.step(&tick());
    assert!(Stage::Crystals5.holds(&w) && !Stage::Crystals1.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Crystals5));
    w.step(&Event::Packet(S2c::RemoveEntities {
        ids: vec![91, 92, 93],
    }));
    w.step(&tick());
    assert_eq!(world::stage::crystals_left(&w), Some(2));
    assert_eq!(w.stage_reached, Some(Stage::Crystals5), "2 left");
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![94] }));
    w.step(&tick());
    assert!(Stage::Crystals1.holds(&w) && !Stage::Crystals.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Crystals1));
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![95] }));
    w.step(&tick());
    assert!(Stage::Crystals5.holds(&w) && Stage::Crystals1.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::Crystals));
    w.step(&teleport_to(70.5, 70.0, 0.5));
    w.step(&tick());
    assert_eq!(world::stage::crystals_left(&w), None, "70 off the fountain");
    assert!(!Stage::Crystals5.holds(&w) && !Stage::Crystals1.holds(&w));
    // One spike's column never loaded: its crystal may be alive and unsent, so no count.
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    for [x, z] in world::stage::SPIKES.iter().skip(1) {
        w.step(&air_column(x.div_euclid(16), z.div_euclid(16)));
    }
    w.step(&tick());
    assert_eq!(world::stage::crystals_left(&w), None);
    assert!(!Stage::Crystals5.holds(&w) && !Stage::Crystals1.holds(&w));
    assert_eq!(w.stage_reached, Some(Stage::End));
}

/// `win`: the credits with the dragon seen dead before them (its death event, or its boss bar
/// removed in the End). The credits alone are not.
#[test]
fn win_needs_the_dragon_dead_then_the_credits() {
    let win = Event::Packet(S2c::GameEvent {
        event: 4,
        param: 0.0,
    });
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    w.step(&win);
    w.step(&tick());
    assert!(w.won && !Stage::Win.holds(&w), "credits alone");
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&teleport_to(0.5, 70.0, 0.5));
    w.step(&add(80, data::EntityKind::EnderDragon, 0.5, 70.0, 0.5));
    w.step(&Event::Packet(S2c::EntityEvent { id: 80, status: 3 }));
    assert!(w.dragon_dead.is_some());
    w.step(&win);
    w.step(&tick());
    assert_eq!(w.stage_reached, Some(Stage::Win));
    // The boss bar removed in the End is the death too.
    let mut w = World::new();
    w.step(&login("minecraft:the_end"));
    w.step(&Event::Packet(S2c::BossEvent {
        uuid: [5; 16],
        op: protocol::BossOp::Add {
            name: "entity.minecraft.ender_dragon".to_owned(),
            progress: 0.0,
        },
    }));
    w.step(&Event::Packet(S2c::BossEvent {
        uuid: [5; 16],
        op: protocol::BossOp::Remove,
    }));
    assert!(w.dragon_dead.is_some());
}

/// The kit and geography in `config/end.toml` are the stage's constants.
#[test]
fn the_stage_constants_are_config_end_toml() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/end.toml"
    ))
    .unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    let kit = t["kit"].as_table().unwrap();
    assert_eq!(
        kit["pickaxes"].as_integer(),
        Some(world::stage::ARMED_PICKAXES as i64)
    );
    assert_eq!(
        kit["pickaxe_uses"].as_integer(),
        Some(i64::from(world::stage::ARMED_PICKAXE_USES))
    );
    assert_eq!(kit["cooked"].as_integer(), Some(world::stage::ARMED_COOKED));
    assert_eq!(kit["blocks"].as_integer(), Some(world::stage::ARMED_BLOCKS));
    assert_eq!(
        t["island"]["near"].as_float(),
        Some(world::stage::ISLAND_NEAR)
    );
    assert_eq!(
        t["crystals"]["near"].as_float(),
        Some(world::stage::CRYSTALS_NEAR)
    );
    assert_eq!(
        t["crystals"]["left_5"].as_integer(),
        Some(world::stage::CRYSTALS_LEFT_5 as i64)
    );
    assert_eq!(
        t["crystals"]["left_1"].as_integer(),
        Some(world::stage::CRYSTALS_LEFT_1 as i64)
    );
    let spikes: Vec<[i32; 2]> = t["spike"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let c = s["centre"].as_array().unwrap();
            [
                c[0].as_integer().unwrap() as i32,
                c[1].as_integer().unwrap() as i32,
            ]
        })
        .collect();
    assert_eq!(spikes, world::stage::SPIKES.to_vec());
}
// <<< E1
