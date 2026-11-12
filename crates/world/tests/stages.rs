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
        item: Slot::Item {
            item: item.id(),
            count,
        },
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
        w.step(&Event::Packet(S2c::SetTime));
    }
    w.step(&hit(7, mob_attack));
    assert_eq!(w.hits.len(), 1, "older than a minute is dropped");
}
