//! Issue 19e/f (docs/problems/eyes-stronghold.md) on hand-built worlds, the pure planner with no
//! ticks: the portal room found from its frames, a standing cell clear of the lava, the descent
//! out of the room's footprint, infested blocks never opened; overland legs over a river on a
//! bridge, an ocean gone round, a hut at dusk, and the progress invariant's turn.
//!
//! The worlds are `sim::nether::Stage`s loaded through the packets a server sends, under the
//! overworld's name (the planner reads the dimension's name; the columns keep the Nether's shape).

use data::{BlockKind, ItemKind};
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

fn noon(w: &mut World, t: i64) {
    w.day_clock = Some(world::DayClock {
        total_ticks: t,
        game_time: 0,
        rate: 1.0,
    });
}

fn notes(p: &Plan) -> Vec<String> {
    p.events
        .iter()
        .map(|e| format!("{} ({})", e.what, e.why))
        .collect()
}

/// The portal room (StrongholdPieces$PortalRoom, Q5's probe 45685968956) with its local origin at
/// `o`: a stone-brick shell 11 x 8 x 16, frames at y 3, the pool under the ring and the side pools
/// at y 1, the stairs at z 4 to 6 and the spawner at their top.
fn portal_room(s: &mut Stage, o: Cell) {
    let at = |x: i32, y: i32, z: i32| (o.0 + x, o.1 + y, o.2 + z);
    s.fill(
        (o.0, o.0 + 10),
        (o.1, o.1 + 7),
        (o.2, o.2 + 15),
        BlockKind::StoneBricks,
    );
    s.air((o.0 + 1, o.0 + 9), (o.1 + 1, o.1 + 6), (o.2 + 1, o.2 + 14));
    s.fill(
        (o.0 + 3, o.0 + 7),
        (o.1 + 1, o.1 + 1),
        (o.2 + 8, o.2 + 12),
        BlockKind::StoneBricks,
    );
    s.fill(
        (o.0 + 4, o.0 + 6),
        (o.1 + 1, o.1 + 1),
        (o.2 + 9, o.2 + 11),
        BlockKind::Lava,
    );
    s.fill(
        (o.0 + 1, o.0 + 2),
        (o.1 + 1, o.1 + 1),
        (o.2 + 1, o.2 + 4),
        BlockKind::StoneBricks,
    );
    s.fill(
        (o.0 + 8, o.0 + 9),
        (o.1 + 1, o.1 + 1),
        (o.2 + 1, o.2 + 4),
        BlockKind::StoneBricks,
    );
    s.fill(
        (o.0 + 1, o.0 + 1),
        (o.1 + 1, o.1 + 1),
        (o.2 + 1, o.2 + 3),
        BlockKind::Lava,
    );
    s.fill(
        (o.0 + 9, o.0 + 9),
        (o.1 + 1, o.1 + 1),
        (o.2 + 1, o.2 + 3),
        BlockKind::Lava,
    );
    for k in 4..=6 {
        s.set(at(k, 1, 4), BlockKind::StoneBricks);
        s.fill(
            (o.0 + k, o.0 + k),
            (o.1 + 1, o.1 + 2),
            (o.2 + 5, o.2 + 5),
            BlockKind::StoneBricks,
        );
        s.fill(
            (o.0 + k, o.0 + k),
            (o.1 + 1, o.1 + 2),
            (o.2 + 6, o.2 + 6),
            BlockKind::StoneBricks,
        );
        s.set(at(k, 3, 8), BlockKind::EndPortalFrame);
        s.set(at(k, 3, 12), BlockKind::EndPortalFrame);
    }
    for k in 9..=11 {
        s.set(at(3, 3, k), BlockKind::EndPortalFrame);
        s.set(at(7, 3, k), BlockKind::EndPortalFrame);
    }
    s.set(at(5, 3, 6), BlockKind::Spawner);
}

/// Ground of netherrack to y 63, open air over it, the room's origin at (0, 20, 0).
fn room_world(x: (i32, i32), z: (i32, i32)) -> Stage {
    let mut s = Stage::over(x, z, 63, 127);
    portal_room(&mut s, (0, 20, 0));
    s
}

fn frames_of(w: &World, lo: Cell, hi: Cell) -> Vec<[i32; 3]> {
    let mut v = Vec::new();
    for x in lo.0..=hi.0 {
        for y in lo.1..=hi.1 {
            for z in lo.2..=hi.2 {
                if w.block_at(BlockPos::new(x, y, z)) == Some(BlockKind::EndPortalFrame) {
                    v.push([x, y, z]);
                }
            }
        }
    }
    v
}

fn the_room(w: &World) -> Room {
    let frames = frames_of(w, (0, 20, 0), (10, 27, 15));
    assert_eq!(frames.len(), 12);
    stronghold::room_of(&frames, 2).unwrap()
}

/// The ring's centre and the floor from the frames: centre (5, 23, 10) at the frames' height, the
/// feet on the floor at y 21 (frame y - 2).
#[test]
fn the_room_is_found_from_its_frames() {
    let w = load(&room_world((-40, 60), (-30, 40)), [40.5, 64.0, 10.5]);
    let r = the_room(&w);
    assert_eq!(r.centre, [5, 23, 10]);
    assert_eq!(r.floor_y, 21);
}

/// The standing cell: on the floor, inside the room, out of the ring, two or more from any lava
/// (the pool under the ring and the side pools near the door).
#[test]
fn the_standing_cell_is_clear_of_the_lava() {
    let w = load(&room_world((-40, 60), (-30, 40)), [40.5, 64.0, 10.5]);
    let r = the_room(&w);
    let t = stronghold::stand_target(&w, &r, (40, 64, 10), 7).expect("a standing cell");
    assert_eq!(t.1, 21, "{t:?}");
    for dx in -2..=2 {
        for dz in -2..=2 {
            for dy in -1..=0 {
                let k = w.block_at(BlockPos::new(t.0 + dx, t.1 + dy, t.2 + dz));
                assert_ne!(k, Some(BlockKind::Lava), "{t:?} near lava");
            }
        }
    }
    let off = f64::from(t.0 - 5).hypot(f64::from(t.2 - 10));
    assert!(off <= 7.0 && off > 3.0, "{t:?} {off}");
    assert!(stronghold::inside(&r, t, 7));
}

/// The descent: dry stairs, toward the room from far out, away from its centre when over it.
#[test]
fn the_descent_is_dry_and_keeps_out_of_the_footprint() {
    for (x, want) in [(60, (-1, 0)), (12, (1, 0))] {
        let w = load(
            &room_world((-40, 100), (-30, 40)),
            [f64::from(x) + 0.5, 64.0, 10.5],
        );
        let mut p = Plan::with_table(Aim::Race, table());
        p.stronghold.room = Some(the_room(&w));
        p.stronghold.looked = Some(0);
        let t = p.stronghold_task(&w, 1);
        let Some(Task::Stairs(st)) = t else {
            panic!("at x {x}: {t:?} {:#?}", notes(&p));
        };
        assert!(st.dry, "a dry staircase");
        assert_eq!(st.dir, want, "at x {x}: {:#?}", notes(&p));
        assert!(
            notes(&p)
                .iter()
                .any(|n| n.contains("stronghold stairs down")),
            "{:#?}",
            notes(&p)
        );
    }
}

/// An infested block is refused by the staircase and is a wall to the tunnel's route: the room's
/// east wall all infested, the route from the east at the floor's height goes round it.
#[test]
fn infested_blocks_are_never_opened() {
    let mut s = room_world((-40, 60), (-30, 40));
    s.fill((10, 10), (20, 27), (0, 15), BlockKind::InfestedStoneBricks);
    let w = load(&s, [30.5, 64.0, 10.5]);
    let cell = (10, 21, 10);
    let why = plan::stairs::refusal(&w, &[cell], (11, 20, 10));
    assert!(
        why.as_deref().is_some_and(|y| y.starts_with("infested at")),
        "{why:?}"
    );
    let r = plan::tunnel::route(
        &w,
        (25, 21, 10),
        &[(7, 21, 3)],
        &std::collections::BTreeSet::new(),
        false,
        plan::tunnel::MAX_NODES,
    );
    if let Some(r) = r {
        for p in r.cells.windows(2) {
            let (cells, _) = plan::tunnel::opened(p[0], p[1]).unwrap();
            for c in cells {
                assert!(!stronghold::never_dug(&w, c), "the route opens {c:?}");
            }
        }
    }
}

/// Flat ground at y 63 over x -16 to 200, z -32 to 32, the bot at (0, 64, 0); `water` laid at
/// y 61 to 63 over the x span given.
fn flat(water: Option<(i32, i32)>) -> (World, Plan) {
    let mut s = Stage::over((-16, 200), (-32, 32), 63, 127);
    if let Some(xs) = water {
        s.fill(xs, (61, 63), (-32, 32), BlockKind::Water);
    }
    let mut w = load(&s, [0.5, 64.0, 0.5]);
    give(&mut w, 0, ItemKind::Cobblestone, 64);
    give(&mut w, 1, ItemKind::Cobblestone, 64);
    noon(&mut w, 6000);
    let mut p = Plan::with_table(Aim::Race, table());
    p.overland.fix = Some(([600.5, 0.5], 20.0));
    (w, p)
}

/// A river ten wide across the way: the leg's far walk bridges it with held blocks.
#[test]
fn a_river_is_crossed_on_a_bridge() {
    let (w, mut p) = flat(Some((20, 29)));
    let t = p.overland_task(&w, 0);
    let Some(Task::Explore(walk)) = t else {
        panic!("{t:?} {:#?}", notes(&p));
    };
    let b = walk.bridges_ahead();
    assert!((1..=40).contains(&b), "{b} bridges: {:#?}", notes(&p));
    assert!(walk.more.water);
    let n = notes(&p);
    assert!(
        n.iter().any(|n| n.starts_with("overland leg 1: ")),
        "{n:#?}"
    );
    assert!(n.iter().any(|n| n.contains("eyes 0")), "{n:#?}");
}

/// Water from x 20 to the world's end: an ocean, noted and gone round.
#[test]
fn an_ocean_is_gone_round() {
    let (w, mut p) = flat(Some((20, 200)));
    let _ = p.overland_task(&w, 0);
    let n = notes(&p);
    assert!(
        n.iter().any(|n| n.starts_with("overland ocean ahead")),
        "{n:#?}"
    );
}

/// Under `DUSK_MARGIN` to dusk the overland builds a hut and walks no leg; by day it walks.
#[test]
fn a_hut_at_dusk() {
    let (mut w, mut p) = flat(None);
    noon(&mut w, 11_800);
    let t = p.overland_task(&w, 0);
    assert!(matches!(t, Some(Task::Idle)), "{t:?}");
    let n = notes(&p);
    assert!(
        n.iter().any(|n| n.starts_with("overland hut 1 (night)")),
        "{n:#?}"
    );
    assert!(!n.iter().any(|n| n.starts_with("overland leg")), "{n:#?}");
    // A few looks: blocks go up (or the hand takes the block first).
    for tick in 1..40 {
        let _ = p.overland_task(&w, tick * 10);
        let _ = p.take_acts();
    }
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("overland hut block 1")),
        "{:#?}",
        notes(&p)
    );
}

/// The fix not 32 nearer in 1,200 walking ticks: a note and a turn of 30 degrees.
#[test]
fn no_progress_turns_the_bearing() {
    let (w, mut p) = flat(None);
    p.overland.mark = Some((610.0, 0));
    let _ = p.overland_task(&w, 1300);
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|n| n.starts_with("overland turned 30") || n.starts_with("overland turned -30")),
        "{n:#?}"
    );
}

/// Arrival with no frame seen: the search's centre is set and the overland is over.
#[test]
fn arrival_hands_over_to_the_search() {
    let (w, mut p) = flat(None);
    p.overland.fix = Some(([10.5, 0.5], 5.0));
    assert!(p.overland_task(&w, 0).is_none());
    assert_eq!(p.stronghold.at_fix, Some([10, 0]));
    let t = p.stronghold_task(&w, 1);
    assert!(matches!(t, Some(Task::Stairs(_))), "{t:?}");
}

/// Ground to y 63 with a pocket of air for the feet at `feet` (two cells tall), the search's
/// centre at (0, 0).
fn deep(feet: Cell, extra: &[(Cell, BlockKind)]) -> (World, Plan) {
    let mut s = Stage::over((-48, 48), (-48, 48), 63, 127);
    s.fill(
        (feet.0, feet.0),
        (feet.1, feet.1 + 1),
        (feet.2, feet.2),
        BlockKind::Air,
    );
    for (c, k) in extra {
        s.set(*c, *k);
    }
    let w = load(
        &s,
        [
            f64::from(feet.0) + 0.5,
            f64::from(feet.1),
            f64::from(feet.2) + 0.5,
        ],
    );
    let mut p = Plan::with_table(Aim::Race, table());
    p.stronghold.at_fix = Some([0, 0]);
    (w, p)
}

/// At the search's depth once, never stairs down again: race 45689919380 lane 0 went down at
/// leg 7 from y 35 after a spiral leg's tunnel climbed.
#[test]
fn the_search_descends_once() {
    let (w, mut p) = deep((0, 30, 0), &[]);
    let _ = p.stronghold_task(&w, 1);
    assert!(p.stronghold.descended, "{:#?}", notes(&p));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("stronghold search at depth")),
        "{:#?}",
        notes(&p)
    );
    let (w, mut p) = deep((0, 36, 0), &[]);
    p.stronghold.descended = true;
    let t = p.stronghold_task(&w, 1);
    assert!(
        !notes(&p)
            .iter()
            .any(|n| n.starts_with("stronghold search stairs down")),
        "{t:?} {:#?}",
        notes(&p)
    );
}

/// Stone bricks within the tunnel's reach but out of a dig's: a step dug toward them, never the
/// spiral's turn away (race 45689919380 lane 0 at (1646, 29, -8)).
#[test]
fn a_brick_short_of_reach_is_stepped_to() {
    let (w, mut p) = deep(
        (0, 30, 0),
        &[
            ((4, 29, 1), BlockKind::StoneBricks),
            ((5, 29, 1), BlockKind::Air),
        ],
    );
    p.stronghold.descended = true;
    let t = p.stronghold_task(&w, 1);
    let n = notes(&p);
    assert!(
        matches!(
            t,
            Some(Task::Dig(_)) | Some(Task::Stairs(_)) | Some(Task::Explore(_))
        ),
        "{t:?} {n:#?}"
    );
    assert!(
        !n.iter().any(|n| n.starts_with("stronghold search leg")),
        "{n:#?}"
    );
}

/// No fresh stone pickaxe, cobblestone, sticks and a table in the pack: the table goes down
/// beside the feet for the crafts.
#[test]
fn pickaxes_are_made_from_the_pack() {
    let (mut w, mut p) = flat(None);
    give(&mut w, 2, ItemKind::Stick, 8);
    give(&mut w, 3, ItemKind::CraftingTable, 1);
    let t = p.stronghold_pickaxes(&w, 0);
    let n = notes(&p);
    assert!(matches!(t, Some(Task::Place(_))), "{t:?} {n:#?}");
    assert!(
        n.iter().any(|n| n.starts_with("stronghold pickaxes (")),
        "{n:#?}"
    );
}

/// Stone bricks five under the feet (race 45689919380 lane 1 at (1660, 27, -18) over bricks at
/// y 22): a way down toward them, never the spiral's turn away.
#[test]
fn a_brick_under_the_feet_is_dug_down_to() {
    let (w, mut p) = deep(
        (0, 30, 0),
        &[
            ((0, 25, 0), BlockKind::StoneBricks),
            ((1, 25, 0), BlockKind::Air),
        ],
    );
    p.stronghold.descended = true;
    let t = p.stronghold_task(&w, 1);
    let n = notes(&p);
    assert!(
        matches!(
            t,
            Some(Task::Dig(_)) | Some(Task::Stairs(_)) | Some(Task::Explore(_))
        ),
        "{t:?} {n:#?}"
    );
    assert!(
        !n.iter().any(|n| n.starts_with("stronghold search leg")),
        "{n:#?}"
    );
}

/// Every `[stronghold] junk` name is an item the game knows.
#[test]
fn the_stronghold_junk_names_are_items() {
    let t = table();
    assert!(!t.stronghold.junk.is_empty());
    assert_eq!(
        t.stronghold.junk_kinds().len(),
        t.stronghold.junk.len(),
        "{:?}",
        t.stronghold.junk
    );
}

/// The standing cell passes the `portal_room` stage's own test with the feet on it, from either
/// side of the room.
#[test]
fn the_standing_cell_is_in_the_stage_box() {
    let s = room_world((-40, 60), (-30, 40));
    for from in [(40, 64, 10), (-30, 64, 10), (5, 64, 35)] {
        let w = load(&s, [40.5, 64.0, 10.5]);
        let r = the_room(&w);
        let t = stronghold::stand_target(&w, &r, from, 7).expect("a standing cell");
        let at = load(
            &s,
            [f64::from(t.0) + 0.5, f64::from(t.1), f64::from(t.2) + 0.5],
        );
        assert!(
            world::stage::in_portal_room(&at),
            "from {from:?}: {t:?} is outside the stage's box"
        );
    }
}

/// The stronghold's junk never names what the way on needs: eyes, pearls, rods, powder, food,
/// tools, armour, a table, a furnace, sticks or planks.
#[test]
fn the_stronghold_junk_keeps_what_the_way_needs() {
    let t = table();
    let names: Vec<&str> = t.stronghold.junk.iter().map(|(n, _)| n.as_str()).collect();
    for kept in [
        "ender_eye",
        "ender_pearl",
        "blaze_rod",
        "blaze_powder",
        "cooked_beef",
        "bread",
        "crafting_table",
        "furnace",
        "stick",
    ] {
        assert!(!names.contains(&kept), "{kept} is in the junk");
    }
    for n in &names {
        assert!(
            !n.ends_with("_pickaxe")
                && !n.ends_with("_sword")
                && !n.ends_with("_axe")
                && !n.ends_with("_shovel")
                && !n.ends_with("_helmet")
                && !n.ends_with("_chestplate")
                && !n.ends_with("_leggings")
                && !n.ends_with("_boots")
                && !n.ends_with("_planks")
                && !n.ends_with("shield"),
            "{n} is in the junk"
        );
    }
    let food = ItemKind::CookedBeef;
    assert!(
        !t.stronghold.junk_kinds().iter().any(|(k, _)| *k == food),
        "food in the junk"
    );
}

/// The room far out on the surface: a far walk toward it before any stairs (race 45698008920
/// began its stairs 112 to 118 off and reached the floor's height 78 to 92 off).
#[test]
fn a_far_room_is_walked_to_on_the_surface() {
    let w = load(&room_world((-40, 140), (-30, 40)), [100.5, 64.0, 10.5]);
    let mut p = Plan::with_table(Aim::Race, table());
    p.stronghold.room = Some(the_room(&w));
    p.stronghold.looked = Some(0);
    let t = p.stronghold_task(&w, 1);
    let n = notes(&p);
    assert!(matches!(t, Some(Task::Explore(_))), "{t:?} {n:#?}");
    assert!(
        n.iter().any(|n| n.starts_with("stronghold surface walk 1")),
        "{n:#?}"
    );
}

/// Six over the floor, a stairs leg's cut: the floor's height is latched and no stairs begin
/// (race 45698008920 lane 1 went down to 13 and up to 19 round a floor at 17 for 3,000 ticks).
#[test]
fn within_a_leg_of_the_floor_the_tunnel_takes_over() {
    let mut s = room_world((-40, 60), (-30, 40));
    s.fill((22, 22), (27, 28), (10, 10), BlockKind::Air);
    let w = load(&s, [22.5, 27.0, 10.5]);
    let mut p = Plan::with_table(Aim::Race, table());
    p.stronghold.room = Some(the_room(&w));
    p.stronghold.looked = Some(0);
    let t = p.stronghold_task(&w, 1);
    let n = notes(&p);
    assert!(p.stronghold.at_floor, "{t:?} {n:#?}");
    assert!(
        !n.iter().any(|n| n.starts_with("stronghold stairs")),
        "{t:?} {n:#?}"
    );
}

/// The tunnel's end 4 across and 3 over the standing cell, inside its reach: the last blocks are
/// dug or walked, never "no way" (race 45703184922 lane 0 waited 6,500 ticks at (1620, 20, -30)).
#[test]
fn the_last_steps_from_the_tunnel_end() {
    let s0 = room_world((-40, 60), (-30, 40));
    let w0 = load(&s0, [-20.5, 64.0, 10.5]);
    let r = the_room(&w0);
    let t = stronghold::stand_target(&w0, &r, (-20, 24, 10), 7).expect("a standing cell");
    let feet = (t.0 - 4, t.1 + 3, t.2);
    let mut s = room_world((-40, 60), (-30, 40));
    s.fill(
        (feet.0, feet.0),
        (feet.1, feet.1 + 1),
        (feet.2, feet.2),
        BlockKind::Air,
    );
    let w = load(
        &s,
        [
            f64::from(feet.0) + 0.5,
            f64::from(feet.1),
            f64::from(feet.2) + 0.5,
        ],
    );
    let mut p = Plan::with_table(Aim::Race, table());
    p.stronghold.room = Some(r);
    p.stronghold.looked = Some(0);
    let task = p.stronghold_task(&w, 1);
    let n = notes(&p);
    assert!(
        matches!(
            task,
            Some(Task::Dig(_)) | Some(Task::Explore(_)) | Some(Task::Stairs(_))
        ),
        "{t:?} from {feet:?}: {task:?} {n:#?}"
    );
    assert!(
        !n.iter()
            .any(|n| n.starts_with("stronghold tunnel has no way")),
        "{n:#?}"
    );
}

/// A junk kind that came back is not dropped again while a main slot is free: leather only while
/// a walk is the task (the bot walks off before the pick-up), cobblestone not at all (race
/// 45703184922 lane 0: 167 leather drops to a bot standing still).
#[test]
fn junk_that_came_back_is_held_back() {
    let mut slots = std::collections::BTreeMap::new();
    for s in 9i16..=40 {
        slots.insert(s, (ItemKind::Stick, 1));
    }
    slots.insert(41, (ItemKind::Leather, 4));
    slots.insert(42, (ItemKind::Cobblestone, 64));
    slots.insert(43, (ItemKind::Cobblestone, 64));
    let junk = [(ItemKind::Leather, 0), (ItemKind::Cobblestone, 64)];
    assert!(
        plan::junk_slot(&slots, &junk).is_some(),
        "one slot free: a drop is due"
    );
    let held = [(ItemKind::Leather, 3000), (ItemKind::Cobblestone, 3000)];
    let still = stronghold::junk_now(&junk, &held, 100, false);
    assert_eq!(plan::junk_slot(&slots, &still), None, "{still:?}");
    let walking = stronghold::junk_now(&junk, &held, 100, true);
    assert_eq!(
        plan::junk_slot(&slots, &walking),
        Some((41, ItemKind::Leather)),
        "{walking:?}"
    );
    let later = stronghold::junk_now(&junk, &held, 3000, false);
    assert!(plan::junk_slot(&slots, &later).is_some(), "{later:?}");
    assert!(stronghold::block_of(ItemKind::Cobblestone).is_some());
    assert!(stronghold::block_of(ItemKind::Leather).is_none());
}
