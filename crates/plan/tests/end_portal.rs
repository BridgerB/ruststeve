//! Issue 20c/20d (docs/problems/end-portal.md) on hand-built worlds, the pure planner with no
//! ticks: the frame's eye bit, the `end-kit` row, the standing cells for the eyes, and an eye put
//! into a frame in reach.

use data::{BlockKind, ItemKind};
use plan::end_portal::{self, KitPhase};
use plan::path::Cell;
use plan::stronghold::{self, Room};
use plan::{Aim, Plan, Task};
use protocol::S2c;
use protocol::slot::{Slot, Wear};
use sim::nether::{Stage, nether_type};
use world::{BlockPos, Event, World};

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
        dimension: stronghold::OVERWORLD.to_owned(),
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

/// The frontier's room as E1 read it (frames at the floor + 2 round a 3x3 interior, the lava pool
/// in the middle at the floor's height, air over it), with its ring centre at `c` and the feet's
/// floor at `c.1 - 2`, inside a stone-brick shell; `eyed` frames hold an eye.
fn room_world(c: Cell, eyed: &[Cell]) -> Stage {
    let mut s = Stage::over(
        (c.0 - 40, c.0 + 40),
        (c.2 - 40, c.2 + 40),
        c.1 + 40,
        c.1 + 100,
    );
    let floor = c.1 - 2;
    s.fill(
        (c.0 - 8, c.0 + 8),
        (floor - 1, floor + 6),
        (c.2 - 8, c.2 + 8),
        BlockKind::StoneBricks,
    );
    s.air((c.0 - 7, c.0 + 7), (floor, floor + 5), (c.2 - 7, c.2 + 7));
    s.set((c.0, floor, c.2), BlockKind::Lava);
    let frame = BlockKind::EndPortalFrame.info().unwrap();
    for k in -1..=1 {
        for f in [
            (c.0 + k, c.1, c.2 - 2),
            (c.0 + k, c.1, c.2 + 2),
            (c.0 - 2, c.1, c.2 + k),
            (c.0 + 2, c.1, c.2 + k),
        ] {
            // State 0 of the frame's table is `eye=true`, 4 the default `eye=false`.
            let st = if eyed.contains(&f) {
                frame.first_state
            } else {
                frame.default_state
            };
            s.set_state(f, st);
        }
    }
    s
}

fn the_room(w: &World) -> Room {
    let frames = stronghold::frames_near(w, 8);
    let mut v = frames;
    v.sort_unstable();
    let room = stronghold::room_of(&v, 2).unwrap_or_else(|| {
        // Memory may not index the test's frames: read the world.
        let mut f = Vec::new();
        for x in 1610..1650 {
            for z in -50..-20 {
                if w.block_at(BlockPos::new(x, 19, z)) == Some(BlockKind::EndPortalFrame) {
                    f.push([x, 19, z]);
                }
            }
        }
        stronghold::room_of(&f, 2).unwrap()
    });
    assert_eq!(room.frames.len(), 12, "{room:?}");
    room
}

const C: Cell = (1628, 19, -34);

#[test]
fn the_frame_eye_bit_is_read_from_the_block_report() {
    let info = BlockKind::EndPortalFrame.info().unwrap();
    // E1's probe: 9469 holds an eye, 9473 to 9476 are empty.
    assert_eq!(end_portal::frame_has_eye(info.first_state), Some(true));
    for s in info.first_state..info.first_state + 4 {
        assert_eq!(end_portal::frame_has_eye(s), Some(true), "{s}");
    }
    for s in info.first_state + 4..=info.last_state {
        assert_eq!(end_portal::frame_has_eye(s), Some(false), "{s}");
    }
    assert_eq!(end_portal::frame_has_eye(info.last_state + 1), None);
}

#[test]
fn the_end_kit_row_begins_from_the_frontier_pack() {
    let t = table();
    let row = t.until("end-kit").expect("the end-kit row");
    let counts: plan::make::Counts = [
        (ItemKind::Stick, 3),
        (ItemKind::Cobblestone, 116),
        (ItemKind::RawIron, 7),
        (ItemKind::Furnace, 1),
        (ItemKind::OakLog, 10),
    ]
    .into_iter()
    .collect();
    let near = |_: BlockKind| false;
    let c = plan::make::make_with_wear(&t, &counts, &near, true, &plan::make::Wear::default(), row);
    let plan::make::Choice::Act(a) = c else {
        panic!("{c:?}");
    };
    assert!(a.id.starts_with("craft:"), "{}", a.id);
    // Held: done.
    let done: plan::make::Counts = [
        (ItemKind::StonePickaxe, 3),
        (ItemKind::IronSword, 1),
        (ItemKind::IronBoots, 1),
    ]
    .into_iter()
    .collect();
    assert_eq!(
        plan::make::make_with_wear(&t, &done, &near, true, &plan::make::Wear::default(), row),
        plan::make::Choice::Done
    );
}

#[test]
fn the_empty_frames_and_their_standing_cells() {
    let eyed = [(1627, 19, -32)];
    let w = load(&room_world(C, &eyed), [1624.5, 17.0, -37.5]);
    let room = the_room(&w);
    let empty = end_portal::empty_frames_now(&w, &room);
    assert_eq!(empty.len(), 11, "{empty:?}");
    assert!(!empty.contains(&[1627, 19, -32]));
    for f in &empty {
        let stands = end_portal::eye_stands(&w, &room, *f, (1624, 17, -37), 4.5);
        assert!(!stands.is_empty(), "no stand for {f:?}");
        for s in stands {
            assert!(
                (s.0 - C.0).abs() > 2 || (s.2 - C.2).abs() > 2,
                "{s:?} in the ring"
            );
            let eye = [
                f64::from(s.0) + 0.5,
                f64::from(s.1) + physics::EYE_HEIGHT,
                f64::from(s.2) + 0.5,
            ];
            assert!(end_portal::to_frame_top(eye, *f) <= 4.5, "{s:?} {f:?}");
            for dx in -1..=1 {
                for dz in -1..=1 {
                    for dy in -1..=0 {
                        let k = w.block_at(BlockPos::new(s.0 + dx, s.1 + dy, s.2 + dz));
                        assert_ne!(k, Some(BlockKind::Lava), "{s:?} by the lava");
                    }
                }
            }
        }
    }
}

#[test]
fn an_eye_goes_into_the_frame_in_reach() {
    let w0 = room_world(C, &[]);
    // South of the south side, on the floor: every south frame and the near side ones in reach.
    let mut w = load(&w0, [1628.5, 17.0, -37.5]);
    give(&mut w, 0, ItemKind::EnderEye, 13);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    let mut p = Plan::with_table(Aim::Race, table());
    p.end_portal.begun = true;
    p.end_portal.phase = KitPhase::Over;
    p.end_portal.spawner_done = true;
    let t = p.end_portal_task(&w, 1);
    assert!(matches!(t, Some(Task::Idle)), "{t:?}");
    let acts = p.take_acts();
    assert!(
        acts.iter().any(|a| matches!(
            a,
            world::Act::UseOn { pos, face: 1 } if pos.y == 19
        )),
        "{acts:?} {:#?}",
        p.events.iter().map(|e| e.what.clone()).collect::<Vec<_>>()
    );
    assert!(
        p.events
            .iter()
            .any(|e| e.what.starts_with("end portal: eye into the frame at"))
    );
}

/// The ways in: by each side's frames, two steps laid outward on the floor, a start beside the
/// lower; the south middle frame's is (1628, 17, -37) and (1628, 17, -38).
#[test]
fn the_ways_in_are_laid_outside_the_frames() {
    let w = load(&room_world(C, &[]), [1624.5, 17.0, -37.5]);
    let room = the_room(&w);
    let ways = end_portal::entries(&w, &room);
    assert_eq!(ways.len(), 12, "{ways:#?}");
    let south = ways
        .iter()
        .find(|e| e.frame == [1628, 19, -36])
        .expect("the south middle frame's way");
    assert_eq!(south.high, (1628, 17, -37));
    assert_eq!(south.low, (1628, 17, -38));
    assert_eq!(south.inside, (1628, 19, -35));
    assert_eq!(south.start.2, -38);
    assert_eq!(
        south.way(),
        vec![
            (1628, 18, -38),
            (1628, 19, -37),
            (1628, 20, -36),
            (1628, 19, -35),
            (1628, 19, -34)
        ]
    );
}

/// Every frame eyed and the portal lit: the way in is chosen, the bot walks to its start, and
/// from there lays the first step.
#[test]
fn a_lit_portal_is_stepped_into_by_laid_steps() {
    let eyed: Vec<Cell> = (-1..=1)
        .flat_map(|k| {
            [
                (C.0 + k, C.1, C.2 - 2),
                (C.0 + k, C.1, C.2 + 2),
                (C.0 - 2, C.1, C.2 + k),
                (C.0 + 2, C.1, C.2 + k),
            ]
        })
        .collect();
    let mut s = room_world(C, &eyed);
    s.fill(
        (C.0 - 1, C.0 + 1),
        (C.1, C.1),
        (C.2 - 1, C.2 + 1),
        BlockKind::EndPortal,
    );
    // On the floor south-east of the ring.
    let mut w = load(&s, [1631.5, 17.0, -38.5]);
    give(&mut w, 0, ItemKind::Cobblestone, 64);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    let mut p = Plan::with_table(Aim::Race, table());
    p.end_portal.begun = true;
    p.end_portal.phase = KitPhase::Over;
    p.end_portal.spawner_done = true;
    let t = p.end_portal_task(&w, 1);
    let what: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(what.iter().any(|n| n == "end portal: lit"), "{what:#?}");
    assert!(
        what.iter()
            .any(|n| n.starts_with("end portal: the way in by the frame at")),
        "{what:#?}"
    );
    let e = p.end_portal.entry.clone().expect("a way chosen");
    if (1631, 17, -39) == e.start || w.body.map(|b| plan::path::feet_cell(b.pos)) == Some(e.start) {
        assert!(matches!(t, Some(Task::Idle)), "{t:?}");
    } else {
        assert!(matches!(t, Some(Task::Explore(_))), "{t:?} {what:#?}");
    }
}

/// The climb back up a downward leg: level first, then a step up a block (the bot's leg 7 ran
/// down from (1602, 27, -38) 4 stairs east, then 3 level steps to (1609, 23, -38)).
#[test]
fn the_climb_route_reverses_a_downward_leg() {
    let r = end_portal::climb_route((1609, 23, -38), (1602, 27, -38));
    assert_eq!(
        r,
        vec![
            (1609, 23, -38),
            (1608, 23, -38),
            (1607, 23, -38),
            (1606, 23, -38),
            (1605, 24, -38),
            (1604, 25, -38),
            (1603, 26, -38),
            (1602, 27, -38)
        ]
    );
    // Off the line by one: across first.
    let r = end_portal::climb_route((1600, 24, -37), (1596, 27, -38));
    assert_eq!(r[1], (1600, 24, -38));
    assert_eq!(*r.last().unwrap(), (1596, 27, -38));
    assert!(r.windows(2).all(|p| {
        let (a, b) = (p[0], p[1]);
        (a.0 - b.0).abs() + (a.2 - b.2).abs() == 1 && (0..=1).contains(&(b.1 - a.1))
    }));
}

/// On a frame's top beside the lit portal: no way in is chosen, the push goes straight in.
#[test]
fn on_a_frame_beside_the_lit_portal_the_bot_pushes_in() {
    let eyed: Vec<Cell> = (-1..=1)
        .flat_map(|k| {
            [
                (C.0 + k, C.1, C.2 - 2),
                (C.0 + k, C.1, C.2 + 2),
                (C.0 - 2, C.1, C.2 + k),
                (C.0 + 2, C.1, C.2 + k),
            ]
        })
        .collect();
    let mut s = room_world(C, &eyed);
    s.fill(
        (C.0 - 1, C.0 + 1),
        (C.1, C.1),
        (C.2 - 1, C.2 + 1),
        BlockKind::EndPortal,
    );
    // On the south middle frame (1628, 19, -36), its box half over the portal cell's edge.
    let mut w = load(&s, [1628.5, 19.8125, -35.9]);
    give(&mut w, 0, ItemKind::Cobblestone, 64);
    w.stage_reached = Some(world::stage::Stage::PortalRoom);
    let room = the_room(&w);
    assert!(end_portal::on_frame_by_portal(
        &w,
        &room,
        [1628.5, 19.8125, -35.9]
    ));
    assert!(!end_portal::on_frame_by_portal(
        &w,
        &room,
        [1628.5, 17.0, -37.5]
    ));
    let mut p = Plan::with_table(Aim::Race, table());
    p.end_portal.begun = true;
    p.end_portal.phase = KitPhase::Over;
    p.end_portal.spawner_done = true;
    let t = p.end_portal_task(&w, 1);
    assert!(matches!(t, Some(Task::Enter(_))), "{t:?}");
    let what: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        what.iter()
            .any(|n| n.starts_with("end portal: into the portal by push")),
        "{what:#?}"
    );
    assert!(p.end_portal.entry.is_none(), "no way in chosen");
}

/// A dimension change ends the task in hand (an escape from the overworld walked a bot off the
/// End's platform, race 45887747475 lane 1), noted once.
#[test]
fn a_dimension_change_ends_the_task_in_hand() {
    let mut w = load(&room_world(C, &[]), [1624.5, 17.0, -37.5]);
    let mut p = Plan::with_table(Aim::Hold, table());
    let _ = p.step(&w);
    let c = (1624, 17, -30);
    p.task = Task::Explore(plan::walk::Walk::new(
        c,
        plan::path::Goal::Block { pos: c },
        plan::walk::Arrive::Goal,
        1,
    ));
    w.dimension = Some(world::Dimension {
        name: "minecraft:the_end".to_owned(),
        ..w.dimension.clone().unwrap()
    });
    let _ = p.step(&w);
    let what: Vec<String> = p.events.iter().map(|e| e.what.clone()).collect();
    assert!(
        what.iter().any(|n| n == "dimension changed (task ended)"),
        "{what:#?}"
    );
    assert!(!matches!(p.task, Task::Explore(_)), "{:?}", p.task.name());
}

/// Lava two under the lit portal's cells is no trap at the door.
#[test]
fn over_the_lit_portal_is_not_at_the_door() {
    let mut s = room_world(C, &[]);
    s.fill(
        (C.0 - 1, C.0 + 1),
        (C.1, C.1),
        (C.2 - 1, C.2 + 1),
        BlockKind::EndPortal,
    );
    let w = load(&s, [1628.5, 19.2, -34.5]);
    assert!(end_portal::over_portal(&w, [1628.5, 19.2, -34.5]));
    assert!(!end_portal::over_portal(&w, [1624.5, 17.0, -38.5]));
}

/// A creeper near in the room: the step away opens the distance from where it is now, never
/// toward it, never onto lava (wall 12, race 45886255606 lane 0).
#[test]
fn a_creeper_near_is_stepped_away_from() {
    let w = load(&room_world(C, &[]), [1631.5, 17.0, -37.5]);
    let body = w.body.unwrap();
    // The creeper west of the bot along the ring's south side.
    let foe = [1628.5, 17.0, -37.5];
    let yaw = plan::defend::away_step(&w, &body, foe).expect("a step away");
    let rad = yaw.to_radians();
    let p = [body.pos[0] - rad.sin() * 1.2, body.pos[2] + rad.cos() * 1.2];
    let before = (body.pos[0] - foe[0]).hypot(body.pos[2] - foe[2]);
    let after = (p[0] - foe[0]).hypot(p[1] - foe[2]);
    assert!(after > before + 0.3, "yaw {yaw}: {before} to {after}");
    // Boxed in on three sides with the creeper on the fourth: no step.
    let mut s = room_world(C, &[]);
    s.fill((1630, 1633), (17, 18), (-40, -36), BlockKind::StoneBricks);
    s.air((1630, 1631), (17, 18), (-38, -38));
    let w = load(&s, [1631.5, 17.0, -37.5]);
    let body = w.body.unwrap();
    assert_eq!(
        plan::defend::away_step(&w, &body, [1629.5, 17.0, -37.5]),
        None
    );
}

// >>> E8 (issue 20, the bow line)
/// The bow kit's row from the armed snapshot's pack (race 45882199358 lane 0: 1 log, 8 planks,
/// 15 gravel, 3 stone pickaxes, 90 cobblestone, a table) with 3 string and 2 feathers got: a
/// step the planner can make; with the kit held, met; with no string or feathers only the
/// pickaxe and the sticks are asked for.
#[test]
fn the_bow_row_is_makeable_from_the_armed_pack() {
    let t = table();
    let r = t.end_kit.clone();
    assert!(r.bow, "the bow line runs the bow phase");
    let near = |b: BlockKind| b == BlockKind::CraftingTable;
    let pack: plan::make::Counts = [
        (ItemKind::OakLog, 1),
        (ItemKind::OakPlanks, 8),
        (ItemKind::Gravel, 15),
        (ItemKind::StonePickaxe, 3),
        (ItemKind::Cobblestone, 90),
        (ItemKind::CraftingTable, 1),
        (ItemKind::String, 3),
        (ItemKind::Feather, 2),
    ]
    .into_iter()
    .collect();
    let row = end_portal::bow_row(&r, false, 0, 3, 2);
    assert_eq!(row.provides.len(), 4, "bow, arrows, pickaxe, sticks");
    let c = plan::make::make_with_wear(&t, &pack, &near, true, &plan::make::Wear::default(), &row);
    assert!(matches!(c, plan::make::Choice::Act(_)), "{c:?}");
    let bare = end_portal::bow_row(&r, false, 0, 0, 0);
    assert_eq!(
        bare.provides.len(),
        2,
        "no string, no feathers: pickaxe and sticks"
    );
    let held: plan::make::Counts = [
        (ItemKind::Bow, 1),
        (ItemKind::Arrow, 32),
        (ItemKind::StonePickaxe, 4),
        (ItemKind::Stick, 16),
    ]
    .into_iter()
    .collect();
    let done = end_portal::bow_row(&r, true, 32, 0, 0);
    assert_eq!(
        plan::make::make_with_wear(&t, &held, &near, true, &plan::make::Wear::default(), &done),
        plan::make::Choice::Done
    );
}
// <<< E8
