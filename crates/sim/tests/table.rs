//! Issue 11a: the model places a held block (`BlockItem.place`), opens a crafting table's menu
//! (`CraftingTableBlock`, `open_screen`), crafts in its 3x3 grid with window 0's click rules
//! (`CraftingMenu.quickMoveStack`), digs with a held tool (speed and drops from the tool's
//! component) and puts the grid and the cursor back when a menu closes.

use data::{BlockKind, ItemKind};
use protocol::slot::Slot;
use protocol::{C2s, S2c, decode};
use sim::{Config, In, Sim, Wire, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: false,
};

fn packets(out: Vec<Wire>) -> Vec<S2c> {
    out.iter()
        .filter_map(|w| decode(w.state, w.id, &w.body).ok())
        .collect()
}

fn send(sim: &mut Sim, p: C2s) -> Vec<S2c> {
    packets(sim.step(&In::Packet(p)).unwrap())
}

fn tick(sim: &mut Sim) -> Vec<S2c> {
    packets(sim.step(&In::Tick).unwrap())
}

/// Joined on the grass at (0.5, 64, 0.5), standing, holding what `give` gave.
fn joined(give: &[(ItemKind, i32)]) -> Sim {
    let world = flat_world(2, 63).unwrap();
    let mut sim = Sim::new(CONFIG, world).seeded(5);
    for g in give {
        sim.survival.give(*g);
    }
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    sim
}

fn use_on(sim: &mut Sim, pos: (i32, i32, i32), face: i32, sequence: i32) -> Vec<S2c> {
    send(
        sim,
        C2s::UseItemOn {
            hand: 0,
            pos,
            face,
            cursor: [0.5, 1.0, 0.5],
            inside: false,
            sequence,
        },
    )
}

fn click(sim: &mut Sim, window: i32, slot: i16, button: i8, mode: i32) -> Vec<S2c> {
    let state_id = sim
        .survival
        .table
        .as_ref()
        .map_or(sim.survival.state_id, |t| t.state_id);
    send(
        sim,
        C2s::ContainerClick {
            window,
            state_id,
            slot,
            button,
            mode,
        },
    )
}

fn kind_at(sim: &Sim, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

#[test]
fn a_held_table_is_placed_on_the_face_clicked() {
    let mut sim = joined(&[(ItemKind::CraftingTable, 1)]);
    let out = use_on(&mut sim, (2, 63, 0), 1, 1);
    assert_eq!(kind_at(&sim, (2, 64, 0)), Some(BlockKind::CraftingTable));
    assert!(out.iter().any(|p| matches!(
        p,
        S2c::BlockUpdate {
            pos: (2, 64, 0),
            ..
        }
    )));
    assert_eq!(out.last(), Some(&S2c::BlockChangedAck { sequence: 1 }));
    // The table left the hand; the next tick tells the client.
    let after = tick(&mut sim);
    assert!(after.iter().any(|p| matches!(
        p,
        S2c::ContainerSetSlot {
            window: 0,
            slot: 36,
            item: Slot::Empty,
            ..
        }
    )));
    // Never where the player stands.
    let mut sim = joined(&[(ItemKind::CraftingTable, 1)]);
    use_on(&mut sim, (0, 63, 0), 1, 1);
    assert_eq!(kind_at(&sim, (0, 64, 0)), Some(BlockKind::Air));
}

#[test]
fn a_wooden_pickaxe_in_the_table_then_stone() {
    let mut sim = joined(&[
        (ItemKind::CraftingTable, 1),
        (ItemKind::OakPlanks, 3),
        (ItemKind::Stick, 2),
    ]);
    use_on(&mut sim, (2, 63, 0), 1, 1);
    tick(&mut sim);
    let open = use_on(&mut sim, (2, 64, 0), 4, 2);
    let id = sim.survival.table.as_ref().map(|t| t.id).unwrap();
    assert_eq!(id, 1);
    assert!(
        open.iter()
            .any(|p| matches!(p, S2c::OpenScreen { window: 1, .. }))
    );
    assert!(open.iter().any(
        |p| matches!(p, S2c::ContainerSetContent { window: 1, slots, .. } if slots.len() == 46)
    ));
    // The inventory sits at 10-45 in the table's menu: the planks (window 0's 37) at 38.
    let planks = 38;
    let sticks = 39;
    click(&mut sim, id, planks, 0, 0);
    for cell in [1, 2, 3] {
        click(&mut sim, id, cell, 1, 0);
    }
    click(&mut sim, id, sticks, 0, 0);
    for cell in [5, 8] {
        click(&mut sim, id, cell, 1, 0);
    }
    assert_eq!(
        sim.survival.table.as_ref().unwrap().result,
        Some((ItemKind::WoodenPickaxe, 1))
    );
    let took = click(&mut sim, id, 0, 0, 1);
    assert!(took.iter().any(|p| matches!(
        p,
        S2c::ContainerSetSlot {
            window: 1,
            slot: 0,
            item: Slot::Empty,
            ..
        }
    )));
    // The pickaxe went to the inventory's far end (window 0's 44), the grid is spent.
    assert_eq!(sim.survival.slots[44], Some((ItemKind::WoodenPickaxe, 1)));
    assert!(
        sim.survival
            .table
            .as_ref()
            .unwrap()
            .grid
            .iter()
            .all(Option::is_none)
    );
    send(&mut sim, C2s::ContainerClose { window: id });
    assert!(sim.survival.table.is_none());
    // Hold it (hotbar 8 is window 0's 44) and dig the stone under the grass: a pickaxe on stone
    // is 23 ticks and drops cobblestone.
    send(&mut sim, C2s::SetCarriedItem { slot: 8 });
    let stone = (1, 62, 0);
    send(
        &mut sim,
        C2s::PlayerAction {
            status: 0,
            pos: stone,
            face: 1,
            sequence: 3,
        },
    );
    for _ in 0..23 {
        tick(&mut sim);
    }
    let finish = send(
        &mut sim,
        C2s::PlayerAction {
            status: 2,
            pos: stone,
            face: 1,
            sequence: 4,
        },
    );
    assert!(finish.iter().any(|p| matches!(
        p,
        S2c::BlockUpdate {
            pos: (1, 62, 0),
            state: 0
        }
    )));
    assert!(
        sim.survival
            .items
            .values()
            .any(|i| i.stack.0 == ItemKind::Cobblestone)
    );
}

#[test]
fn stone_by_hand_drops_nothing() {
    let mut sim = joined(&[]);
    let stone = (1, 62, 0);
    send(
        &mut sim,
        C2s::PlayerAction {
            status: 0,
            pos: stone,
            face: 1,
            sequence: 1,
        },
    );
    for _ in 0..150 {
        tick(&mut sim);
    }
    send(
        &mut sim,
        C2s::PlayerAction {
            status: 2,
            pos: stone,
            face: 1,
            sequence: 2,
        },
    );
    for _ in 0..5 {
        tick(&mut sim);
    }
    assert_eq!(kind_at(&sim, stone), Some(BlockKind::Air));
    assert!(sim.survival.items.is_empty(), "no drop by hand");
}

#[test]
fn closing_puts_the_grid_and_the_cursor_back() {
    let mut sim = joined(&[(ItemKind::OakPlanks, 4)]);
    click(&mut sim, 0, 36, 0, 0);
    click(&mut sim, 0, 1, 1, 0);
    assert_eq!(sim.survival.carried, Some((ItemKind::OakPlanks, 3)));
    send(&mut sim, C2s::ContainerClose { window: 0 });
    assert_eq!(sim.survival.carried, None);
    assert!(sim.survival.items.is_empty(), "nothing thrown");
    let held: i32 = sim
        .survival
        .slots
        .iter()
        .flatten()
        .filter(|(k, _)| *k == ItemKind::OakPlanks)
        .map(|(_, n)| n)
        .sum();
    assert_eq!(held, 4);
}

#[test]
fn tools_stack_to_one() {
    let mut sim = joined(&[(ItemKind::WoodenPickaxe, 1), (ItemKind::WoodenPickaxe, 1)]);
    tick(&mut sim);
    assert_eq!(sim.survival.slots[36], Some((ItemKind::WoodenPickaxe, 1)));
    assert_eq!(sim.survival.slots[37], Some((ItemKind::WoodenPickaxe, 1)));
}
