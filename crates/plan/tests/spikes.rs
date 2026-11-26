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
    dragon(&mut w, [8.5, 64.0, 55.5], 0);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(notes(&p).iter().any(|n| n == "spikes: no climb"));
    // Perched 50 off: the climb.
    let mut w = world(2, [8.5, 64.0, 5.5]);
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
    dragon(&mut w, [8.5, 64.0, 105.5], 0);
    let mut p = Plan::new(Aim::Race);
    p.spikes_task(&w, 100);
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: pillar up from"))
    );
    // Charging 30 off with the bot half way up a pillar: no wait there, on up to the box.
    let mut w = world(2, [8.5, 67.0, 5.5]);
    pillar(&mut w, (8, 5), 64, 66, BlockKind::EndStone);
    dragon(&mut w, [8.5, 67.0, 35.5], 8);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(
        notes(&p).iter().any(|n| n == "spikes: climb on through"),
        "{:?}",
        notes(&p)
    );
    assert!(matches!(p.spikes.doing, Some(spikes::Doing::Climb(_))));
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
    // The crystal gone, the dragon strafing 30 off: held in the box, not down.
    gone(&mut w);
    let mut p = Plan::new(Aim::Race);
    p.spikes.pillars.push(PillarRec {
        x: 8,
        z: 5,
        base: 64,
    });
    assert_eq!(p.spikes_task(&w, 100), Some(Task::Idle));
    assert!(notes(&p).iter().any(|n| n == "spikes: hold in the box"));
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
    let w = world(2, [8.5, 64.0, 5.5]);
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
    let c = p.spikes_step(&w, 100);
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
        vec![Act::Hold { slot: 1 }],
        "the bucket to hand"
    );
    w.held = 1;
    fall(&mut w, 70.0);
    p.spikes_step(&w, 101);
    assert!(p.take_acts().is_empty(), "too high to pour yet");
    fall(&mut w, 66.5);
    p.spikes_step(&w, 102);
    assert_eq!(p.take_acts(), vec![Act::UseItem], "poured near the ground");
    w.inventory.slots.insert(37, (ItemKind::Bucket, 1));
    fall(&mut w, 65.5);
    p.spikes_step(&w, 103);
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
    p.spikes_step(&w, 104);
    assert_eq!(
        p.take_acts(),
        vec![Act::UseItem],
        "landed: the water taken back"
    );
    w.inventory.slots.insert(37, (ItemKind::WaterBucket, 1));
    assert!(p.spikes_step(&w, 105).is_none());
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("spikes: clutch over")),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.spikes.clutches, 1);
}
