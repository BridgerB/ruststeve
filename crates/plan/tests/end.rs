//! Issue 20e (agent E3, docs/problems/end-island.md), on hand-built End worlds: the void as an
//! edge, the look kept down, the pillar and the bridge from the platform with guards on both
//! sides, the wait for a flying dragon, the island reached, the bunker dug, the stock's spiral.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::end::{self, BRIDGE_FLOOR, end_look, stock_dir, void_column, void_edge};
use plan::look::ENDERMAN_EYE;
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use protocol::metadata::MetaValue;
use world::{Act, Event, World};

fn state(k: BlockKind) -> u32 {
    k.info().map(|i| i.default_state).unwrap_or(0)
}

fn section(k: BlockKind) -> Section {
    Section {
        block_count: if k == BlockKind::Air { 0 } else { 4096 },
        fluid_count: 0,
        blocks: Paletted::Single(state(k)),
        biomes: Paletted::Single(0),
    }
}

/// An End world, 16 sections a column (y 0 to 255): end stone in the lowest `solid` sections
/// (its top at y 16 x solid - 1), air over them, chunk columns x `cx` and z -2 to 2; the feet at
/// `pos`.
fn world(solid: usize, cx: std::ops::RangeInclusive<i32>, pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::EndStone); solid];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 16 - solid));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 2,
        dimension: end::END.to_owned(),
        game_type: 0,
    }));
    for x in cx {
        for z in -2..=2 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w.inventory.slots.insert(36, (ItemKind::Cobblestone, 64));
    w.inventory.slots.insert(37, (ItemKind::Cobblestone, 64));
    w.inventory.slots.insert(38, (ItemKind::StonePickaxe, 1));
    w.inventory.slots.insert(39, (ItemKind::CookedBeef, 32));
    w.held = 0;
    w
}

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

/// The arrival platform over the void: 5x5 obsidian at y 48 (E1's measure of this seed).
fn platform(pos: [f64; 3]) -> World {
    let mut w = world(0, 4..=7, pos);
    for x in 98..=102 {
        for z in -2..=2 {
            set(&mut w, (x, 48, z), BlockKind::Obsidian);
        }
    }
    w
}

fn with_entity(w: &mut World, id: i32, kind: EntityKind, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
        velocity: [0.0; 3],
    }));
}

fn table() -> Arc<make::Table> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/goals.toml"
    ))
    .unwrap_or_default();
    let goals: make::Goals = toml::from_str(&text).unwrap_or_else(|e| panic!("goals.toml: {e}"));
    Arc::new(make::Table::new(&goals).unwrap_or_else(|e| panic!("table: {e}")))
}

fn racer() -> Plan {
    Plan::with_table(Aim::Race, table())
}

fn noted(p: &Plan, what: &str) -> bool {
    p.events.iter().any(|e| e.what.starts_with(what))
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

#[test]
fn the_void_beside_the_platform_is_an_edge_and_ground_is_not() {
    let w = platform([100.5, 49.0, 0.5]);
    // The platform's west edge: (97, 48, 0) and everything under it is air to y 0.
    assert!(void_column(&w, (97, 48, 0)));
    assert!(void_edge(&w, (98, 49, 0)));
    // Its middle has obsidian on every side.
    assert!(!void_edge(&w, (100, 49, 0)));
    // Ground anywhere under the cell beside: no void.
    let mut w = platform([100.5, 49.0, 0.5]);
    set(&mut w, (97, 3, 0), BlockKind::EndStone);
    assert!(!void_column(&w, (97, 48, 0)));
    // Not loaded under it: not a known void.
    let w = world(0, 6..=6, [100.5, 49.0, 0.5]);
    assert!(!void_column(&w, (200, 48, 0)));
}

#[test]
fn a_look_at_an_endermans_eyes_is_turned_down() {
    let mut w = world(4, 0..=1, [8.5, 64.0, 8.5]);
    let at = [18.5, 64.0, 8.5];
    with_entity(&mut w, 41, EntityKind::Enderman, at);
    let eye = [8.5, 64.0 + physics::EYE_HEIGHT, 8.5];
    let (yaw, pitch) = plan::walk::look_at(eye, [at[0], at[1] + ENDERMAN_EYE, at[2]]);
    #[allow(clippy::cast_possible_truncation)]
    let want = (yaw as f32, pitch);
    assert_eq!(end_look(&w, Some(want)), (want.0, 90.0));
    // The other way it is sent as asked.
    let away = (want.0 + 180.0, 0.0);
    assert_eq!(end_look(&w, Some(away)), away);
    // No look asked for: straight down.
    assert_eq!(end_look(&w, None).1, 90.0);
}

#[test]
fn in_the_end_a_walk_looks_down() {
    let w = platform([100.5, 49.0, 0.5]);
    let mut p = racer();
    let c = p.step(&w);
    assert_eq!(c.pitch, Some(90.0), "{:?}", notes(&p));
}

#[test]
fn on_the_platform_the_bot_pillars_up_to_the_bridges_height() {
    let w = platform([100.5, 49.0, 0.5]);
    let mut p = racer();
    p.step(&w);
    assert!(noted(&p, "end: arrived at (100, 49, 0)"), "{:?}", notes(&p));
    assert!(
        noted(&p, &format!("end: pillar up {} at", BRIDGE_FLOOR + 1 - 49)),
        "{:?}",
        notes(&p)
    );
    // Centred and holding a block, it jumps.
    let jumped = (0..4).any(|_| p.step(&w).jump);
    assert!(jumped, "{:?}", notes(&p));
}

/// The bot on its pillar at the bridge's height, the column under it of cobblestone.
fn on_the_pillar() -> World {
    let mut w = platform([100.5, f64::from(BRIDGE_FLOOR + 1), 0.5]);
    for y in 49..=BRIDGE_FLOOR {
        set(&mut w, (100, y, 0), BlockKind::Cobblestone);
    }
    w
}

#[test]
fn the_bridge_lays_a_guard_first_then_the_floor_ahead() {
    let w = on_the_pillar();
    let mut p = racer();
    p.step(&w);
    assert!(noted(&p, "end: bridge west from"), "{:?}", notes(&p));
    let mut acts = Vec::new();
    for _ in 0..10 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(noted(&p, "end: guard into"), "{:?}", notes(&p));
    assert!(
        acts.iter().any(|a| matches!(a, Act::UseOn { .. })),
        "{acts:?}"
    );
    // With both sides guarded, the floor ahead (99, 52, 0) is laid against the block stood on.
    let mut w = on_the_pillar();
    for z in [-1, 1] {
        set(&mut w, (100, BRIDGE_FLOOR, z), BlockKind::Cobblestone);
        set(&mut w, (100, BRIDGE_FLOOR + 1, z), BlockKind::Cobblestone);
    }
    let mut p = racer();
    let mut acts = Vec::new();
    for _ in 0..10 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        noted(
            &p,
            &format!("end: bridge block into (99, {BRIDGE_FLOOR}, 0)")
        ),
        "{:?}",
        notes(&p)
    );
    assert!(
        acts.contains(&Act::UseOn {
            pos: world::BlockPos::new(100, BRIDGE_FLOOR, 0),
            face: 4
        }),
        "{acts:?}"
    );
}

#[test]
fn a_flying_dragon_near_holds_the_bridge_crouched() {
    let mut w = on_the_pillar();
    with_entity(&mut w, 7, EntityKind::EnderDragon, [80.0, 70.0, 0.0]);
    if let Some(e) = w.entities.get_mut(&7) {
        // Phase 0, the holding pattern: flying.
        e.metadata.insert(16, MetaValue::Int(0));
    }
    let mut p = racer();
    p.step(&w);
    let c = p.step(&w);
    assert!(
        noted(&p, "end: bridge waits for the dragon"),
        "{:?}",
        notes(&p)
    );
    assert!(c.sneak && !c.forward, "{c:?}");
    // Perched (phase 6), the bridge goes on.
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(6));
    }
    p.step(&w);
    assert!(noted(&p, "end: bridge goes on"), "{:?}", notes(&p));
}

#[test]
fn end_stone_under_the_sky_is_the_island_and_the_bot_walks_to_the_bunker_site() {
    // Flat end stone to y 63 from x 0 to 127; the feet at x 60.
    let w = world(4, 0..=7, [60.5, 64.0, 0.5]);
    let mut p = racer();
    p.step(&w);
    assert!(
        noted(&p, "end: on the island at (60, 64, 0)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        noted(&p, "end: walk to the bunker site (12, 64, 0)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn at_the_site_the_bunker_begins_with_its_post() {
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    // The block in hand (a hand-built world does not answer the hold's swap): the post first.
    w.held = 0;
    let mut p = racer();
    p.end.bridged = true;
    let mut acts = Vec::new();
    for _ in 0..5 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        noted(&p, "end: dig the bunker at (12, 64, 0)"),
        "{:?}",
        notes(&p)
    );
    // The post first: a block on the ground beside the shaft's top cell.
    assert!(
        acts.contains(&Act::UseOn {
            pos: world::BlockPos::new(13, 63, 0),
            face: 1
        }),
        "{acts:?}"
    );
}

#[test]
fn the_bunker_shaft_is_roofed_after_every_dig() {
    use plan::end::{Op, shaft_ops};
    let ops = shaft_ops((12, 64, 0), (13, 63, 0));
    // The post of two on the ground block beside the top cell.
    assert!(ops.contains(&Op::Place {
        cell: (13, 64, 0),
        against: (13, 63, 0),
        face: 1
    }));
    assert!(ops.contains(&Op::Place {
        cell: (13, 65, 0),
        against: (13, 64, 0),
        face: 1
    }));
    // Every dig under the feet is followed by the fall and the cell over the new head.
    let digs: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| matches!(o, Op::Dig(_)))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(digs.len(), 4);
    for i in digs {
        let Some(Op::Dig(c)) = ops.get(i) else {
            panic!("dig");
        };
        assert_eq!(ops.get(i + 1), Some(&Op::Settle));
        // The feet fall into c: the head is c + 1, the cell over it c + 2.
        assert_eq!(ops.get(i + 2), Some(&Op::Roof((c.0, c.1 + 2, c.2))));
    }
    // The bunker's feet four down at (12, 60, 0), the roof's top cells laid.
    assert!(ops.contains(&Op::Dig((12, 60, 0))));
    assert!(ops.contains(&Op::Roof((12, 62, 0))));
}

#[test]
fn a_flying_dragon_near_holds_the_bunker_shaft() {
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    with_entity(&mut w, 7, EntityKind::EnderDragon, [30.0, 80.0, 0.0]);
    if let Some(e) = w.entities.get_mut(&7) {
        e.metadata.insert(16, MetaValue::Int(1));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.step(&w);
    assert!(
        noted(&p, "end: the bunker waits for the dragon"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: dig the bunker"), "{:?}", notes(&p));
}

#[test]
fn every_stock_run_begins_and_ends_at_the_bunker() {
    // The bunker at (12, 61, 0), sealed; the stock begins with the stairs down.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: stock run 1 toward"), "{:?}", notes(&p));
    assert_eq!(p.end.stock_out, Some(plan::end::StockRun::Down));
    // The run over at its foot (12, 60, 1) (the stairs run along z, off the door), the way back
    // open: walked home.
    p.task = plan::Task::Idle;
    for c in [(12, 60, 1), (12, 61, 1), (12, 62, 1)] {
        set(&mut w, c, BlockKind::Air);
    }
    w.body = Some(physics::Body {
        pos: [12.5, 60.0, 1.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    assert!(
        noted(&p, "end: stock run 1 over at (12, 60, 1)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        noted(&p, "end: stock back to the bunker"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.end.stock_foot, Some((12, 60, 1)));
    // Home again: counted, and the next run is walked out to from the bunker.
    p.task = plan::Task::Idle;
    w.body = Some(physics::Body {
        pos: [12.5, 61.0, 0.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    assert!(
        noted(&p, "end: back at the bunker after stock run 1"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.end.stock_back, 1);
    assert!(
        noted(&p, "end: stock walk to (12, 60, 1)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn in_the_sealed_bunker_short_of_stone_a_stock_tunnel_begins() {
    // The bunker at (12, 61, 0): its cells open, the roof at y 63 in place.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    set(&mut w, (12, 61, 0), BlockKind::Air);
    set(&mut w, (12, 62, 0), BlockKind::Air);
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: stock run 1 toward"), "{:?}", notes(&p));
    assert_eq!(p.task.name(), "stairs");
}

#[test]
fn the_stock_spirals_out_from_the_door() {
    let door = (1, 0);
    let dirs: Vec<(i32, i32)> = (0..6).map(|n| stock_dir(door, n)).collect();
    // Legs of 1, 1, 2, 2 runs, a quarter turn each, never first toward the door.
    assert_eq!(
        dirs,
        vec![(0, 1), (-1, 0), (0, -1), (0, -1), (1, 0), (1, 0)]
    );
}

#[test]
fn in_a_roofed_tunnel_an_angry_enderman_seals_no_pocket() {
    // The bunker at (12, 61, 0) and a tunnel cell beside it, under the end stone's top at 63.
    let mut w = world(4, 0..=7, [12.5, 61.0, 0.5]);
    for c in [(12, 61, 0), (12, 62, 0), (12, 61, 1), (12, 62, 1)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    assert!(end::covered(&w));
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(
        noted(&p, "end: covered from an angry enderman"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "provoked enderman"), "{:?}", notes(&p));
    // On the open surface it is the issue 19 pocket as before.
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    assert!(!end::covered(&w));
}

#[test]
fn up_a_pillar_with_no_crystal_left_the_crystals_task_brings_the_bot_down() {
    // E6's closed loop: the last crystal gone, the bot stayed 18 up on its pillar.
    let mut w = world(4, 0..=7, [20.5, 74.0, 0.5]);
    for y in 64..=73 {
        set(&mut w, (20, y, 0), BlockKind::EndStone);
    }
    for c in [(12, 61, 0), (12, 62, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.spikes.pillars.push(plan::spikes::PillarRec {
        x: 20,
        z: 0,
        base: 63,
    });
    for _ in 0..3 {
        p.step(&w);
    }
    assert!(
        noted(&p, "end: up a pillar at (20, 74, 0)"),
        "{:?}",
        notes(&p)
    );
    assert!(!noted(&p, "end: no walk to the bunker"), "{:?}", notes(&p));
    assert!(!noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
}

#[test]
fn hurt_the_walk_to_the_bunker_waits_while_a_cloud_lies_on_the_way() {
    // The bunker at (12, 61, 0), its shaft open to the surface; the bot 18 east at 10 health.
    let mut w = world(4, 0..=7, [30.5, 64.0, 0.5]);
    for y in 61..=63 {
        set(&mut w, (12, y, 0), BlockKind::Air);
    }
    w.player.health = Some(10.0);
    with_entity(&mut w, 11, EntityKind::AreaEffectCloud, [21.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&11) {
        e.metadata.insert(8, MetaValue::Float(3.0));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 61, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(noted(&p, "end: retreat to the bunker"), "{:?}", notes(&p));
    assert!(
        noted(&p, "end: the way to the bunker waits for a cloud"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.task.name(), "idle");
    // The cloud gone, the walk begins.
    w.entities.remove(&11);
    p.retry_at = 0;
    p.step(&w);
    assert!(noted(&p, "end: back to the bunker"), "{:?}", notes(&p));
}

#[test]
fn deep_in_the_tunnels_with_no_way_back_the_bot_cuts_stairs_up_to_hand_over() {
    // The stock over at y 52, the bunker at (12, 59, 0) out of reach (its cells shut).
    let mut w = world(4, 0..=7, [30.5, 52.0, 0.5]);
    for c in [(30, 52, 0), (30, 53, 0), (29, 52, 0), (29, 53, 0)] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 59, 0]);
    p.end.door = Some((1, 0));
    p.step(&w);
    assert!(
        noted(&p, "end: stairs up out of the tunnels from (30, 52, 0)"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.task.name(), "stairs", "{:?}", notes(&p));
}

#[test]
fn on_a_laid_block_within_reach_of_the_fountain_the_bunker_is_dug_there() {
    // Race 45888713362 lane 0: 25 from the fountain on a cobblestone block, ten site walks done.
    let mut w = world(4, 0..=7, [23.5, 64.0, -7.5]);
    set(&mut w, (23, 63, -8), BlockKind::Cobblestone);
    let mut p = racer();
    p.end.bridged = true;
    p.end.site_walks = plan::end::SITE_WALKS;
    p.step(&w);
    assert!(
        noted(&p, "end: dig the bunker at (23, 64, -8)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_bunker_opened_the_bot_walks_up_its_stair_onto_the_ground() {
    // Race 45890445738: both bots stood half on the stair's first step for 14,000 ticks.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
        (10, 62, 0),
        (10, 63, 0),
        (9, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.exit_walk = true;
    p.step(&w);
    assert!(
        noted(
            &p,
            "end: up the bunker's stair by [(11, 61, 0), (10, 62, 0), (9, 63, 0)]"
        ),
        "{:?}",
        notes(&p)
    );
    // Steered at the first step with a jump.
    let c = p.step(&w);
    assert!(c.forward && c.jump, "{c:?}");
    // On it (race 45898928805 lane 0 stood here, half over the bunker, with no route found): on
    // to the next step.
    w.body = Some(physics::Body {
        pos: [11.5, 61.0, 0.5],
        on_ground: true,
        ..physics::Body::default()
    });
    p.step(&w);
    let c = p.step(&w);
    assert!(c.forward && c.jump, "{c:?}");
}

#[test]
fn from_the_first_end_tick_every_look_is_straight_down() {
    // E5's race 45886819862 lane 1: the arrival's pitch 0 stared at an enderman at tick 60.
    let mut w = platform([100.5, 49.0, 0.5]);
    w.player.pitch = 0.0;
    with_entity(&mut w, 41, EntityKind::Enderman, [90.5, 49.0, 0.5]);
    let mut p = racer();
    // No body yet (the arrival's teleport not answered): down.
    let body = w.body.take();
    assert_eq!(p.step(&w).pitch, Some(90.0));
    w.body = body;
    assert_eq!(p.step(&w).pitch, Some(90.0));
}

#[test]
fn a_pocket_is_built_from_end_stone_when_that_is_all_that_is_held() {
    // E5's race 45886819862 lane 1: "no block held for a pocket" with 192 end stone.
    let mut w = world(4, 0..=7, [12.5, 64.0, 0.5]);
    w.inventory.slots.clear();
    w.inventory.slots.insert(36, (ItemKind::EndStone, 64));
    w.held = 0;
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    let mut p = racer();
    p.end.bridged = true;
    for _ in 0..3 {
        p.step(&w);
    }
    assert!(noted(&p, "pocket block 1"), "{:?}", notes(&p));
    assert!(!noted(&p, "no block held"), "{:?}", notes(&p));
}

#[test]
fn on_the_bunkers_stair_an_angry_enderman_seals_no_pocket() {
    // Race 45898928805: a pocket laid in the stair shut the bunker's own door.
    let mut w = world(4, 0..=7, [11.5, 61.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 9, EntityKind::Enderman, [16.5, 64.0, 0.5]);
    if let Some(e) = w.entities.get_mut(&9) {
        e.metadata.insert(17, MetaValue::Bool(true));
    }
    let mut p = racer();
    p.end.bridged = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    assert!(p.at_bunker(&w));
    p.step(&w);
    assert!(!noted(&p, "provoked enderman"), "{:?}", notes(&p));
    assert!(
        noted(&p, "end: covered from an angry enderman"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_shut_step_of_the_bunkers_stair_is_dug_out_on_the_way_up() {
    // The bunker opened, its stair's second step (10, 62, 0) shut by a block laid there.
    let mut w = world(4, 0..=7, [12.5, 60.0, 0.5]);
    for c in [
        (12, 60, 0),
        (12, 61, 0),
        (12, 62, 0),
        (11, 61, 0),
        (11, 62, 0),
        (11, 63, 0),
        (10, 63, 0),
        (9, 63, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    with_entity(&mut w, 12, EntityKind::EndCrystal, [42.5, 83.0, 0.5]);
    w.held = 2;
    let mut p = racer();
    p.end.bridged = true;
    p.end.stock_over = true;
    p.bunker = Some([12, 60, 0]);
    p.end.door = Some((-1, 0));
    p.end.exit_walk = true;
    p.step(&w);
    // On the first step: the next step's cell is dug before it is stepped onto.
    w.body = Some(physics::Body {
        pos: [11.5, 61.0, 0.5],
        on_ground: true,
        ..physics::Body::default()
    });
    let mut acts = Vec::new();
    for _ in 0..6 {
        p.step(&w);
        acts.extend(p.take_acts());
    }
    assert!(
        acts.iter().any(|a| matches!(
            a,
            Act::Dig { pos, .. } if *pos == world::BlockPos::new(10, 62, 0)
        )),
        "{acts:?} {:?}",
        notes(&p)
    );
}
