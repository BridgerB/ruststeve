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
    assert_eq!(Stage::ALL.len(), 20);
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
/// stronghold; the feet in an air cell within 8 of the ring's centre at the room's height is the
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
    // 9 off on x: outside the box.
    w.step(&teleport_to(13.4, 28.0, 22.5));
    w.step(&tick());
    assert!(!Stage::PortalRoom.holds(&w));
}
