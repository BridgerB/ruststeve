//! Issue 20f, the crystals by pillar and strike (agent E4, docs/problems/end-crystals.md), on
//! hand-built worlds: an end stone floor at y 63 (the feet at 64), a spike round column (8, 8)
//! with its bedrock and its crystal. The tests mirror clojurecraft's `dragon_test.clj` by name
//! where one exists.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::spikes::{self, Axis, Next, PillarRec, Role, Spike, SpikeState, StandPlan};
use plan::{Aim, Plan, Task};
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

fn set(w: &mut World, c: (i32, i32, i32), k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: c,
        state: state(k),
    }));
}

const CRYSTAL: i32 = 7;
const DRAGON: i32 = 40;
/// On the bedrock of a spike six high round (8, 8) on the floor at 64 (clojurecraft's
/// `crystal-at`).
const CRYSTAL_AT: [f64; 3] = [8.5, 71.0, 8.5];

/// End stone from y 0 to 63, air over it, 5 by 5 chunk columns round the origin; a spike of
/// radius `r` round (8, 8), obsidian from 64 to 69, its bedrock at 70 and its crystal on it; the
/// feet at `pos`, on the ground, health 20, end stone in hand.
fn world(r: i32, pos: [f64; 3]) -> World {
    let mut sections = vec![section(BlockKind::EndStone); 4];
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    let col = Arc::new(Column { min_y: 0, sections });
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 0,
        dimension: "minecraft:overworld".to_owned(),
        game_type: 0,
    }));
    for x in -2..=2 {
        for z in -2..=2 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    for dx in -r..=r {
        for dz in -r..=r {
            if dx * dx + dz * dz <= r * r + 1 {
                for y in 64..70 {
                    set(&mut w, (8 + dx, y, 8 + dz), BlockKind::Obsidian);
                }
            }
        }
    }
    set(&mut w, (8, 70, 8), BlockKind::Bedrock);
    w.step(&Event::Packet(S2c::AddEntity {
        id: CRYSTAL,
        uuid: [0; 16],
        kind: EntityKind::EndCrystal.id(),
        x: CRYSTAL_AT[0],
        y: CRYSTAL_AT[1],
        z: CRYSTAL_AT[2],
        velocity: [0.0; 3],
    }));
    w.inventory.slots.insert(36, (ItemKind::EndStone, 64));
    w.held = 0;
    at(&mut w, pos);
    w.player.health = Some(20.0);
    w.player.food = Some(20);
    w
}

fn at(w: &mut World, pos: [f64; 3]) {
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
}

fn pillar(w: &mut World, (x, z): (i32, i32), y0: i32, y1: i32, k: BlockKind) {
    for y in y0..=y1 {
        set(w, (x, y, z), k);
    }
}

fn gone(w: &mut World) {
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![CRYSTAL] }));
}

fn dragon(w: &mut World, pos: [f64; 3], phase: i32) {
    w.step(&Event::Packet(S2c::AddEntity {
        id: DRAGON,
        uuid: [1; 16],
        kind: EntityKind::EnderDragon.id(),
        x: pos[0],
        y: pos[1],
        z: pos[2],
        velocity: [0.0; 3],
    }));
    w.step(&Event::Packet(S2c::SetEntityData {
        id: DRAGON,
        entries: vec![(16, MetaValue::Int(phase))],
    }));
}

const R2: Spike = Spike {
    centre: [8, 8],
    radius: 2,
    top_y: 70,
    caged: false,
};

/// The pillar along north (column (8, 5)) up to the stand, its guard ring and its wall built.
fn walled() -> World {
    let mut w = world(2, [8.5, 70.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 69, BlockKind::EndStone);
    for c in [(9, 69, 5), (7, 69, 5), (8, 69, 4)] {
        set(&mut w, c, BlockKind::EndStone);
    }
    for c in [
        (8, 70, 6),
        (8, 71, 6),
        (9, 70, 5),
        (7, 70, 5),
        (8, 70, 4),
        (9, 71, 5),
        (7, 71, 5),
        (8, 71, 4),
        (8, 72, 5),
    ] {
        set(&mut w, c, BlockKind::EndStone);
    }
    w
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

#[test]
fn the_approach_stands_three_out_behind_a_wall() {
    assert_eq!(
        spikes::stand(&R2, Axis::North),
        StandPlan {
            axis: Axis::North,
            column: (8, 5),
            stand: (8, 70, 5),
            walls: [(8, 70, 6), (8, 71, 6)],
            on_spike: false,
        },
        "a spike of radius 2: the pillar's top is the stand"
    );
    let r4 = Spike { radius: 4, ..R2 };
    assert_eq!(
        spikes::stand(&r4, Axis::North),
        StandPlan {
            axis: Axis::North,
            column: (8, 3),
            stand: (8, 70, 5),
            walls: [(8, 70, 6), (8, 71, 6)],
            on_spike: true,
        },
        "a spike of radius 4: the pillar beside it, the stand on its top"
    );
    let w = world(2, [8.5, 64.0, 0.5]);
    assert_eq!(
        spikes::spike_for(&w, &[], CRYSTAL_AT),
        Some(R2),
        "read from its blocks"
    );
    let a = spikes::best_approach(&w, &R2, [8.5, 64.0, 0.5], &[], 0);
    assert_eq!(
        a.map(|a| (a.foot, a.plan.axis)),
        Some(((8, 64, 5), Axis::North)),
        "along the axis toward the feet"
    );
    let geo = Spike { radius: 3, ..R2 };
    assert_eq!(
        spikes::spike_for(&w, &[geo], CRYSTAL_AT),
        Some(geo),
        "the geography first"
    );
}

#[test]
fn the_planner_walks_pillars_walls_strikes_and_digs_down() {
    let st = SpikeState::default();
    let mut w = world(2, [8.5, 64.0, 0.5]);
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Walk {
            to: (8, 64, 5),
            approach: Some((8, 8, Axis::North))
        },
        "on the ground, the walk to the pillar's foot"
    );
    at(&mut w, [8.5, 64.0, 5.5]);
    let up = Next::Pillar {
        column: (8, 5),
        top: 70,
        axis: Axis::North,
        centre: [8, 8],
    };
    assert_eq!(spikes::next(&w, &st, 0), up, "at its foot, the pillar up");
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    at(&mut w, [8.5, 67.0, 5.5]);
    assert_eq!(spikes::next(&w, &st, 0), up, "half way up, on up");
    pillar(&mut w, (8, 5), 67, 69, BlockKind::EndStone);
    at(&mut w, [8.5, 70.0, 5.5]);
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Build((8, 70, 6), Role::Wall),
        "at the stand, the blast's line first: a block between at the feet's height"
    );
    set(&mut w, (8, 70, 6), BlockKind::EndStone);
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Build((8, 71, 6), Role::Wall),
        "then at the eye's"
    );
    set(&mut w, (8, 71, 6), BlockKind::EndStone);
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Strike(CRYSTAL),
        "the wall up, the strike at once: no other block first"
    );
    // The box, when one is wanted (a hold up high), goes up in this order.
    assert_eq!(
        spikes::box_gap(&w, (8, 70, 5), &[(8, 70, 6), (8, 71, 6)]),
        Some(((9, 69, 5), Role::Ledge)),
        "a ledge where nothing would hold a side"
    );
    for c in [
        (9, 69, 5),
        (7, 69, 5),
        (8, 69, 4),
        (9, 70, 5),
        (7, 70, 5),
        (8, 70, 4),
    ] {
        set(&mut w, c, BlockKind::EndStone);
    }
    for c in [(9, 71, 5), (7, 71, 5), (8, 71, 4)] {
        set(&mut w, c, BlockKind::EndStone);
    }
    assert_eq!(
        spikes::box_gap(&w, (8, 70, 5), &[(8, 70, 6), (8, 71, 6)]),
        Some(((9, 72, 5), Role::Ledge)),
        "a block over a side first: nothing else holds the roof"
    );
    set(&mut w, (9, 72, 5), BlockKind::EndStone);
    assert_eq!(
        spikes::box_gap(&w, (8, 70, 5), &[(8, 70, 6), (8, 71, 6)]),
        Some(((8, 72, 5), Role::Roof))
    );
    set(&mut w, (8, 72, 5), BlockKind::EndStone);
    assert!(spikes::boxed(&w, (8, 70, 5)), "the box whole");
    gone(&mut w);
    let rec = SpikeState {
        pillars: vec![PillarRec {
            x: 8,
            z: 5,
            base: 64,
        }],
        ..SpikeState::default()
    };
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::DigDown((8, 69, 5)),
        "the crystal gone, the way down: end stone on end stone, by the pillar's record"
    );
    for y in 65..=69 {
        set(&mut w, (8, y, 5), BlockKind::Air);
    }
    at(&mut w, [8.5, 65.0, 5.5]);
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::DigDown((8, 64, 5)),
        "one block left under the feet: that one too"
    );
    set(&mut w, (8, 64, 5), BlockKind::Air);
    at(&mut w, [8.5, 64.0, 5.5]);
    assert_eq!(spikes::next(&w, &rec, 0), Next::Done, "on the ground: down");
    // On a wide spike's top with the crystal gone, back to the pillar's top.
    let mut wide = world(4, [8.5, 70.0, 3.5]);
    pillar(&mut wide, (8, 3), 64, 69, BlockKind::EndStone);
    assert_eq!(
        spikes::next(&wide, &st, 0),
        Next::Walk {
            to: (8, 70, 5),
            approach: Some((8, 8, Axis::North))
        },
        "on the pillar's top the walk onto the spike: the pillar's own block is no spike"
    );
    gone(&mut wide);
    at(&mut wide, [8.5, 70.0, 5.5]);
    let wrec = SpikeState {
        pillars: vec![PillarRec {
            x: 8,
            z: 3,
            base: 64,
        }],
        ..SpikeState::default()
    };
    assert_eq!(
        spikes::next(&wide, &wrec, 0),
        Next::Walk {
            to: (8, 70, 3),
            approach: None
        }
    );
    // Boxed in on the wide spike's top: the side toward the pillar opened first.
    for c in [(8, 70, 4), (8, 71, 4)] {
        set(&mut wide, c, BlockKind::EndStone);
    }
    assert_eq!(spikes::next(&wide, &wrec, 0), Next::Open((8, 70, 4)));
    set(&mut wide, (8, 70, 4), BlockKind::Air);
    assert_eq!(spikes::next(&wide, &wrec, 0), Next::Open((8, 71, 4)));
}

#[test]
fn a_pillar_with_a_gap_under_it_is_still_the_way_down() {
    // gym dragon-c run 1: the dragon smashed the pillar under the bot, which fell to a stump with
    // a gap under it. A placed block no End ground is made of is a pillar with no record.
    let mut w = world(2, [8.5, 70.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 65, BlockKind::Cobblestone);
    set(&mut w, (8, 69, 5), BlockKind::Cobblestone);
    gone(&mut w);
    assert_eq!(
        spikes::next(&w, &SpikeState::default(), 0),
        Next::DigDown((8, 69, 5))
    );
}

#[test]
fn a_bot_falling_from_a_pillar_waits_and_asks_for_no_walk() {
    let mut w = world(2, [8.5, 66.3, 5.5]);
    if let Some(b) = w.body.as_mut() {
        b.on_ground = false;
    }
    assert_eq!(
        spikes::next(&w, &SpikeState::default(), 0),
        Next::Wait("falling".to_owned())
    );
}

#[test]
fn the_climb_jumps_and_puts_the_block_in_once_clear() {
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: pillar up from (8, 64, 5)"),
        "{:?}",
        notes(&p)
    );
    let jumped = p.spikes_step(&w, 101);
    assert!(
        jumped.is_some_and(|c| c.jump),
        "a jump from the ground: {jumped:?}"
    );
    assert!(p.take_acts().is_empty(), "nothing while on the ground");
    let mut rising = w.clone();
    at(&mut rising, [8.5, 64.6, 5.5]);
    if let Some(b) = rising.body.as_mut() {
        b.on_ground = false;
    }
    p.spikes_step(&rising, 102);
    assert!(
        p.take_acts().is_empty(),
        "nothing while the body is still in the cell"
    );
    at(&mut w, [8.5, 65.17, 5.5]);
    if let Some(b) = w.body.as_mut() {
        b.on_ground = false;
    }
    let clear = p.spikes_step(&w, 103);
    assert_eq!(
        p.take_acts(),
        vec![Act::UseOn {
            pos: world::BlockPos::new(8, 63, 5),
            face: 1
        }],
        "clear of it, the block in against the top of the one under"
    );
    assert_eq!(
        clear.and_then(|c| c.pitch),
        Some(90.0),
        "looking straight down"
    );
}

#[test]
fn hurt_on_the_ground_the_crystals_wait_for_the_health() {
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    w.player.health = Some(12.0);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p).iter().any(|n| n == "spikes: no climb"),
        "{:?}",
        notes(&p)
    );
    let held = p.spikes_step(&w, 101);
    assert!(
        held.is_some_and(|c| c.sneak && !c.jump),
        "held still: {held:?}"
    );
}

#[test]
fn the_dragon_holds_the_climb_at_the_foot_and_the_way_down_in_the_box() {
    // Strafing 50 off: no climb.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    dragon(&mut w, [8.5, 64.0, 55.5], 0);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(notes(&p).iter().any(|n| n == "spikes: no climb"));
    // Perched 50 off: the climb.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    dragon(&mut w, [8.5, 64.0, 55.5], 6);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: pillar up from"))
    );
    // Flying 100 off: the climb.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    dragon(&mut w, [8.5, 64.0, 105.5], 0);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: pillar up from"))
    );
    // Strafing 30 off with the bot half way up a pillar and no clutch bucket: a strafe is
    // fireballs on the way (frontier 45947478869 lane 1), so down, not on up to the stand.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    dragon(&mut w, [8.5, 67.0, 35.5], 1);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert!(matches!(p.spikes_task(&w, 100), Some(Task::Dig(_))));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: down from a fireball at")),
        "{:?}",
        notes(&p)
    );
    // Charging 30 off: held still in the column.
    dragon(&mut w, [8.5, 67.0, 35.5], 8);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: hold in the shaft from the charge"),
        "{:?}",
        notes(&p)
    );
    // Charging 30 off with the bot boxed in at its stand: the strike, the stand noted closed.
    let mut w = walled();
    dragon(&mut w, [8.5, 70.0, 35.5], 8);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Strike(_))));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: stand walled at (8, 70, 5)")
    );
    // The crystal gone, the dragon strafing 30 off: down, box or not (a fireball's cloud takes
    // whoever is within 4 of its impact, walls or not); boxed, no side is open, so the dig.
    gone(&mut w);
    dragon(&mut w, [8.5, 70.0, 35.5], 1);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 69, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: down from a fireball at"))
    );
    // The dragon gone far: down.
    w.step(&Event::Packet(S2c::RemoveEntities { ids: vec![DRAGON] }));
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 69, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
}

#[test]
fn a_pillar_that_rises_no_block_in_1200_ticks_fails_and_says_why() {
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(p.spikes_step(&w, 1400).is_none());
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: pillar stalled")),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.spikes.stalls, 1);
    match spikes::next(&w, &p.spikes, 1401) {
        Next::Walk {
            approach: Some((8, 8, a)),
            ..
        } => assert_ne!(a, Axis::North, "another approach"),
        n => panic!("{n:?}"),
    }
}

#[test]
fn the_strike_attacks_the_crystal_and_notes_it_gone() {
    let mut w = walled();
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    p.spikes_step(&w, 101);
    assert_eq!(p.take_acts(), vec![Act::Attack { entity: CRYSTAL }]);
    assert!(notes(&p).iter().any(|n| n == "spikes: strike 7"));
    gone(&mut w);
    assert!(p.spikes_step(&w, 102).is_none());
    assert!(
        notes(&p).iter().any(|n| n == "spikes: crystal gone 7"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.spikes.gone, 1);
}

#[test]
fn never_a_strike_under_danger_health() {
    let mut w = walled();
    w.player.health = Some(7.0);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    p.spikes_step(&w, 101);
    assert!(p.take_acts().is_empty());
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: no strike under danger health")
    );
}

#[test]
fn a_cage_is_struck_through_its_bars() {
    // Parity e03 (E6, run 45864998071): through the intact bars at 2 off, 0.42 lost.
    let mut w = walled();
    for c in [(8, 70, 6), (8, 71, 6)] {
        set(&mut w, c, BlockKind::IronBars);
    }
    let caged = spikes::spike_for(&w, &[], CRYSTAL_AT);
    assert_eq!(caged.map(|s| s.caged), Some(true));
    assert_eq!(
        spikes::next(&w, &SpikeState::default(), 0),
        Next::Strike(CRYSTAL),
        "through the bars"
    );
    let fallback = SpikeState {
        bars_shield: false,
        ..SpikeState::default()
    };
    assert_eq!(
        spikes::next(&w, &fallback, 0),
        Next::Bar((8, 70, 6)),
        "the fallback: dug, and end stone put back"
    );
    set(&mut w, (8, 70, 6), BlockKind::Air);
    assert_eq!(
        spikes::next(&w, &fallback, 0),
        Next::Build((8, 70, 6), Role::Wall),
        "the bar dug: end stone in its place"
    );
}

#[test]
fn a_blast_takes_the_wall_and_a_crystal_left_gets_it_again() {
    // Parity e03: both wall cells are air after the blast. A crystal still there is walled
    // again before any strike.
    let mut w = walled();
    for c in [(8, 70, 6), (8, 71, 6)] {
        set(&mut w, c, BlockKind::Air);
    }
    assert_eq!(
        spikes::next(&w, &SpikeState::default(), 0),
        Next::Build((8, 70, 6), Role::Wall)
    );
}

#[test]
fn the_spikes_read_from_end_toml_rows() {
    // E1's `[[spike]]` rows (config/end.toml, probe 45863427332), with its extra fields.
    #[derive(serde::Deserialize)]
    struct Rows {
        spike: Vec<Spike>,
    }
    let text =
        "[[spike]]\ncentre = [-34, 24]\ntop_y = 79\nradius = 2\ncaged = true\nground_y = 62\n";
    let rows: Rows = toml::from_str(text).unwrap();
    assert_eq!(
        rows.spike,
        vec![Spike {
            centre: [-34, 24],
            radius: 2,
            top_y: 79,
            caged: true,
        }]
    );
}

#[test]
fn config_end_toml_holds_the_ten_spikes() {
    // E1's measured rows (config/end.toml, probe 45863427332), as the shell loads them.
    #[derive(serde::Deserialize)]
    struct Rows {
        spike: Vec<Spike>,
    }
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../config/end.toml"
    ))
    .unwrap();
    let rows: Rows = toml::from_str(&text).unwrap();
    assert_eq!(rows.spike.len(), 10);
    assert_eq!(rows.spike.iter().filter(|s| s.caged).count(), 2);
    let first = Spike {
        centre: [12, 39],
        radius: 2,
        top_y: 76,
        caged: false,
    };
    assert_eq!(rows.spike.first(), Some(&first));
}

#[test]
fn out_of_blocks_half_way_up_the_bot_digs_down() {
    let mut w = world(2, [8.5, 67.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    w.inventory.slots.remove(&36);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 66, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
}

#[test]
fn a_blow_inside_the_box_is_noted() {
    let mut w = walled();
    let mut p = Plan::new(Aim::Race);
    p.spikes_step(&w, 100);
    w.player.health = Some(15.0);
    p.spikes_step(&w, 101);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: blow in the box at (8, 70, 5)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn a_fall_from_a_pillar_is_caught_with_the_water_bucket_and_the_water_taken_back() {
    // Parity e06 (E6): a 21-block fall onto water poured while falling cost 0. Race 45871173293
    // lane 0 fell 8 for 7.5 with a water bucket held.
    let mut w = world(2, [8.5, 75.0, 5.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    let fall = |w: &mut World, y: f64| {
        at(w, [8.5, y, 5.5]);
        if let Some(b) = w.body.as_mut() {
            b.on_ground = false;
            b.vel = [0.0, -0.8, 0.0];
        }
    };
    fall(&mut w, 75.0);
    let mut p = Plan::new(Aim::Race);
    let c = p.spikes_clutch(&w, 100);
    assert_eq!(c.and_then(|c| c.pitch), Some(90.0), "looking straight down");
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: clutch begun")),
        "{:?}",
        notes(&p)
    );
    assert_eq!(
        p.take_acts(),
        vec![Act::ReleaseUse, Act::Hold { slot: 1 }],
        "the use key let go, the bucket to hand"
    );
    w.held = 1;
    fall(&mut w, 70.0);
    p.spikes_clutch(&w, 101);
    assert!(p.take_acts().is_empty(), "too high to pour yet");
    fall(&mut w, 66.5);
    p.spikes_clutch(&w, 102);
    assert_eq!(
        p.take_acts(),
        vec![Act::UseItem],
        "the first tick in reach (the server places it on handling the packet)"
    );
    fall(&mut w, 65.0);
    p.spikes_clutch(&w, 103);
    assert!(p.take_acts().is_empty(), "one pour only");
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    fall(&mut w, 64.5);
    p.spikes_clutch(&w, 104);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: clutch poured"))
    );
    set(&mut w, (8, 64, 5), BlockKind::Water);
    at(&mut w, [8.5, 64.0, 5.5]);
    if let Some(b) = w.body.as_mut() {
        b.in_water = true;
    }
    p.take_acts();
    p.spikes_clutch(&w, 104);
    assert_eq!(
        p.take_acts(),
        vec![Act::UseItem],
        "landed: the water taken back"
    );
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    assert!(p.spikes_clutch(&w, 105).is_none());
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: clutch over")),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.spikes.clutches, 1);
}

#[test]
fn the_way_down_centres_first_and_steps_into_a_dug_shaft() {
    // Race 45885217441 lane 0: the dig under a wide spike's pillar top left the body on the
    // spike's edge beside the shaft, "no approach" and then "stranded" to the cap.
    let mut w = world(4, [8.5, 70.0, 3.85]);
    pillar(&mut w, (8, 3), 64, 69, BlockKind::EndStone);
    gone(&mut w);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 3,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: centre over the pillar")),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(
        p.spikes.doing,
        Some(spikes::Doing::Step { sneak: true, .. })
    ));
    // The shaft dug with the body held up beside it: into it.
    set(&mut w, (8, 69, 3), BlockKind::Air);
    let rec = SpikeState {
        pillars: p.spikes.pillars.clone(),
        ..SpikeState::default()
    };
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::Step {
            to: (8, 70, 3),
            sneak: false
        }
    );
    // On the spike's top beside it: a step in; farther off, the walk to beside it.
    at(&mut w, [8.5, 70.0, 4.5]);
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::Step {
            to: (8, 70, 3),
            sneak: false
        }
    );
    at(&mut w, [8.5, 70.0, 7.5]);
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::Walk {
            to: (8, 70, 4),
            approach: None
        }
    );
    // Off the axis (race 45895161187 lane 0 walked to a diagonal "beside" 900 times): the cell
    // beside it along an axis.
    at(&mut w, [9.5, 70.0, 5.5]);
    assert_eq!(
        spikes::next(&w, &rec, 0),
        Next::Walk {
            to: (8, 70, 4),
            approach: None
        }
    );
}

#[test]
fn a_48_block_fall_at_terminal_speed_pours_once_in_the_last_tick_in_reach() {
    // Race 45885217441 lane 1: flung 48 over the ground, the bucket held, never poured. The pour
    // is predicted from the velocity (gravity 0.08, drag 0.98), never waited for.
    let ground = 54.0;
    let (mut y, mut v) = (ground + 48.0, -3.9);
    let mut pours = Vec::new();
    while y > ground {
        if spikes::pour_now(y, v, ground) {
            pours.push((y, v));
        }
        y += v;
        v = (v - 0.08) * 0.98;
    }
    // E6's parity e09-pour (run 45929976619): the server judges the use from the body's position
    // before this tick's move, and places water with the eye within 4.5 of the ground there. So
    // the pour goes by the current height, the feet `POUR_FEET` (2.7) or less over the ground.
    // At terminal speed a tick falls 3.9, more than that band, so a fall's phase can skip it:
    // this one's does not.
    assert!(!pours.is_empty(), "{pours:?}");
    let (py, _) = pours[0];
    assert!(py > ground && py - ground <= spikes::POUR_FEET, "{py}");
    // Every start from rest 5 to 60 over the ground: under 2.78 a tick (about 100 blocks of
    // fall), a tick always lands in reach, and exactly one pour goes.
    for h in 5..=60 {
        let (mut y, mut v) = (ground + f64::from(h), 0.0);
        let mut n = 0;
        while y > ground {
            if spikes::pour_now(y, v, ground) {
                n += 1;
            }
            y += v;
            v = (v - 0.08) * 0.98;
        }
        assert!(n >= 1, "a fall from {h}: {n}");
    }
}

#[test]
fn up_a_pillar_a_provoked_enderman_gets_no_pocket() {
    // Race 45890149072 lane 0: a sealed pocket on the pillar at y 80 held the bot still for the
    // charging dragon. No enderman reaches a one-wide column's top: the crystals' work goes on.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 60,
        uuid: [3; 16],
        kind: EntityKind::Enderman.id(),
        x: 14.5,
        y: 64.0,
        z: 0.5,
        velocity: [0.0; 3],
    }));
    w.step(&Event::Packet(S2c::SetEntityData {
        id: 60,
        entries: vec![(17, MetaValue::Bool(true))],
    }));
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert!(p.spikes_up_high(&w));
    for t in 100..110 {
        w.ticks = t;
        p.step(&w);
        p.take_acts();
    }
    let n = notes(&p);
    assert!(
        n.iter().any(|n| n == "spikes: up high, no enderman pocket"),
        "{n:?}"
    );
    assert!(
        !n.iter().any(|n| n.starts_with("provoked enderman")),
        "{n:?}"
    );
    // On the ground the enderman rule answers as before.
    let mut ground = w.clone();
    at(&mut ground, [12.5, 64.0, 12.5]);
    let fresh = Plan::new(Aim::Race);
    assert!(!fresh.spikes_up_high(&ground));
}

#[test]
fn a_body_held_up_by_the_floor_beside_steps_onto_it_first() {
    // E3's race 45890445738: x 14.2 half on a stair's step over the bunker's air; every walk from
    // the feet's cell (no floor) was refused for 14,000 ticks.
    let mut w = world(2, [8.2, 64.0, 0.5]);
    set(&mut w, (8, 63, 0), BlockKind::Air);
    assert_eq!(
        spikes::next(&w, &SpikeState::default(), 0),
        Next::Step {
            to: (7, 64, 0),
            sneak: true
        }
    );
}

#[test]
fn hurt_on_the_way_down_with_the_dragon_near_the_bot_holds_in_its_shaft() {
    // Race 45895161187 lane 1: going down at 9 health, struck dead.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    gone(&mut w);
    w.player.health = Some(9.0);
    dragon(&mut w, [8.5, 67.0, 45.5], 0);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: hold in the shaft, hurt"),
        "{:?}",
        notes(&p)
    );
    // The dragon perched: down.
    dragon(&mut w, [0.5, 66.0, 0.5], 6);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 66, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
}

#[test]
fn a_breath_on_a_pillar_top_is_dug_down_out_of() {
    // Race 45899557907 lane 1: a breath on its pillar's top at y 83; the walk out of it found no
    // way ("NoPath" every tick), and the bot fell.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 50,
        uuid: [2; 16],
        kind: EntityKind::AreaEffectCloud.id(),
        x: 8.5,
        y: 67.0,
        z: 5.5,
        velocity: [0.0; 3],
    }));
    w.step(&Event::Packet(S2c::SetEntityData {
        id: 50,
        entries: vec![(8, MetaValue::Float(3.0))],
    }));
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert!(p.breath_step(&w, 100).is_none(), "no walk out up high");
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 66, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: dig down out of the breath"))
    );
}

#[test]
fn a_clutch_that_begins_while_eating_still_pours() {
    // Race 45899557907 lane 0: the eat row cut in the tick the fall began; no pour went.
    let mut w = world(2, [8.5, 75.0, 5.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    w.inventory.slots.insert(36, (ItemKind::CookedBeef, 8));
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    let fall = |w: &mut World, y: f64| {
        at(w, [8.5, y, 5.5]);
        if let Some(b) = w.body.as_mut() {
            b.on_ground = false;
            b.vel = [0.0, -0.8, 0.0];
        }
    };
    fall(&mut w, 75.0);
    let mut p = Plan::new(Aim::Race);
    p.task = Task::Eat(plan::eat::Eat::new(&w, ItemKind::CookedBeef, 99));
    assert!(p.spikes_clutch(&w, 100).is_some());
    assert_eq!(p.task, Task::Idle, "the eating dropped");
    assert_eq!(
        p.take_acts(),
        vec![Act::ReleaseUse, Act::Hold { slot: 1 }],
        "the use key let go, the bucket's slot selected"
    );
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: clutch stops the eating")
    );
    w.held = 1;
    fall(&mut w, 65.0);
    p.spikes_clutch(&w, 101);
    assert_eq!(p.take_acts(), vec![Act::UseItem], "the pour on its tick");
}

#[test]
fn up_a_pillar_the_water_bucket_is_kept_in_the_hotbar() {
    let mut w = world(2, [8.5, 67.0, 5.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    w.inventory.slots.insert(12, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    p.spikes_step(&w, 100);
    assert_eq!(
        p.take_acts(),
        vec![Act::Click {
            slot: 12,
            button: 8,
            mode: 2
        }]
    );
    // In the hotbar already: nothing.
    w.inventory.slots.remove(&12);
    w.inventory.slots.insert(44, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    p.spikes_step(&w, 100);
    assert!(p.take_acts().is_empty());
}

#[test]
fn a_body_flung_sideways_pours_on_the_column_it_will_land_on() {
    // Race 45903987747: both bots knocked off sideways near the island's edge, the column under
    // the fall's start not the one landed on.
    let g = 54.0;
    // By the current height over the ground under the body (E6's parity e09-pour): in reach,
    // the pour; over it, none yet; no ground under the body, none.
    assert!(spikes::pour_now_at(g + 2.5, -1.5, Some(g), None));
    assert!(!spikes::pour_now_at(g + 3.0, -0.5, Some(g), Some(g)));
    assert!(!spikes::pour_now_at(g + 2.5, -1.5, None, Some(g)));
}

#[test]
fn the_landing_water_is_poured_from_the_first_block_up_and_dropped_into_from_the_top() {
    // Race 45916169323 lane 1: a pour on the pillar's top made a waterfall too slow to ride, and
    // the bot drowned in it. The water goes beside the foot; the way down is a drop.
    // Race 45949063425 lane 0: poured from the foot, it ran into the foot's cell and the bot,
    // in water, rose no block in three climbs. At the foot: the climb, no pour.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        !notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: landing water poured")),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Climb(_))));
    // The first block up: the climb hands back for the pour.
    set(&mut w, (8, 64, 5), BlockKind::EndStone);
    at(&mut w, [8.5, 65.0, 5.5]);
    let mut c = match p.spikes.doing.take() {
        Some(spikes::Doing::Climb(c)) => c,
        d => panic!("{d:?}"),
    };
    c.top = 64;
    p.spikes.doing = Some(spikes::Doing::Climb(c));
    p.spikes_step(&w, 120);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: climb paused for the landing water at (8, 65, 5)"),
        "{:?}",
        notes(&p)
    );
    p.spikes.doing = None;
    assert_eq!(p.spikes_task(&w, 121), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: landing water poured at (9, 64, 5)"),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Bucket { .. })));
    // The pillar up, the crystal struck: from the top, off the side over the water.
    p.spikes.doing = None;
    pillar(&mut w, (8, 5), 64, 69, BlockKind::EndStone);
    set(&mut w, (8, 70, 6), BlockKind::EndStone);
    set(&mut w, (8, 71, 6), BlockKind::EndStone);
    set(&mut w, (9, 64, 5), BlockKind::Water);
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    gone(&mut w);
    at(&mut w, [8.5, 70.0, 5.5]);
    p.spikes.pillars.clear();
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 200), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: waterfall down from (8, 70, 5)"),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(
        p.spikes.doing,
        Some(spikes::Doing::Step {
            to: (9, 70, 5),
            sneak: false,
            ..
        })
    ));
    // Landed in the water: down, and the water taken back.
    p.spikes.doing = None;
    at(&mut w, [9.5, 64.0, 5.5]);
    assert_eq!(p.spikes_task(&w, 300), Some(Task::Idle));
    let n = notes(&p);
    assert!(
        n.iter().any(|n| n.starts_with("spikes: waterfall down to")),
        "{n:?}"
    );
    assert!(
        n.iter()
            .any(|n| n == "spikes: landing water taken back at (9, 64, 5)"),
        "{n:?}"
    );
    assert!(p.spikes.pillars.is_empty());
    assert_eq!(p.spikes.waterfalls, 1);
}

#[test]
fn a_pool_is_made_by_the_bunker_and_refills_the_buckets() {
    let mut w = world(2, [20.5, 64.0, 20.5]);
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    // The bunker 6 off: the pool keeps 5 from it (`POOL_KEEP`).
    p.bunker = Some([14, 64, 20]);
    let dug = |t: Option<Task>| match t {
        Some(Task::Dig(d)) => d.target,
        t => panic!("{t:?}"),
    };
    // A 2 by 2 square (race 45920382795: a 2 by 1 pit is no infinite source).
    let square = [(21, 63, 20), (22, 63, 20), (21, 63, 21), (22, 63, 21)];
    for (i, c) in square.iter().enumerate() {
        assert_eq!(dug(p.spikes_task(&w, 100 + i as u64)), *c);
        set(&mut w, *c, BlockKind::Air);
    }
    assert_eq!(p.spikes_task(&w, 105), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: water pool poured at (21, 63, 20)"),
        "{:?}",
        notes(&p)
    );
    // Both diagonal corners poured, the square full: the pool made, a bucket refilled.
    p.spikes.doing = None;
    for c in square {
        set(&mut w, c, BlockKind::Water);
    }
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    w.inventory.slots.insert(38, (ItemKind::Bucket, 1));
    assert_eq!(p.spikes_task(&w, 106), Some(Task::Idle));
    let n = notes(&p);
    assert!(
        n.iter()
            .any(|n| n == "spikes: water pool at (21, 63, 20) and (22, 63, 21)"),
        "{n:?}"
    );
    assert!(
        n.iter().any(|n| n == "spikes: refilled at (21, 63, 20)"),
        "{n:?}"
    );
    assert!(p.spikes.pool_made);
}

/// The clutch's water bucket in the hotbar's last slot: no climb begins without it.
fn water(w: &mut World) {
    w.inventory.slots.insert(44, (ItemKind::WaterBucket, 1));
}

#[test]
fn no_climb_begins_without_a_water_bucket_in_the_hotbar() {
    // Race 45908299055: both bots climbed with no water at all, and fell dead.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p).iter().any(|n| n == "spikes: no climb: no water"),
        "{:?}",
        notes(&p)
    );
    assert!(!matches!(p.spikes.doing, Some(spikes::Doing::Climb(_))));
    // One in the main slots: moved to the hotbar, and still no climb this time.
    w.inventory.slots.insert(12, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert_eq!(
        p.take_acts(),
        vec![Act::Click {
            slot: 12,
            button: 8,
            mode: 2
        }]
    );
    // One bucket, in the hotbar: the climb, with no landing water (the way down is the dig).
    w.inventory.slots.remove(&12);
    water(&mut w);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Climb(_))));
    assert!(
        p.spikes.landings.is_empty(),
        "no landing water from the clutch's one bucket"
    );
    // Two, one in the hotbar: the climb, and the landing water from the first block up (wall
    // 25), the clutch's still in hand.
    w.inventory.slots.insert(12, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Climb(_))));
    assert!(p.spikes.landings.is_empty());
    p.spikes.doing = None;
    set(&mut w, (8, 64, 5), BlockKind::EndStone);
    at(&mut w, [8.5, 65.0, 5.5]);
    p.spikes_task(&w, 120);
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Bucket { .. })));
    assert_eq!(p.spikes.landings.len(), 1);
}

#[test]
fn the_waterfall_ride_is_kept_and_its_pour_is_no_bar_to_the_way_back() {
    // Race 45912100302 lane 1: mid-ride the escape out of water walked the bot back onto the
    // spike's top, and the pillar's watered top then read as no way down: stranded to the cap.
    let mut w = world(4, [7.5, 66.0, 3.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    if let Some(b) = w.body.as_mut() {
        b.on_ground = false;
        b.in_water = true;
        b.vel = [0.0, -0.3, 0.0];
    }
    let mut p = Plan::new(Aim::Race);
    p.spikes.riding = Some((8, 3));
    let c = p.spikes_clutch(&w, 100);
    assert_eq!(c.and_then(|c| c.pitch), Some(90.0));
    assert!(c.is_some_and(|c| !c.forward), "carried down, no other step");
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: riding the waterfall")
    );
    // On the spike's top with the pillar's top watered: the walk back to it.
    let mut w = world(4, [8.5, 70.0, 5.5]);
    pillar(&mut w, (8, 3), 64, 69, BlockKind::EndStone);
    set(&mut w, (8, 70, 3), BlockKind::Water);
    gone(&mut w);
    let st = SpikeState {
        pillars: vec![PillarRec {
            x: 8,
            z: 3,
            base: 64,
        }],
        ..SpikeState::default()
    };
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Walk {
            to: (8, 70, 3),
            approach: None
        }
    );
}

#[test]
fn no_pool_site_says_why() {
    // Race 45912100302 made no pool and wrote nothing of why.
    let mut w = world(2, [20.5, 64.0, 20.5]);
    for x in 11..=29 {
        for z in 11..=29 {
            set(&mut w, (x, 63, z), BlockKind::Obsidian);
        }
    }
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([20, 64, 20]);
    p.spikes_task(&w, 100);
    let n = notes(&p);
    assert!(n.iter().any(|n| n == "spikes: no pool: no site"), "{n:?}");
    let why = p
        .events
        .iter()
        .find(|e| e.what == "spikes: no pool: no site")
        .map(|e| e.why.clone())
        .unwrap_or_default();
    assert!(why.contains("Obsidian"), "{why}");
    // No site is no bar: on to the crystals with the buckets in hand (race 45949063425 lane 1).
    assert!(n.iter().any(|n| n.starts_with("spikes: walk to")), "{n:?}");
}

#[test]
fn with_every_approach_of_one_spike_refused_the_next_spike_is_tried() {
    // Race 45949063425 lane 1: the four approaches of one spike refused in turn, then back to
    // it; none of the nine others tried.
    let mut w = world(2, [14.5, 64.0, 8.5]);
    for dx in -2..=2_i32 {
        for dz in -2..=2_i32 {
            if dx * dx + dz * dz <= 5 {
                for y in 64..70 {
                    set(&mut w, (24 + dx, y, 8 + dz), BlockKind::Obsidian);
                }
            }
        }
    }
    set(&mut w, (24, 70, 8), BlockKind::Bedrock);
    w.step(&Event::Packet(S2c::AddEntity {
        id: 8,
        uuid: [5; 16],
        kind: EntityKind::EndCrystal.id(),
        x: 24.5,
        y: 71.0,
        z: 8.5,
        velocity: [0.0; 3],
    }));
    // The nearer spike first.
    match spikes::next(&w, &SpikeState::default(), 0) {
        Next::Walk {
            approach: Some((8, 8, _)),
            ..
        } => {}
        n => panic!("{n:?}"),
    }
    let st = SpikeState {
        refused: Axis::ALL.map(|a| (8, 8, a, u64::MAX)).to_vec(),
        ..SpikeState::default()
    };
    match spikes::next(&w, &st, 0) {
        Next::Walk {
            approach: Some((24, 8, _)),
            ..
        } => {}
        n => panic!("{n:?}"),
    }
}

#[test]
fn shut_in_a_pocket_the_bot_digs_out_at_once() {
    // Race 45949063425 lane 1: sealed in its bunker, each approach was walked at and refused,
    // 40 ticks each, before the dig-out began.
    let mut w = world(2, [13.5, 61.0, 0.5]);
    set(&mut w, (13, 61, 0), BlockKind::Air);
    set(&mut w, (13, 62, 0), BlockKind::Air);
    assert!(spikes::shut_in(&w, (13, 61, 0)));
    match spikes::next(&w, &SpikeState::default(), 0) {
        Next::DigOut { .. } => {}
        n => panic!("{n:?}"),
    }
    // On the open floor: not shut in, the walk.
    let w = world(2, [13.5, 64.0, 0.5]);
    assert!(!spikes::shut_in(&w, (13, 64, 0)));
}

#[test]
fn a_dig_out_climbs_to_the_ground_and_fails_when_it_gets_no_nearer() {
    // The frontier race 45918666403 lane 2: from a notch under its bunker the stair rose only to
    // the foot's height under the ground, and every walk was refused for 19,560 ticks.
    let mut w = world(2, [13.5, 57.0, 0.5]);
    for y in 57..=63 {
        set(&mut w, (13, y, 0), BlockKind::Air);
    }
    let all = Axis::ALL.map(|a| (8, 8, a, u64::MAX));
    let st = SpikeState {
        refused: all.to_vec(),
        ..SpikeState::default()
    };
    match spikes::next(&w, &st, 0) {
        Next::DigOut { cell, .. } => assert_eq!(cell, (12, 58, 0)),
        n => panic!("{n:?}"),
    }
    // Three steps open: up them, toward the ground at 64, not the foot's height.
    for c in [
        (12, 58, 0),
        (12, 59, 0),
        (12, 60, 0),
        (11, 59, 0),
        (11, 60, 0),
        (11, 61, 0),
        (10, 60, 0),
        (10, 61, 0),
    ] {
        set(&mut w, c, BlockKind::Air);
    }
    assert_eq!(
        spikes::next(&w, &st, 0),
        Next::Walk {
            to: (10, 60, 0),
            approach: None
        }
    );
    // No nearer in 600 ticks: the dig-out fails, says why and hands back.
    let w = {
        let mut w = world(2, [13.5, 57.0, 0.5]);
        for y in 57..=63 {
            set(&mut w, (13, y, 0), BlockKind::Air);
        }
        w
    };
    let mut p = Plan::new(Aim::Race);
    p.spikes.refused = all.to_vec();
    assert!(matches!(p.spikes_task(&w, 100), Some(Task::Dig(_))));
    p.spikes.refused = all.to_vec();
    assert_eq!(p.spikes_task(&w, 800), None);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: dig-out: no progress at (13, 57, 0)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_pool_is_never_sited_by_the_bunker_or_its_stair() {
    // Race 45930464018 lane 0: a misplaced pour ran down the bunker's stair and drowned the bot
    // in its bunker. The pool keeps 5 columns from the bunker's column, and is never uphill of it.
    let mut w = world(2, [20.5, 64.0, 20.5]);
    // The bunker's stair out east of it: open cells stepping up to the ground.
    for (i, x) in (21..=24).enumerate() {
        let y = 60 + i32::try_from(i).unwrap_or(0);
        for dy in 0..=2 {
            set(&mut w, (x, y + dy, 20), BlockKind::Air);
        }
    }
    let bunker = [20, 60, 20];
    let (stand, sq) = spikes::pool_site(&w, (20, 64, 20), Some(bunker)).unwrap();
    let keep = |c: (i32, i32, i32)| (c.0 - bunker[0]).abs().max((c.2 - bunker[2]).abs());
    assert!(keep(stand) >= spikes::POOL_KEEP, "{stand:?}");
    assert!(sq.iter().all(|c| keep(*c) >= spikes::POOL_KEEP), "{sq:?}");
    assert!(stand.1 <= 64, "not uphill of the bunker: {stand:?}");
}

#[test]
fn the_pool_is_worked_from_its_stand_or_given_up() {
    // Race 45934621769 lane 0: back in its bunker 6 off, the dig of a pool cell was refused 25
    // times to the cap.
    let mut w = world(2, [20.5, 64.0, 20.5]);
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::WaterBucket, 1));
    let mut p = Plan::new(Aim::Race);
    p.bunker = Some([14, 64, 20]);
    assert!(matches!(p.spikes_task(&w, 100), Some(Task::Dig(_))));
    // Shut in a pit 3 off its stand: no way back to it, the pool given up.
    for y in 61..=63 {
        set(&mut w, (17, y, 20), BlockKind::Air);
    }
    at(&mut w, [17.5, 61.0, 20.5]);
    p.spikes_task(&w, 200);
    assert!(
        notes(&p).iter().any(|n| n == "spikes: no pool: given up"),
        "{:?}",
        notes(&p)
    );
    assert!(p.spikes.pool_off);
}

#[test]
fn landing_water_left_by_a_refused_approach_is_taken_back() {
    // Race 45937952400 lane 0: poured at one pillar's foot, that approach refused, another
    // climbed and dug down, the bucket lost.
    let mut w = world(2, [10.5, 64.0, 5.5]);
    water(&mut w);
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    set(&mut w, (9, 64, 5), BlockKind::Water);
    let mut p = Plan::new(Aim::Race);
    p.spikes.landings.push(((8, 5), (9, 64, 5)));
    p.spikes.refused.push((8, 8, Axis::North, u64::MAX));
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: landing water taken back at (9, 64, 5)"),
        "{:?}",
        notes(&p)
    );
    assert!(p.spikes.landings.is_empty());
}

fn fireball(w: &mut World, pos: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id: 70,
        uuid: [4; 16],
        kind: EntityKind::DragonFireball.id(),
        x: pos[0],
        y: pos[1],
        z: pos[2],
        velocity: [0.0; 3],
    }));
}

#[test]
fn a_fireball_in_flight_keeps_the_bot_off_its_pillar_and_takes_it_down() {
    // E5's jar read: a fireball's cloud goes on the first living entity within 4 of its impact,
    // walls or not; up high there is no room to dodge.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    fireball(&mut w, [8.5, 80.0, 35.5]);
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p).iter().any(|n| n == "spikes: no climb"),
        "{:?}",
        notes(&p)
    );
    // Half way up when one comes: down at once.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    water(&mut w);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    fireball(&mut w, [8.5, 80.0, 35.5]);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    // Frontier race 45947478869 lane 1: the dig down, a block every 40 ticks, could not beat the
    // fireball. Off the side onto the clutch instead.
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: drop with the clutch from (8, 67, 5)"),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(
        p.spikes.doing,
        Some(spikes::Doing::Step { sneak: false, .. })
    ));
    // No water bucket in the hotbar: the dig, the only way left.
    w.inventory.slots.remove(&44);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    match p.spikes_task(&w, 100) {
        Some(Task::Dig(d)) => assert_eq!(d.target, (8, 66, 5)),
        t => panic!("{t:?} {:?}", notes(&p)),
    }
    // The strafe within 64 with no fireball yet: the same drop.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    water(&mut w);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    dragon(&mut w, [8.5, 80.0, 50.5], 1);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: drop with the clutch from (8, 67, 5)"),
        "{:?}",
        notes(&p)
    );
}

#[test]
fn the_landing_water_is_poured_from_a_bucket_other_than_the_clutchs() {
    // Race 45942992471 lane 1: the landing pour took the clutch's bucket from the hotbar.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    w.inventory.slots.insert(12, (ItemKind::WaterBucket, 1));
    assert_eq!(
        spikes::pour_hold(&w),
        plan::hold::Hold::Act(Act::Click {
            slot: 12,
            button: 0,
            mode: 2
        }),
        "the main slot's bucket into the hotbar, not over the clutch's"
    );
    w.inventory.slots.remove(&12);
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    assert_eq!(
        spikes::pour_hold(&w),
        plan::hold::Hold::Act(Act::Hold { slot: 1 }),
        "the other hotbar bucket"
    );
}

#[test]
fn a_placed_block_in_a_tunnel_is_no_pillar() {
    // Frontier race 45954274931 lane 0: E3's stair tunnel put cobbled deepslate under the feet;
    // read as a pillar, the bot dug its own floor down into a pit.
    let mut w = world(2, [20.5, 62.0, 20.5]);
    set(&mut w, (20, 62, 20), BlockKind::Air);
    set(&mut w, (20, 63, 20), BlockKind::Air);
    set(&mut w, (20, 61, 20), BlockKind::CobbledDeepslate);
    assert!(!spikes::on_pillar(&w, (20, 62, 20), &[]));
    // Standing free on the floor: a pillar even with no record.
    let mut w = world(2, [20.5, 66.0, 20.5]);
    pillar(&mut w, (20, 20), 64, 65, BlockKind::Cobblestone);
    assert!(spikes::on_pillar(&w, (20, 66, 20), &[]));
}

#[test]
fn a_clutch_landed_in_its_own_water_takes_it_back() {
    // Frontier race 45954274931 lane 1: "missed" from 90.9 at health 20, standing in the water it
    // had poured, which was never taken back: the bucket lost, every walk from the cell refused.
    let mut w = world(2, [20.5, 64.0, 20.5]);
    if let Some(d) = w.dimension.as_mut() {
        d.name = "minecraft:the_end".to_owned();
    }
    set(&mut w, (20, 64, 20), BlockKind::Water);
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    w.held = 1;
    let mut p = Plan::new(Aim::Race);
    p.spikes.clutch = Some(spikes::Clutch {
        since: 90,
        from_y: 90.9,
        poured: None,
        landed_at: None,
        sent_at: Some(95),
    });
    assert!(p.spikes_clutch(&w, 100).is_some());
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: clutch poured late at (20, 64, 20)"),
        "{:?}",
        notes(&p)
    );
    assert!(
        !notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: clutch missed"))
    );
    // The next tick: the take-back.
    p.spikes_clutch(&w, 101);
    assert!(
        p.take_acts().contains(&Act::UseItem),
        "the water taken back"
    );
}

#[test]
fn the_landing_pour_waits_for_the_body_centred_and_aims_past_the_first_block() {
    // Frontier race 45954274931 lanes 1 and 2: off the column's centre, the ray met the first
    // block's top and the water went into the feet's own cell; the pillar was dug away.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    w.inventory.slots.insert(38, (ItemKind::WaterBucket, 1));
    set(&mut w, (8, 64, 5), BlockKind::EndStone);
    at(&mut w, [8.2, 65.0, 5.5]);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: centre for the landing water at (8, 65, 5)"),
        "{:?}",
        notes(&p)
    );
    assert!(p.spikes.landings.is_empty());
    // Centred: the pour, aimed past the landing cell's centre, away from the pillar.
    at(&mut w, [8.5, 65.0, 5.5]);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    p.spikes_task(&w, 100);
    match &p.spikes.doing {
        Some(spikes::Doing::Bucket { look, .. }) => {
            assert!(
                (look[0] - (9.5 + spikes::LANDING_AIM)).abs() < 1e-9,
                "{look:?}"
            );
        }
        d => panic!("{d:?} {:?}", notes(&p)),
    }
}

#[test]
fn every_clutch_landing_takes_its_water_back_wherever_it_went() {
    // E5's read of frontier race 45954274931 lanes 2 and 3: bots stood in water spread over the
    // ground, and no walk or dodge could begin from it.
    let landed = |water: (i32, i32, i32)| {
        let mut w = world(2, [20.5, 64.0, 20.5]);
        if let Some(d) = w.dimension.as_mut() {
            d.name = "minecraft:the_end".to_owned();
        }
        set(&mut w, water, BlockKind::Water);
        w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
        w.held = 1;
        w
    };
    let poured = || spikes::Clutch {
        since: 90,
        from_y: 80.0,
        poured: Some((20, 64, 20)),
        landed_at: None,
        sent_at: Some(95),
    };
    // In the cell poured over: taken back.
    let w = landed((20, 64, 20));
    let mut p = Plan::new(Aim::Race);
    p.spikes.clutch = Some(poured());
    p.spikes_clutch(&w, 100);
    assert!(p.take_acts().contains(&Act::UseItem), "{:?}", notes(&p));
    // Knocked a cell aside: found beside the feet and taken back.
    let w = landed((21, 64, 20));
    let mut p = Plan::new(Aim::Race);
    p.spikes.clutch = Some(poured());
    p.spikes_clutch(&w, 100);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: clutch water found at (21, 64, 20)"),
        "{:?}",
        notes(&p)
    );
    assert!(p.take_acts().contains(&Act::UseItem));
    assert_eq!(
        p.spikes.clutch.as_ref().and_then(|c| c.poured),
        Some((21, 64, 20))
    );
    assert_eq!(spikes::clutch_water(&w, (20, 64, 20)), Some((21, 64, 20)));
}

#[test]
fn no_climb_begins_without_the_blocks_for_the_whole_pillar() {
    // Frontier race 45959448248 lane 3: 0 end stone, the one block each dig down gave back put up
    // 104 times to the cap, and E3's stock never resumed.
    let mut w = world(2, [8.5, 64.0, 5.5]);
    water(&mut w);
    w.inventory.slots.insert(36, (ItemKind::EndStone, 3));
    let mut p = Plan::new(Aim::Race);
    assert_eq!(p.spikes_task(&w, 100), None, "handed back for the stock");
    assert!(
        notes(&p)
            .iter()
            .any(|n| n == "spikes: no climb: too few blocks"),
        "{:?}",
        notes(&p)
    );
    assert!(p.spikes.pillars.is_empty(), "no pillar begun");
}
