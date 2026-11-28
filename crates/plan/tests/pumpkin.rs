//! Issue 80 (docs/problems/pumpkin.md) on hand-built worlds, the pure planner with no ticks: the
//! End kit's pumpkin phase (a pumpkin seen with an exposed face walked to, sheared, the carved
//! pumpkin dug and picked up; an unexposed one never chosen), the head slot (the pumpkin on, the
//! iron helmet held and never put back over it), and the gaze guard's backstop count.

use data::{BlockKind, EntityKind, ItemKind};
use physics::Controls;
use plan::end_portal::{KitPhase, shears_first};
use plan::look::{self, Kept, count_spared, spared};
use plan::path::Cell;
use plan::{Aim, Plan, Task};
use protocol::S2c;
use protocol::slot::{Slot, Wear};
use sim::nether::{Stage, nether_type};
use world::{Act, Event, World};

fn table() -> std::sync::Arc<plan::make::Table> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: plan::make::Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    std::sync::Arc::new(plan::make::Table::new(&goals).unwrap())
}

fn load(stage: &Stage, at: [f64; 3]) -> World {
    let model = stage.build().unwrap();
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: nether_type(),
        dimension: plan::stronghold::OVERWORLD.to_owned(),
        game_type: 0,
    }));
    for ((x, z), c) in &model.columns {
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: *x,
            z: *z,
            column: c.column.clone(),
        }));
    }
    w.step(&Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x: at[0],
        y: at[1],
        z: at[2],
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    }));
    w
}

fn give(w: &mut World, slot: i32, item: ItemKind, n: i32) {
    w.step(&Event::Packet(S2c::SetPlayerInventory {
        slot,
        item: Slot::Item {
            item: item.id(),
            count: n,
            wear: Wear {
                damage: None,
                max_damage: None,
            },
        },
    }));
}

/// The stairs' top of `[end_kit]` (1608, 63, -32): a floor whose top is y 63, air over it.
fn ground() -> Stage {
    Stage::over((1580, 1640), (-60, -4), 63, 100)
}

/// The bot by the stairs' top, in the kit's pumpkin phase.
fn in_phase() -> Plan {
    let mut p = Plan::with_table(Aim::Race, table());
    p.end_portal.begun = true;
    p.end_portal.phase = KitPhase::Pumpkin;
    p
}

fn whats(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

const FEET: [f64; 3] = [1608.5, 64.0, -31.5];

#[test]
fn a_seen_pumpkin_in_reach_is_sheared_on_its_face() {
    let c: Cell = (1611, 64, -32);
    let mut s = ground();
    s.set(c, BlockKind::Pumpkin);
    let mut w = load(&s, FEET);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    give(&mut w, 0, ItemKind::Shears, 1);
    let mut p = in_phase();
    let t = p.end_portal_task(&w, 1);
    assert!(matches!(t, Some(Task::Idle)), "{t:?} {:#?}", whats(&p));
    let ev = whats(&p);
    assert!(
        ev.iter().any(|e| e == "pumpkin seen at (1611, 64, -32)"),
        "{ev:#?}"
    );
    assert!(ev.iter().any(|e| e.starts_with("shears made")), "{ev:#?}");
    let acts = p.take_acts();
    assert!(
        acts.iter().any(|a| matches!(
            a,
            Act::UseOn { pos, .. } if (pos.x, pos.y, pos.z) == c
        )),
        "{acts:?} {ev:#?}"
    );
    assert_eq!(p.end_portal.pumpkin_at, Some(c));
}

#[test]
fn a_seen_pumpkin_out_of_reach_is_walked_to() {
    let c: Cell = (1620, 64, -32);
    let mut s = ground();
    s.set(c, BlockKind::Pumpkin);
    let mut w = load(&s, FEET);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    give(&mut w, 0, ItemKind::Shears, 1);
    let mut p = in_phase();
    let t = p.end_portal_task(&w, 1);
    assert!(
        matches!(t, Some(Task::Explore(_))),
        "{t:?} {:#?}",
        whats(&p)
    );
    assert!(
        whats(&p)
            .iter()
            .any(|e| e.starts_with("end kit: walk to the pumpkin at (1620, 64, -32)"))
    );
}

#[test]
fn the_carved_pumpkin_is_dug_then_its_pickup_ends_the_phase() {
    let c: Cell = (1611, 64, -32);
    let mut s = ground();
    s.set(c, BlockKind::CarvedPumpkin);
    let mut w = load(&s, FEET);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    give(&mut w, 0, ItemKind::Shears, 1);
    let mut p = in_phase();
    p.end_portal.pumpkin_at = Some(c);
    let t = p.end_portal_task(&w, 1);
    assert!(matches!(t, Some(Task::ToLog(_))), "{t:?} {:#?}", whats(&p));
    let ev = whats(&p);
    assert!(
        ev.iter().any(|e| e == "pumpkin carved at (1611, 64, -32)"),
        "{ev:#?}"
    );
    // Picked up: the phase is over and the way down is next.
    give(&mut w, 1, ItemKind::CarvedPumpkin, 1);
    let _ = p.end_portal_task(&w, 2);
    let ev = whats(&p);
    assert!(
        ev.iter().any(|e| e == "carved pumpkin picked up"),
        "{ev:#?}"
    );
    assert!(
        ev.iter().any(|e| e == "end kit: pumpkin over, down next"),
        "{ev:#?}"
    );
    assert_ne!(p.end_portal.phase, KitPhase::Pumpkin);
}

#[test]
fn an_unexposed_pumpkin_is_never_chosen() {
    // Buried in the floor, every face against the floor's blocks: no sighting (no X-ray).
    let c: Cell = (1610, 60, -32);
    let mut s = ground();
    s.set(c, BlockKind::Pumpkin);
    let mut w = load(&s, FEET);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    give(&mut w, 0, ItemKind::Shears, 1);
    let mut p = in_phase();
    let _ = p.end_portal_task(&w, 1);
    let ev = whats(&p);
    assert!(
        !ev.iter().any(|e| e.starts_with("pumpkin seen at")),
        "{ev:#?}"
    );
    assert_eq!(p.end_portal.pumpkin_at, None);
    assert!(p.end_portal.pumpkins_seen.is_empty());
}

#[test]
fn with_too_little_iron_the_phase_gives_up_with_a_note() {
    let mut s = ground();
    s.set((1611, 64, -32), BlockKind::Pumpkin);
    let mut w = load(&s, FEET);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    give(&mut w, 0, ItemKind::IronIngot, 1);
    let mut p = in_phase();
    let _ = p.end_portal_task(&w, 1);
    let ev = whats(&p);
    assert!(
        ev.iter()
            .any(|e| e == "pumpkin given up: 1 iron held, the shears want 2"),
        "{ev:#?}"
    );
    assert_ne!(p.end_portal.phase, KitPhase::Pumpkin);
}

#[test]
fn the_end_kit_row_keeps_the_shears_iron_ahead_of_the_boots() {
    let t = table();
    let row = t.until("end-kit").unwrap();
    let out = shears_first(row);
    let names: Vec<String> = out.provides.iter().map(|(n, _)| format!("{n:?}")).collect();
    let at = |s: &str| names.iter().position(|n| n.contains(s)).unwrap();
    assert!(at("Shears") < at("IronBoots"), "{names:?}");
    assert_eq!(shears_first(&out).provides.len(), out.provides.len());
}

/// The bot on the way down, the helmet worn and a carved pumpkin held.
fn on_the_way_down() -> (World, Plan) {
    let w = load(&ground(), FEET);
    let mut p = Plan::with_table(Aim::Race, table());
    p.end_portal.begun = true;
    p.end_portal.phase = KitPhase::Down;
    (w, p)
}

#[test]
fn the_pumpkin_goes_on_and_the_helmet_is_held() {
    let (mut w, mut p) = on_the_way_down();
    w.inventory.slots.insert(5, (ItemKind::IronHelmet, 1));
    w.inventory.slots.insert(21, (ItemKind::CarvedPumpkin, 1));
    assert!(p.pumpkin_leg(&w));
    assert!(p.helmet_off(&w));
    // The helmet off first.
    assert!(p.pumpkin_head(&w, 1));
    let acts = p.take_acts();
    assert!(
        acts.contains(&Act::Click {
            slot: 5,
            button: 0,
            mode: 1
        }),
        "{acts:?}"
    );
    // The helmet in the pack, the head empty: never back on, the pumpkin goes on.
    w.inventory.slots.remove(&5);
    w.inventory.slots.insert(20, (ItemKind::IronHelmet, 1));
    assert_eq!(plan::unworn_armour(&w.inventory.slots), Some(20));
    assert_eq!(
        plan::unworn_armour_kept(&w.inventory.slots, p.helmet_off(&w)),
        None
    );
    assert!(p.pumpkin_head(&w, 30));
    let acts = p.take_acts();
    assert!(
        acts.contains(&Act::Click {
            slot: 21,
            button: 0,
            mode: 1
        }),
        "{acts:?}"
    );
    // Worn: noted once, nothing more clicked.
    w.inventory.slots.remove(&21);
    w.inventory.slots.insert(5, (ItemKind::CarvedPumpkin, 1));
    assert!(!p.pumpkin_head(&w, 60));
    assert!(!p.pumpkin_head(&w, 90));
    let worn = whats(&p)
        .iter()
        .filter(|e| *e == "pumpkin worn (helmet held)")
        .count();
    assert_eq!(worn, 1, "{:#?}", whats(&p));
    assert!(look::pumpkin_worn(&w));
}

#[test]
fn before_the_way_down_the_helmet_stays_on() {
    let (mut w, mut p) = on_the_way_down();
    p.end_portal.phase = KitPhase::Pumpkin;
    w.inventory.slots.insert(5, (ItemKind::IronHelmet, 1));
    w.inventory.slots.insert(21, (ItemKind::CarvedPumpkin, 1));
    assert!(!p.pumpkin_leg(&w));
    assert!(!p.helmet_off(&w));
    assert!(!p.pumpkin_head(&w, 1));
    assert!(p.take_acts().is_empty());
}

fn enderman_world(feet: [f64; 3], at: [f64; 3]) -> World {
    let mut w = World::new();
    w.body = Some(physics::Body {
        pos: feet,
        ..physics::Body::default()
    });
    w.step(&Event::Packet(S2c::AddEntity {
        id: 41,
        uuid: [0; 16],
        kind: EntityKind::Enderman.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
    w
}

#[test]
fn with_the_pumpkin_on_the_guard_counts_rather_than_bends() {
    let feet = [0.5, 64.0, 0.5];
    let at = [10.5, 64.0, 0.5];
    let eye = [feet[0], feet[1] + physics::EYE_HEIGHT, feet[2]];
    let (yaw, pitch) = plan::walk::look_at(eye, [at[0], at[1] + look::ENDERMAN_EYE, at[2]]);
    let c = Controls {
        yaw,
        pitch: Some(pitch),
        ..Controls::default()
    };
    let mut w = enderman_world(feet, at);
    // Bare-headed: bent, nothing spared.
    let (bent, why) = look::keep_off(&w, c);
    assert_eq!(why, Some(41));
    assert_ne!(bent.pitch, c.pitch);
    assert_eq!(spared(&w, &c, None), None);
    // The pumpkin on: sent as asked, and counted as the stare it spared.
    w.inventory.slots.insert(5, (ItemKind::CarvedPumpkin, 1));
    let (sent, why) = look::keep_off(&w, c);
    assert_eq!(why, None);
    assert_eq!(sent, c);
    assert_eq!(spared(&w, &c, None), Some((41, Kept::Stare)));
}

#[test]
fn the_spared_count_notes_the_first_and_every_nth() {
    let mut counts = Vec::new();
    let noted: Vec<u64> = (0..2 * look::SPARED_NOTE_EVERY)
        .filter_map(|_| count_spared(&mut counts, 41))
        .collect();
    assert_eq!(
        noted,
        vec![1, look::SPARED_NOTE_EVERY, 2 * look::SPARED_NOTE_EVERY]
    );
    assert_eq!(
        count_spared(&mut counts, 7),
        Some(1),
        "a count per enderman"
    );
    assert_eq!(counts.len(), 2);
}
