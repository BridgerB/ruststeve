//! Issue 16e: the facts the ledger is made of, from packets as a server sends them.

use data::{BlockKind, EntityKind, ItemKind};
use observe::{CLICK_SETTLE, Fact, Touch, diff, senses, snapshot};
use protocol::S2c;
use protocol::metadata::MetaValue;
use protocol::slot::{Slot, Wear};
use world::{Event, PendingClick, World};

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
    // Its last use (131 uses, 130 spent), then the slot is empty: it broke.
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 130));
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

/// Issue 16 (blaze rods): a fireball's closest pass is kept while it flies and told when it is
/// gone (clojurecraft's 24 of 212 that hit all passed within 1.4 blocks).
#[test]
fn a_fireballs_closest_pass_is_told_when_it_goes() {
    let mut w = World::new();
    w.player.pos = Some([0.0, 0.0, 0.0]);
    let at = |w: &mut World, x: f64| {
        w.step(&Event::Packet(S2c::AddEntity {
            id: 5,
            uuid: [0; 16],
            kind: EntityKind::SmallFireball.id(),
            x,
            y: 0.0,
            z: 0.0,
        }));
    };
    at(&mut w, 6.0);
    let mut a = snapshot(&w);
    at(&mut w, 1.2);
    let mut b = snapshot(&w);
    diff(&a, &mut b, &w);
    a = b;
    at(&mut w, 4.0);
    let mut c = snapshot(&w);
    diff(&a, &mut c, &w);
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![5] }));
    let mut d = snapshot(&w);
    let facts: Vec<Fact> = diff(&c, &mut d, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts.iter().any(|f| matches!(
            f,
            Fact::Shot { kind: EntityKind::SmallFireball, closest } if (*closest - 1.2).abs() < 1e-9
        )),
        "{facts:?}"
    );
}

fn facts_between(a: &observe::Snapshot, w: &World) -> (observe::Snapshot, Vec<Fact>) {
    let mut b = snapshot(w);
    let facts = diff(a, &mut b, w).into_iter().map(|s| s.fact).collect();
    (b, facts)
}

fn cursor(w: &mut World, item: Slot) {
    w.step(&Event::Packet(S2c::SetCursorItem { item }));
}

/// Observability gaps, wall 2: a pickaxe on its last use picked up onto the cursor and put down
/// in another slot is held throughout. The ledger read "item stone_pickaxe 1 -> 0" and "tool
/// broke" for it (the counts left the cursor out); now the only facts are the cursor's.
#[test]
fn a_pickaxe_moved_over_the_cursor_is_neither_lost_nor_broken() {
    let mut w = World::new();
    // Hotbar slot 0 is menu slot 36; main slot 9 is menu slot 9.
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 130));
    let a = snapshot(&w);
    assert_eq!(a.items.get("stone_pickaxe"), Some(&1));
    // Picked up: the slot empties and the cursor holds it.
    put(&mut w, 0, Slot::Empty);
    cursor(&mut w, worn(ItemKind::StonePickaxe, 130));
    let (b, facts) = facts_between(&a, &w);
    assert_eq!(b.items.get("stone_pickaxe"), Some(&1), "{facts:?}");
    assert_eq!(
        facts,
        vec![Fact::Cursor {
            item: Some("stone_pickaxe".to_owned()),
            count: 1
        }]
    );
    assert_eq!(facts[0].line(), "on cursor: stone_pickaxe 1");
    let row = senses(&w, "craft", "", None);
    assert_eq!(row.cursor, Some(("stone_pickaxe".to_owned(), 1)));
    // Put down in a main slot: the cursor empties.
    cursor(&mut w, Slot::Empty);
    put(&mut w, 9, worn(ItemKind::StonePickaxe, 130));
    let (c, facts) = facts_between(&b, &w);
    assert_eq!(c.items.get("stone_pickaxe"), Some(&1));
    assert_eq!(
        facts,
        vec![Fact::Cursor {
            item: None,
            count: 0
        }]
    );
    assert_eq!(facts[0].line(), "cursor empty");
}

/// Wall 2: the world forgets the cursor as it clicks, so between a click and the server's answer
/// the stack is in no slot and not on the cursor. The holdings are read as they last settled
/// until the answer (or `CLICK_SETTLE` ticks), so nothing dips and comes back.
#[test]
fn holdings_wait_for_a_clicks_answer() {
    let mut w = World::new();
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 10));
    put(&mut w, 0, Slot::Empty);
    cursor(&mut w, worn(ItemKind::StonePickaxe, 10));
    let a = snapshot(&w);
    assert_eq!(a.items.get("stone_pickaxe"), Some(&1));
    // The put-down click: the cursor is forgotten, the slot not yet filled.
    w.ticks = 100;
    w.click = Some(PendingClick {
        window: 0,
        slot: 9,
        state_id: 0,
        at_tick: 100,
    });
    w.inventory.carried = None;
    w.ticks = 102;
    let (b, facts) = facts_between(&a, &w);
    assert!(facts.is_empty(), "{facts:?}");
    assert_eq!(b.items.get("stone_pickaxe"), Some(&1));
    // The answer: the slot filled, the click answered.
    put(&mut w, 9, worn(ItemKind::StonePickaxe, 10));
    w.click = None;
    w.ticks = 104;
    let (c, facts) = facts_between(&b, &w);
    assert_eq!(
        facts,
        vec![Fact::Cursor {
            item: None,
            count: 0
        }]
    );
    assert_eq!(c.items.get("stone_pickaxe"), Some(&1));
    // A click never answered does not hold the holdings for ever.
    put(&mut w, 9, Slot::Empty);
    w.click = Some(PendingClick {
        window: 0,
        slot: 9,
        state_id: 0,
        at_tick: 104,
    });
    w.ticks = 104 + CLICK_SETTLE;
    let (d, facts) = facts_between(&c, &w);
    assert!(facts.is_empty(), "{facts:?}");
    w.ticks = 105 + CLICK_SETTLE;
    let (_, facts) = facts_between(&d, &w);
    assert!(
        facts
            .iter()
            .any(|f| matches!(f, Fact::Item { item, from: 1, to: 0 } if item == "stone_pickaxe")),
        "{facts:?}"
    );
}

/// Wall 2: a tool gone with uses left is said as gone, never as broken; one whose uses run out
/// in its slot is broken.
#[test]
fn a_tool_gone_with_uses_left_is_not_a_break() {
    let mut w = World::new();
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 91));
    let a = snapshot(&w);
    put(&mut w, 0, Slot::Empty);
    let (_, facts) = facts_between(&a, &w);
    assert!(
        facts.iter().any(
            |f| matches!(f, Fact::ToolGone { item, uses_left: 40 } if item == "stone_pickaxe")
        ),
        "{facts:?}"
    );
    assert!(
        !facts.iter().any(|f| matches!(f, Fact::ToolBroke { .. })),
        "{facts:?}"
    );
    let gone = facts
        .iter()
        .find(|f| matches!(f, Fact::ToolGone { .. }))
        .map(Fact::line);
    assert_eq!(
        gone.as_deref(),
        Some("tool gone: stone_pickaxe with 40 uses left (dropped or lost, not broken)")
    );
    // Two pickaxes: one swapped out of its slot for the other is no loss at all.
    let mut w = World::new();
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 130));
    put(&mut w, 1, worn(ItemKind::StonePickaxe, 5));
    let a = snapshot(&w);
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 5));
    put(&mut w, 1, worn(ItemKind::StonePickaxe, 130));
    let (_, facts) = facts_between(&a, &w);
    assert!(
        !facts.iter().any(|f| matches!(
            f,
            Fact::ToolBroke { .. } | Fact::ToolGone { .. } | Fact::Item { .. }
        )),
        "{facts:?}"
    );
    // Uses run out in the slot (a stack the server leaves at its limit): broken.
    let mut w = World::new();
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 130));
    let a = snapshot(&w);
    put(&mut w, 0, worn(ItemKind::StonePickaxe, 131));
    let (_, facts) = facts_between(&a, &w);
    assert!(
        facts.iter().any(|f| matches!(f, Fact::ToolBroke { .. })),
        "{facts:?}"
    );
}

fn block(w: &mut World, pos: (i32, i32, i32), kind: BlockKind) {
    let state = kind.info().map_or(0, |i| i.default_state);
    w.step(&Event::Packet(S2c::BlockUpdate { pos, state }));
}

/// Wall 1 (race 45204423376, lane 3): the bot at (30.5, 31.0, -18.3) was printed "30,31,-18"
/// and its blocks were read one cell off: its feet cell is (30, 31, -19), as the planner floors
/// it. The row now carries that cell, the block under it and the lava against the body.
#[test]
fn a_senses_row_carries_the_feet_cell_what_it_stands_on_and_lava_beside() {
    let mut w = World::new();
    w.player.pos = Some([30.499, 31.0, -18.3]);
    for dx in -1..=1 {
        for dz in -1..=1 {
            block(&mut w, (30 + dx, 31, -19 + dz), BlockKind::Air);
            block(&mut w, (30 + dx, 32, -19 + dz), BlockKind::Air);
        }
    }
    block(&mut w, (30, 33, -19), BlockKind::Air);
    block(&mut w, (30, 30, -19), BlockKind::Netherrack);
    let row = senses(&w, "idle", "", None);
    assert_eq!(row.cell, Some([30, 31, -19]));
    assert_eq!(row.under.as_deref(), Some("netherrack"));
    assert_eq!(row.lava, Touch::No);
    assert_eq!(row.standing(), "cell 30,31,-19 on netherrack");
    // Lava one cell north at head height: beside.
    block(&mut w, (30, 32, -20), BlockKind::Lava);
    let row = senses(&w, "idle", "", None);
    assert_eq!(row.lava, Touch::Beside);
    assert_eq!(row.standing(), "cell 30,31,-19 on netherrack, lava beside");
    // Lava in the feet's own cell: in it.
    block(&mut w, (30, 31, -19), BlockKind::Lava);
    let row = senses(&w, "idle", "", None);
    assert_eq!(row.lava, Touch::In);
    assert_eq!(row.standing(), "cell 30,31,-19 in lava over netherrack");
    // Nothing under the cell (a jump, a fall, or the box resting on the block beside).
    block(&mut w, (30, 31, -19), BlockKind::Air);
    block(&mut w, (30, 30, -19), BlockKind::Air);
    let row = senses(&w, "idle", "", None);
    assert_eq!(
        row.standing(),
        "cell 30,31,-19 over air (in the air or on an edge), lava beside"
    );
    // On soul sand the feet (y 31.875) are inside the soul sand's own cell: the planner's feet
    // cell is that block, which the row says outright.
    let mut w = World::new();
    w.player.pos = Some([0.5, 31.875, 0.5]);
    block(&mut w, (0, 31, 0), BlockKind::SoulSand);
    block(&mut w, (0, 30, 0), BlockKind::Netherrack);
    block(&mut w, (0, 32, 0), BlockKind::Water);
    let row = senses(&w, "idle", "", None);
    assert_eq!(
        row.standing(),
        "cell 0,31,0 in soul_sand over netherrack, in water"
    );
    // A row from before the cell was kept: the position floored, and nothing claimed about the
    // blocks.
    let old = observe::Senses {
        cell: None,
        feet_block: None,
        under: None,
        pos: Some([30.499, 31.0, -18.3]),
        water: Touch::No,
        ..row
    };
    assert_eq!(old.feet(), Some([30, 31, -19]));
    assert_eq!(old.standing(), "cell 30,31,-19");
}

fn health(w: &mut World, health: f32) {
    w.step(&Event::Packet(S2c::SetHealth {
        health,
        food: 20,
        saturation: 5.0,
    }));
}

fn damage(w: &mut World) {
    w.step(&Event::Packet(S2c::DamageEvent {
        entity_id: 1,
        source_type: 0,
        cause: None,
        direct: None,
    }));
}

/// Race 45335627948 (lanes 1 and 2): health fell by 5 with no hit in the ledger. A fall with a
/// damage event of its own is a hit; one without is a fact of its own, with what was nearest
/// and whether the bot burned; a second fall does not borrow the first one's event.
#[test]
fn a_fall_in_health_with_no_damage_event_is_a_fact_of_its_own() {
    let mut w = World::new();
    w.player.entity_id = Some(1);
    w.player.pos = Some([0.0, 0.0, 0.0]);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 9,
        uuid: [0; 16],
        kind: EntityKind::Blaze.id(),
        x: 2.0,
        y: 0.0,
        z: 0.0,
    }));
    health(&mut w, 20.0);
    let a = snapshot(&w);
    // A blow with its event: a hit, and no silent fall.
    w.ticks = 10;
    damage(&mut w);
    health(&mut w, 17.0);
    let (b, facts) = facts_between(&a, &w);
    assert!(
        facts.iter().any(|f| matches!(f, Fact::Hit { .. })),
        "{facts:?}"
    );
    assert!(
        !facts.iter().any(|f| matches!(f, Fact::HealthDrop { .. })),
        "{facts:?}"
    );
    // Health falls again a tick later with no event: told, with the blaze beside.
    w.ticks = 11;
    health(&mut w, 12.0);
    let (c, facts) = facts_between(&b, &w);
    let drop = facts
        .iter()
        .find(|f| matches!(f, Fact::HealthDrop { .. }))
        .map(Fact::line);
    assert_eq!(
        drop.as_deref(),
        Some(
            "health dropped 5.0 to 12.0 with no damage event; nearest hostile blaze at 2.0; on fire no"
        ),
        "{facts:?}"
    );
    // The event a tick before its health (two packets, two ticks) still answers it.
    w.ticks = 40;
    damage(&mut w);
    let (d, facts) = facts_between(&c, &w);
    assert!(facts.iter().any(|f| matches!(f, Fact::Hit { .. })));
    w.ticks = 41;
    health(&mut w, 9.0);
    let (_, facts) = facts_between(&d, &w);
    assert!(
        !facts.iter().any(|f| matches!(f, Fact::HealthDrop { .. })),
        "{facts:?}"
    );
}

/// The world prunes its list of blows as it grows: in a long fight the list's length stands
/// still, and a blow told by the length alone went unseen. Each new blow is a fact.
#[test]
fn a_blow_is_seen_when_the_list_of_blows_does_not_grow() {
    let mut w = World::new();
    w.player.entity_id = Some(1);
    damage(&mut w);
    let a = snapshot(&w);
    // The old blow pruned and a new one pushed in one tick: one in the list before and after.
    w.hits.clear();
    w.session.server_ticks += 20;
    damage(&mut w);
    assert_eq!(w.hits.len(), 1);
    let (b, facts) = facts_between(&a, &w);
    assert_eq!(
        facts
            .iter()
            .filter(|f| matches!(f, Fact::Hit { .. }))
            .count(),
        1,
        "{facts:?}"
    );
    // Nothing new: nothing told twice.
    let (_, facts) = facts_between(&b, &w);
    assert!(facts.is_empty(), "{facts:?}");
}

fn nether_bot_at(x: f64, z: f64) -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 7,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 3,
        dimension: "minecraft:the_nether".to_owned(),
        game_type: 0,
    }));
    w.step(&Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x,
        y: 64.0,
        z,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    }));
    w
}

/// A Nether column of one biome everywhere (sections of air).
fn column_of(biome: &str) -> std::sync::Arc<protocol::chunk::Column> {
    use protocol::chunk::{Column, Paletted, Section};
    let id = data::generated::registries::BIOME
        .iter()
        .position(|b| *b == biome)
        .unwrap() as u32;
    let section = Section {
        block_count: 0,
        fluid_count: 0,
        blocks: Paletted::Single(0),
        biomes: Paletted::Single(id),
    };
    std::sync::Arc::new(Column {
        min_y: 0,
        sections: vec![section; 16],
    })
}

/// Issue 17b: the feet biome is a fact when it changes, and the senses row carries it.
#[test]
fn a_new_feet_biome_is_a_fact() {
    let mut w = nether_bot_at(8.5, 8.5);
    w.columns.insert(
        world::ChunkPos { x: 0, z: 0 },
        column_of("soul_sand_valley"),
    );
    w.columns
        .insert(world::ChunkPos { x: 1, z: 0 }, column_of("warped_forest"));
    assert_eq!(w.feet_biome(), Some("soul_sand_valley"));
    let a = snapshot(&w);
    w.body.as_mut().unwrap().pos[0] = 20.5;
    assert_eq!(w.feet_biome(), Some("warped_forest"));
    let mut b = snapshot(&w);
    let facts: Vec<Fact> = diff(&a, &mut b, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts
            .iter()
            .any(|f| matches!(f, Fact::Biome { name } if name == "warped_forest")),
        "{facts:?}"
    );
    let row = senses(&w, "t", "w", None);
    assert_eq!(row.biome.as_deref(), Some("warped_forest"));
    assert!(row.travel().contains("biome warped_forest"));
}

/// Issue 17b: an enderman first seen is a fact with whether it ticks; walking near enough
/// makes it tick (a fact again); one that ticks and stands still for `STILL_TICKS` is noted.
#[test]
fn an_enderman_seen_and_ticking_is_in_the_ledger() {
    let mut w = nether_bot_at(0.5, 0.5);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 11,
        uuid: [1; 16],
        kind: EntityKind::Enderman.id(),
        x: 70.0,
        y: 64.0,
        z: 0.5,
    }));
    let a = snapshot(&w);
    let mut b = snapshot(&w);
    let none = observe::Snapshot {
        endermen: Default::default(),
        ..a.clone()
    };
    let facts: Vec<Fact> = diff(&none, &mut b, &w)
        .into_iter()
        .map(|s| s.fact)
        .collect();
    assert!(
        facts.iter().any(|f| matches!(
            f,
            Fact::Enderman {
                id: 11,
                ticking: false,
                ..
            }
        )),
        "chunk 4 is beyond 3: frozen; {facts:?}"
    );
    let row = senses(&w, "t", "w", None);
    assert_eq!(row.enderman_ticking, None);
    assert!(row.enderman_frozen.is_some());
    // The bot walks 20 east: the enderman's chunk is 3 from the bot's now.
    w.body.as_mut().unwrap().pos[0] = 20.5;
    let mut c = snapshot(&w);
    let facts: Vec<Fact> = diff(&b, &mut c, &w).into_iter().map(|s| s.fact).collect();
    assert!(
        facts.iter().any(|f| matches!(
            f,
            Fact::Enderman {
                id: 11,
                ticking: true,
                ..
            }
        )),
        "{facts:?}"
    );
    // It ticks and never moves: one note once STILL_TICKS have passed, and only one.
    let mut prev = c;
    let mut still = 0;
    for _ in 0..40 {
        w.session.server_ticks += 20;
        let mut next = snapshot(&w);
        still += diff(&prev, &mut next, &w)
            .iter()
            .filter(|s| matches!(s.fact, Fact::EndermanStill { id: 11, .. }))
            .count();
        prev = next;
    }
    assert_eq!(still, 1);
}
