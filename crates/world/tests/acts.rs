//! Issue 10a: acts become packets and the world remembers them; what the server answers is
//! matched back (the dig's sequence acknowledged, a click answered when the window moves, a
//! pickup for the bot).

use protocol::slot::Slot;
use protocol::{C2s, S2c};
use world::{Act, BlockPos, DigStep, Effect, Event, World};

fn joined() -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 7,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    }));
    w
}

#[test]
fn digs_count_their_sequence_and_acks_raise_it() {
    let mut w = joined();
    let pos = BlockPos::new(1, 64, 2);
    let a = w.step(&Event::Act(Act::Dig {
        step: DigStep::Start,
        pos,
        face: 1,
    }));
    let b = w.step(&Event::Act(Act::Dig {
        step: DigStep::Finish,
        pos,
        face: 1,
    }));
    assert_eq!(
        a,
        [Effect::Send(C2s::PlayerAction {
            status: 0,
            pos: (1, 64, 2),
            face: 1,
            sequence: 1
        })]
    );
    assert_eq!(
        b,
        [Effect::Send(C2s::PlayerAction {
            status: 2,
            pos: (1, 64, 2),
            face: 1,
            sequence: 2
        })]
    );
    w.step(&Event::Packet(S2c::BlockChangedAck { sequence: 2 }));
    w.step(&Event::Packet(S2c::BlockChangedAck { sequence: 1 }));
    assert_eq!(
        (w.sequence_sent, w.sequence_acked),
        (2, 2),
        "acks only rise"
    );
}

#[test]
fn a_click_waits_for_the_window_to_move() {
    let mut w = joined();
    w.step(&Event::Packet(S2c::ContainerSetContent {
        window: 0,
        state_id: 3,
        slots: vec![Slot::Empty; 46],
        carried: Some(Slot::Empty),
    }));
    let out = w.step(&Event::Act(Act::Click {
        slot: 36,
        button: 0,
        mode: 0,
    }));
    assert_eq!(
        out,
        [Effect::Send(C2s::ContainerClick {
            window: 0,
            state_id: 3,
            slot: 36,
            button: 0,
            mode: 0
        })]
    );
    assert!(w.click.is_some());
    // An update with the same state id does not answer it; a moved one does.
    w.step(&Event::Packet(S2c::ContainerSetSlot {
        window: 0,
        state_id: 3,
        slot: 1,
        item: Slot::Empty,
    }));
    assert!(w.click.is_some());
    w.step(&Event::Packet(S2c::ContainerSetSlot {
        window: 0,
        state_id: 4,
        slot: 1,
        item: Slot::Empty,
    }));
    assert_eq!(w.click, None);
    assert_eq!((w.clicks_sent, w.clicks_answered), (1, 1));
}

#[test]
fn holding_and_pickups() {
    let mut w = joined();
    let out = w.step(&Event::Act(Act::Hold { slot: 3 }));
    assert_eq!(out, [Effect::Send(C2s::SetCarriedItem { slot: 3 })]);
    assert_eq!(w.held, 3);
    w.step(&Event::Packet(S2c::SetHeldSlot { slot: 5 }));
    assert_eq!(w.held, 5);
    w.step(&Event::Packet(S2c::TakeItemEntity {
        item: 40,
        collector: 99,
        amount: 1,
    }));
    assert!(w.pickups.is_empty(), "another player's pickup is not ours");
    w.step(&Event::Packet(S2c::TakeItemEntity {
        item: 41,
        collector: 7,
        amount: 2,
    }));
    assert_eq!(w.pickups.len(), 1);
    assert_eq!(w.pickups[0].amount, 2);
}
