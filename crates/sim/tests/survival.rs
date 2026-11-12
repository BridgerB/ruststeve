//! Issue 10b: the model digs as ServerPlayerGameMode does (FINISH accepted from 0.7 of the
//! progress, an early FINISH a delayed destroy), drops and picks up as Block.popResource and
//! Player.aiStep do, and crafts in window 0 as the menu does, answering a click that predicted
//! nothing with the server's slots.

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

/// Joined on the grass at (0.5, 64, 0.5), standing (on the ground), with an oak log at
/// (1, 64, 0) beside the feet.
fn joined() -> (Sim, Vec<S2c>) {
    let mut world = flat_world(2, 63).unwrap();
    let log = BlockKind::OakLog.info().unwrap().default_state;
    world.set_block(1, 64, 0, log).unwrap();
    let mut sim = Sim::new(CONFIG, world).seeded(7);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    let join = send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    (sim, join)
}

fn dig(sim: &mut Sim, status: i32, sequence: i32) -> Vec<S2c> {
    send(
        sim,
        C2s::PlayerAction {
            status,
            pos: (1, 64, 0),
            face: 1,
            sequence,
        },
    )
}

fn broken(out: &[S2c]) -> bool {
    out.iter().any(|p| {
        matches!(
            p,
            S2c::BlockUpdate {
                pos: (1, 64, 0),
                state: 0
            }
        )
    })
}

fn click(sim: &mut Sim, slot: i16, button: i8, mode: i32) -> Vec<S2c> {
    let state_id = sim.survival.state_id;
    send(
        sim,
        C2s::ContainerClick {
            window: 0,
            state_id,
            slot,
            button,
            mode,
        },
    )
}

fn stack(sim: &Sim, slot: usize) -> Option<(ItemKind, i32)> {
    sim.survival.slots[slot]
}

#[test]
fn the_join_sends_window_zero_at_state_one() {
    let (_, join) = joined();
    assert!(
        join.iter()
            .any(|p| matches!(p, S2c::SetHeldSlot { slot: 0 }))
    );
    assert!(join.iter().any(|p| matches!(
        p,
        S2c::ContainerSetContent { window: 0, state_id: 1, slots, .. } if slots.len() == 46
    )));
}

#[test]
fn a_log_by_hand_breaks_on_a_finish_from_tick_41() {
    // 1 / hardness 2 / 30 = 1/60 a tick; FINISH is accepted once (ticks + 1) / 60 >= 0.7.
    let (mut sim, _) = joined();
    let start = dig(&mut sim, 0, 1);
    assert!(start.contains(&S2c::BlockChangedAck { sequence: 1 }));
    assert!(!broken(&start));
    for _ in 0..41 {
        tick(&mut sim);
    }
    let finish = dig(&mut sim, 2, 2);
    assert!(broken(&finish), "{finish:?}");
    assert_eq!(finish.last(), Some(&S2c::BlockChangedAck { sequence: 2 }));
    assert!(
        finish
            .iter()
            .any(|p| matches!(p, S2c::AddEntity { kind: 71, .. }))
    );
    assert!(finish.iter().any(|p| matches!(
        p,
        S2c::SetEntityData { entries, .. } if entries.iter().any(|(i, _)| *i == 8)
    )));
}

#[test]
fn an_early_finish_is_a_delayed_destroy_not_an_abort() {
    let (mut sim, _) = joined();
    dig(&mut sim, 0, 1);
    for _ in 0..30 {
        tick(&mut sim);
    }
    assert!(!broken(&dig(&mut sim, 2, 2)), "refused at tick 30");
    // The server breaks it itself once the progress reaches 1: tick 59 after the start.
    let broke_at = (31..=70).find(|t| {
        let out = tick(&mut sim);
        broken(&out) && *t > 0
    });
    assert_eq!(broke_at, Some(59));
}

#[test]
fn the_drop_is_picked_up_into_the_hotbar_after_the_delay() {
    let (mut sim, _) = joined();
    dig(&mut sim, 0, 1);
    for _ in 0..41 {
        tick(&mut sim);
    }
    dig(&mut sim, 2, 2);
    let mut seen = Vec::new();
    for _ in 0..40 {
        seen.extend(tick(&mut sim));
    }
    let take = seen.iter().position(|p| {
        matches!(
            p,
            S2c::TakeItemEntity {
                collector: 1,
                amount: 1,
                ..
            }
        )
    });
    assert!(take.is_some(), "{seen:?}");
    assert!(seen.iter().any(|p| matches!(
        p,
        S2c::ContainerSetSlot {
            window: 0,
            state_id: 2,
            slot: 36,
            item: Slot::Item { count: 1, .. }
        }
    )));
    assert_eq!(stack(&sim, 36), Some((ItemKind::OakLog, 1)));
}

/// Pick up from `from` and put down at `to`, as the craft task does: one click per round trip.
fn carry(sim: &mut Sim, from: i16, to: i16, button: i8) {
    click(sim, from, 0, 0);
    click(sim, to, button, 0);
}

#[test]
fn planks_sticks_and_a_table_in_the_grid() {
    let (mut sim, _) = joined();
    sim.survival.give((ItemKind::OakLog, 1));
    // The log into the grid: the result appears first, with its own state id.
    click(&mut sim, 36, 0, 0);
    let put = click(&mut sim, 1, 0, 0);
    assert!(matches!(
        put.first(),
        Some(S2c::ContainerSetSlot {
            slot: 0,
            item: Slot::Item { count: 4, .. },
            ..
        })
    ));
    assert_eq!(stack(&sim, 0), Some((ItemKind::OakPlanks, 4)));
    // Shift-click the result: four planks at the hotbar's far end, the grid empty.
    let took = click(&mut sim, 0, 0, 1);
    assert!(!took.is_empty());
    assert_eq!(stack(&sim, 44), Some((ItemKind::OakPlanks, 4)));
    assert_eq!(stack(&sim, 1), None);
    assert_eq!(stack(&sim, 0), None);
    // Two planks stacked in a column make four sticks.
    click(&mut sim, 44, 0, 0);
    click(&mut sim, 1, 1, 0);
    click(&mut sim, 3, 1, 0);
    click(&mut sim, 44, 0, 0);
    assert_eq!(stack(&sim, 0), Some((ItemKind::Stick, 4)));
    click(&mut sim, 0, 0, 1);
    assert!((9..45).any(|s| stack(&sim, s) == Some((ItemKind::Stick, 4))));
    assert_eq!(stack(&sim, 44), Some((ItemKind::OakPlanks, 2)));
}

#[test]
fn four_planks_make_a_table() {
    let (mut sim, _) = joined();
    sim.survival.give((ItemKind::OakPlanks, 4));
    for g in 1..=4 {
        carry(&mut sim, 36, g, 1);
        click(&mut sim, 36, 0, 0);
    }
    assert_eq!(stack(&sim, 0), Some((ItemKind::CraftingTable, 1)));
    click(&mut sim, 0, 0, 1);
    assert!((9..45).any(|s| stack(&sim, s) == Some((ItemKind::CraftingTable, 1))));
    assert!((1..=4).all(|g| stack(&sim, g).is_none()));
}

#[test]
fn a_click_that_changes_nothing_draws_no_reply() {
    let (mut sim, _) = joined();
    let before = sim.survival.state_id;
    assert!(click(&mut sim, 36, 0, 0).is_empty());
    assert_eq!(sim.survival.state_id, before);
}

#[test]
fn a_stale_state_id_draws_the_whole_window() {
    let (mut sim, _) = joined();
    sim.survival.give((ItemKind::OakLog, 3));
    let out = send(
        &mut sim,
        C2s::ContainerClick {
            window: 0,
            state_id: 99,
            slot: 36,
            button: 0,
            mode: 0,
        },
    );
    assert!(
        out.iter()
            .any(|p| matches!(p, S2c::ContainerSetContent { window: 0, .. }))
    );
    assert_eq!(sim.survival.carried, Some((ItemKind::OakLog, 3)));
}

#[test]
fn holding_the_cursor_is_told_by_set_cursor_item() {
    let (mut sim, _) = joined();
    sim.survival.give((ItemKind::OakLog, 3));
    tick(&mut sim);
    let out = click(&mut sim, 36, 0, 0);
    assert!(out.iter().any(|p| matches!(
        p,
        S2c::ContainerSetSlot {
            slot: 36,
            item: Slot::Empty,
            ..
        }
    )));
    assert!(out.iter().any(|p| matches!(
        p,
        S2c::SetCursorItem {
            item: Slot::Item { count: 3, .. }
        }
    )));
}

#[test]
fn leaf_litter_on_a_broken_block_pops_off_with_its_segments() {
    let mut world = flat_world(2, 63).unwrap();
    let litter = BlockKind::LeafLitter.info().unwrap();
    // segment_amount 2 of 1-4, facing north (the default's other properties).
    let two = (litter.first_state..=litter.last_state)
        .find(|s| {
            data::block_of_state(*s) == Some(BlockKind::LeafLitter)
                && sim::survival::property(*s, "segment_amount") == Some("2")
        })
        .unwrap();
    world.set_block(1, 64, 0, two).unwrap();
    let mut sim = Sim::new(CONFIG, world).seeded(3);
    for p in protocol::session::opening(775, "model", 0, "rs-test") {
        send(&mut sim, p);
    }
    send(&mut sim, C2s::LoginAcknowledged);
    send(&mut sim, C2s::SelectKnownPacks);
    send(&mut sim, C2s::FinishConfiguration);
    send(&mut sim, C2s::AcceptTeleportation { teleport_id: 1 });
    send(&mut sim, C2s::MovePlayerStatusOnly { flags: 1 });
    let at = |sim: &mut Sim, status, sequence| {
        send(
            sim,
            C2s::PlayerAction {
                status,
                pos: (1, 63, 0),
                face: 1,
                sequence,
            },
        )
    };
    at(&mut sim, 0, 1);
    for _ in 0..30 {
        tick(&mut sim);
    }
    let out = at(&mut sim, 2, 2);
    assert!(out.iter().any(|p| matches!(
        p,
        S2c::BlockUpdate {
            pos: (1, 64, 0),
            state: 0
        }
    )));
    let litter_drop = out.iter().any(|p| {
        matches!(
            p,
            S2c::SetEntityData { entries, .. }
                if entries.iter().any(|(_, v)| matches!(
                    v,
                    protocol::metadata::MetaValue::Item(Slot::Item { count: 2, .. })
                ))
        )
    });
    assert!(litter_drop, "{out:?}");
}
