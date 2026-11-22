//! Issue 16e: the facts the ledger is made of, from packets as a server sends them.

use data::{EntityKind, ItemKind};
use observe::{Fact, diff, senses, snapshot};
use protocol::S2c;
use protocol::metadata::MetaValue;
use protocol::slot::{Slot, Wear};
use world::{Event, World};

fn worn(item: ItemKind, damage: i32) -> Slot {
    Slot::Item {
        item: item.id(),
        count: 1,
        wear: Wear {
            damage: Some(damage),
            max_damage: None,
        },
    }
}

fn put(w: &mut World, slot: i32, item: Slot) {
    w.step(&Event::Packet(S2c::SetPlayerInventory { slot, item }));
}

/// Blaze wall 7: a stone pickaxe wearing down is noted as it falls under a share of its uses, and
/// its breaking is a fact of its own (before, only "items lost" at the end said it).
#[test]
fn a_pickaxe_wearing_out_and_breaking_is_in_the_ledger() {
    let mut w = World::new();
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 60));
    let a = snapshot(&w);
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 70));
    let mut b = snapshot(&w);
    let facts: Vec<Fact> = diff(&a, &mut b, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts.iter().any(
            |f| matches!(f, Fact::ToolWorn { item, uses_left: 61 } if item == "stone_pickaxe")
        ),
        "{facts:?}"
    );
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 129));
    let c = snapshot(&w);
    put(&mut w, 0, Slot::Empty);
    let mut d = snapshot(&w);
    let facts: Vec<Fact> = diff(&c, &mut d, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts
            .iter()
            .any(|f| matches!(f, Fact::ToolBroke { item } if item == "stone_pickaxe")),
        "{facts:?}"
    );
    assert!(
        facts
            .iter()
            .any(|f| matches!(f, Fact::Item { item, from: 1, to: 0 } if item == "stone_pickaxe"))
    );
}

/// Three early endings in race 44970722653 were creepers: a creeper starting its fuse near the
/// bot is a fact, and shows in the senses row.
#[test]
fn a_creeper_fusing_is_a_sign_and_in_the_senses_row() {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::AddEntity {
        id: 9,
        uuid: [0; 16],
        kind: EntityKind::Creeper.id(),
        x: 3.0,
        y: 0.0,
        z: 0.0,
    }));
    w.player.pos = Some([0.0, 0.0, 0.0]);
    let a = snapshot(&w);
    w.step(&Event::Packet(S2c::SetEntityData {
        id: 9,
        entries: vec![(16, MetaValue::Int(1))],
    }));
    let mut b = snapshot(&w);
    let facts: Vec<Fact> = diff(&a, &mut b, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts.iter().any(
            |f| matches!(f, Fact::Sign { sign, kind: EntityKind::Creeper, .. } if sign == "fuse")
        ),
        "{facts:?}"
    );
    let row = senses(&w, "stairs", "toward the log", Some(120));
    assert_eq!(row.hostiles.len(), 1);
    assert!(row.hostiles[0].signs.fuse);
    assert_eq!(row.task, "stairs");
}
