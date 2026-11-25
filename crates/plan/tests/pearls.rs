//! Issue 18e and 18f (docs/problems/pearls-hunt.md): the burrow's site and shell, its geometry
//! (the roof cuts the stare from B, the mouth sees out from M), and the hunt's rows on hand-built
//! Nether worlds: the strike from B (torso, attack then swing), the hold, the provoke from M with
//! its one exempt stare, the breach's seal, the collect, and none of it outside the hunt.

use std::sync::Arc;

use data::{BlockKind, EntityKind, ItemKind};
use plan::burrow::{self, Burrow};
use plan::pearls::{self, PearlRules};
use plan::{Aim, Plan, make};
use protocol::S2c;
use protocol::chunk::{Column, Paletted, Section};
use protocol::metadata::MetaValue;
use world::{Act, BlockPos, Event, World};

/// B, the bot's cell: the floor (warped nylium) tops out at y 63.
const B: [i32; 3] = [8, 64, 8];
/// The burrow's mouth faces east.
const DIR: [i32; 2] = [1, 0];

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

/// A Nether column: netherrack to y 47, warped nylium 48 to 63, air above.
fn column() -> Column {
    let mut sections = vec![section(BlockKind::Netherrack); 3];
    sections.push(section(BlockKind::WarpedNylium));
    sections.extend(std::iter::repeat_n(section(BlockKind::Air), 12));
    Column { min_y: 0, sections }
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

fn floor_world(feet: [i32; 3]) -> World {
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: 3,
        dimension: "minecraft:the_nether".to_owned(),
        game_type: 0,
    }));
    let col = Arc::new(column());
    for x in -3..=3 {
        for z in -3..=3 {
            w.step(&Event::Packet(S2c::LevelChunkWithLight {
                x,
                z,
                column: Arc::clone(&col),
            }));
        }
    }
    let pos = Burrow::centre(feet);
    w.body = Some(physics::Body {
        pos,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(pos);
    w.player.health = Some(12.5);
    w.player.food = Some(17);
    w
}

fn set(w: &mut World, c: [i32; 3], k: BlockKind) {
    w.step(&Event::Packet(S2c::BlockUpdate {
        pos: (c[0], c[1], c[2]),
        state: state(k),
    }));
}

/// The world with the burrow at B, toward east, built of cobblestone.
fn built() -> (World, Burrow) {
    let mut w = floor_world(B);
    let b = Burrow::at(B, DIR);
    for c in b.shell() {
        set(&mut w, c, BlockKind::Cobblestone);
    }
    (w, b)
}

fn add(w: &mut World, id: i32, kind: EntityKind, at: [f64; 3]) {
    w.step(&Event::Packet(S2c::AddEntity {
        id,
        uuid: [0; 16],
        kind: kind.id(),
        x: at[0],
        y: at[1],
        z: at[2],
    }));
}

fn meta(w: &mut World, id: i32, entries: Vec<(u8, MetaValue)>) {
    w.step(&Event::Packet(S2c::SetEntityData { id, entries }));
}

/// An enderman at `at`, its health 40, angry or calm.
fn enderman(w: &mut World, id: i32, at: [f64; 3], angry: bool) {
    add(w, id, EntityKind::Enderman, at);
    meta(
        w,
        id,
        vec![(9, MetaValue::Float(40.0)), (17, MetaValue::Bool(angry))],
    );
}

/// A hunting planner with its burrow built at B: an iron sword in hotbar slot 0, held, and a
/// stack of cobblestone in slot 1.
fn hunter(w: &mut World, b: Burrow) -> Plan {
    w.inventory.slots.insert(36, (ItemKind::IronSword, 1));
    w.inventory.slots.insert(37, (ItemKind::Cobblestone, 64));
    w.held = 0;
    let mut p = Plan::with_table(Aim::Race, table());
    p.ender.left = true;
    p.base.over = true;
    p.pearls.burrow = Some(b);
    p.pearls.built = true;
    p.pearls.since = Some(0);
    p
}

fn notes(p: &Plan) -> Vec<String> {
    p.events.iter().map(|e| e.what.clone()).collect()
}

#[test]
fn a_flat_floor_gives_a_site_at_the_feet() {
    let w = floor_world(B);
    let s = burrow::site(&w, &w.memory, B).unwrap_or_else(|| panic!("a site"));
    assert_eq!(s.b, B, "nearest the feet: the feet");
    assert_eq!(s.m, [B[0] + s.dir[0], B[1], B[2] + s.dir[1]]);
    // Built of nothing yet: 38 shell blocks wanted (the tunnel in a solid 4 by 5), the floors
    // there.
    assert_eq!(burrow::missing(&w, &s).len(), 38);
}

#[test]
fn a_cap_over_the_mouth_moves_the_site() {
    let mut w = floor_world(B);
    // A block three high over every O round the feet: no enderman stands there.
    for d in [[1, 0], [-1, 0], [0, 1], [0, -1]] {
        set(
            &mut w,
            [B[0] + 2 * d[0], B[1] + 2, B[2] + 2 * d[1]],
            BlockKind::Netherrack,
        );
    }
    let (s, why) = burrow::site_avoiding(&w, &w.memory, B, &[], 0);
    let s = s.unwrap_or_else(|| panic!("a site elsewhere"));
    assert_ne!(s.b, B);
    assert!(
        why.iter().any(|(k, _)| *k == "under 3 clear over O"),
        "{why:?}"
    );
    // A refused site is not taken again.
    let (t, _) = burrow::site_avoiding(&w, &w.memory, B, &[s.b], 1);
    assert_ne!(t.map(|t| t.b), Some(s.b));
}

#[test]
fn lava_or_a_drop_near_the_mouth_refuses_it() {
    let mut w = floor_world(B);
    set(&mut w, [B[0] + 4, 63, B[2]], BlockKind::Lava);
    let b = Burrow::at(B, DIR);
    let mut cells = plan::path::Cells::new(&w);
    assert_eq!(
        burrow::refuse(&w, &w.memory, &mut cells, &b),
        Some("lava near O")
    );
    let mut w = floor_world(B);
    for y in 58..=63 {
        set(&mut w, [B[0] + 2, y, B[2] + 3], BlockKind::Air);
    }
    let mut cells = plan::path::Cells::new(&w);
    assert_eq!(
        burrow::refuse(&w, &w.memory, &mut cells, &b),
        Some("a drop near O")
    );
}

#[test]
fn the_roof_cuts_the_stare_from_b_and_the_mouth_sees_out_from_m() {
    let (w, b) = built();
    assert!(burrow::missing(&w, &b).is_empty());
    // An enderman at O: from B the line to its eye meets the roof; from M it does not.
    let o = Burrow::centre(b.o);
    let its_eye = [o[0], o[1] + plan::look::ENDERMAN_EYE, o[2]];
    assert!(!plan::look::clear(&w, pearls::eye_in(b.b), its_eye));
    // One 20 off on the floor is in sight from M.
    let far = [o[0] + 18.0, o[1] + plan::look::ENDERMAN_EYE, o[2]];
    assert!(plan::look::clear(&w, pearls::eye_in(b.m), far));
    // Its box at O is in the bot's reach from B (eye to box 1.7 or less), not with the blaze's.
    let eye = pearls::eye_in(b.b);
    let reach = plan::defend::box_distance_of(eye, o, EntityKind::Enderman);
    assert!(reach <= plan::defend::STRIKE_REACH, "{reach}");
}

#[test]
fn an_angry_enderman_at_the_mouth_is_struck_at_the_torso_attack_then_swing() {
    let (mut w, b) = built();
    let o = Burrow::centre(b.o);
    enderman(&mut w, 50, [o[0] - 0.2, o[1], o[2]], true);
    let mut p = hunter(&mut w, b);
    w.ticks = 100;
    let c = p.step(&w);
    let acts = p.take_acts();
    assert_eq!(
        acts,
        vec![Act::Attack { entity: 50 }, Act::Swing],
        "notes {:?}",
        notes(&p)
    );
    assert!(!c.forward, "the bot stays in B");
    // The look was the torso's, not a stare.
    #[allow(clippy::cast_possible_truncation)]
    let stare = plan::look::stares(
        &w,
        pearls::eye_in(b.b),
        c.yaw as f32,
        c.pitch.unwrap_or(0.0),
    );
    assert_eq!(stare, None);
    // The next tick, the charge is not full: no swing.
    w.ticks = 101;
    p.step(&w);
    assert!(p.take_acts().is_empty());
    assert!(notes(&p).iter().any(|n| n.starts_with("fight enderman 50")));
}

#[test]
fn an_angry_enderman_out_of_reach_is_held_for_in_b() {
    let (mut w, b) = built();
    let o = Burrow::centre(b.o);
    enderman(&mut w, 51, [o[0] + 9.0, o[1], o[2]], true);
    let mut p = hunter(&mut w, b);
    w.ticks = 100;
    let c = p.step(&w);
    assert!(p.take_acts().is_empty());
    assert_eq!(p.pearls.row, "hold");
    // Pressed back against the back wall (facing west, sneaking), never toward the mouth
    // (P4's real row e04: a bot left at B's middle drifted 0.14 toward M).
    assert!(c.forward && c.sneak, "{c:?}");
    assert!((c.yaw - 90.0).abs() < 1.0, "west: {}", c.yaw);
    // Once 0.1 or more behind B's middle it stands, looking out.
    let back = [
        f64::from(B[0]) + 0.5 - 0.18,
        f64::from(B[1]),
        f64::from(B[2]) + 0.5,
    ];
    w.body = Some(physics::Body {
        pos: back,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(back);
    w.ticks = 101;
    let c = p.step(&w);
    assert!(!c.forward, "{c:?}");
}

#[test]
fn a_calm_ticking_enderman_is_provoked_from_m_and_the_bot_steps_back() {
    let (mut w, b) = built();
    // The bot stands in M.
    let m = Burrow::centre(b.m);
    w.body = Some(physics::Body {
        pos: m,
        on_ground: true,
        ..physics::Body::default()
    });
    w.player.pos = Some(m);
    enderman(&mut w, 52, [m[0] + 20.0, m[1], m[2]], false);
    let mut p = hunter(&mut w, b);
    w.ticks = 100;
    w.session.server_ticks = 100;
    let c = p.step(&w);
    assert!(
        notes(&p).iter().any(|n| n == "provoke enderman 52"),
        "{:?}",
        notes(&p)
    );
    assert_eq!(p.pearls.provoking, Some(52));
    // The look goes out staring at it: the one exemption.
    #[allow(clippy::cast_possible_truncation)]
    let stare = plan::look::stares(
        &w,
        pearls::eye_in(b.m),
        c.yaw as f32,
        c.pitch.unwrap_or(0.0),
    );
    assert_eq!(stare, Some(52));
    // Its creepy flag: provoked, and back to B.
    meta(&mut w, 52, vec![(17, MetaValue::Bool(true))]);
    w.ticks = 110;
    w.session.server_ticks = 110;
    let c = p.step(&w);
    assert!(notes(&p).iter().any(|n| n == "provoked enderman 52"));
    assert!(c.forward, "stepping back to B");
    assert_eq!(p.pearls.angers, 1);
}

#[test]
fn a_stare_at_another_enderman_while_provoking_is_kept_off() {
    let (mut w, b) = built();
    let m = Burrow::centre(b.m);
    enderman(&mut w, 60, [m[0] + 20.0, m[1], m[2]], false);
    enderman(&mut w, 61, [m[0] + 30.0, m[1], m[2]], false);
    let eye = pearls::eye_in(b.m);
    let (yaw, pitch) = plan::walk::look_at(eye, [m[0] + 20.0, m[1] + 2.55, m[2]]);
    #[allow(clippy::cast_possible_truncation)]
    let yaw = yaw as f32;
    // At 60's eye the look also stares at 61 behind it on the line? No: 61's eye is at the same
    // height 10 farther, so the look is within its cone too; the exemption is 60's alone.
    assert_eq!(
        plan::look::stares_except(&w, eye, yaw, pitch, Some(60)),
        Some(61)
    );
    let (_, kept) = plan::look::keep_off_except(&w, eye, yaw, pitch, Some(60));
    assert_eq!(
        plan::look::stares_except(&w, eye, yaw, kept, Some(60)),
        None
    );
    // Exempting 61 instead leaves 60 filtered.
    assert_eq!(
        plan::look::stares_except(&w, eye, yaw, pitch, Some(61)),
        Some(60)
    );
}

#[test]
fn a_hit_by_an_enderman_seals_the_mouth_from_b() {
    let (mut w, b) = built();
    let o = Burrow::centre(b.o);
    enderman(&mut w, 53, [o[0], o[1], o[2]], true);
    let mut p = hunter(&mut w, b);
    w.ticks = 100;
    w.session.server_ticks = 100;
    p.step(&w);
    p.take_acts();
    w.hits.push(world::Hit {
        at_tick: 105,
        damage_type: "mob_attack".to_owned(),
        cause: Some(EntityKind::Enderman),
        direct: Some(EntityKind::Enderman),
    });
    w.ticks = 120;
    let c = p.step(&w);
    assert!(notes(&p).iter().any(|n| n == "breach: hurt by an enderman"));
    // Cobblestone to the hand (slot 1), then the seal block into M.
    let acts = p.take_acts();
    assert_eq!(acts, vec![Act::Hold { slot: 1 }], "{:?}", notes(&p));
    assert_eq!(c.pitch, Some(plan::endermen::EYES_DOWN));
    w.held = 1;
    w.ticks = 121;
    p.step(&w);
    let acts = p.take_acts();
    assert!(
        acts.iter().any(|a| matches!(a, Act::UseOn { .. })),
        "{acts:?} {:?}",
        notes(&p)
    );
    assert!(
        notes(&p)
            .iter()
            .any(|n| n.starts_with("seal block at (9, 64, 8)"))
    );
}

#[test]
fn a_pearl_at_the_mouth_with_none_angry_is_collected_from_m() {
    let (mut w, b) = built();
    let o = Burrow::centre(b.o);
    add(&mut w, 70, EntityKind::Item, [o[0], o[1], o[2]]);
    meta(
        &mut w,
        70,
        vec![(
            8,
            MetaValue::Item(protocol::slot::Slot::item(ItemKind::EnderPearl.id(), 1)),
        )],
    );
    let mut p = hunter(&mut w, b);
    w.ticks = 100;
    let c = p.step(&w);
    assert!(
        notes(&p).iter().any(|n| n == "collect pearl 70"),
        "{:?}",
        notes(&p)
    );
    assert!(c.forward, "stepping out to M");
    assert!((c.yaw - -90.0).abs() < 1.0, "east: {}", c.yaw);
}

#[test]
fn outside_the_hunt_the_rows_do_nothing() {
    let (mut w, b) = built();
    let o = Burrow::centre(b.o);
    enderman(&mut w, 54, [o[0], o[1], o[2]], true);
    let mut p = hunter(&mut w, b);
    p.ender.left = false;
    w.ticks = 100;
    p.step(&w);
    assert!(
        !p.take_acts()
            .iter()
            .any(|a| matches!(a, Act::Attack { .. })),
        "never a swing at an enderman outside the hunt"
    );
    assert!(!p.pearls.started);
}

#[test]
fn a_hunter_with_no_burrow_finds_a_site_and_builds_from_b() {
    let mut w = floor_world(B);
    w.inventory.slots.insert(36, (ItemKind::Cobblestone, 64));
    w.held = 0;
    let mut p = Plan::with_table(Aim::Race, table());
    p.ender.left = true;
    p.base.over = true;
    w.ticks = 100;
    p.step(&w);
    let b = p
        .pearls
        .burrow
        .unwrap_or_else(|| panic!("a site: {:?}", notes(&p)));
    assert_eq!(b.b, B);
    // The bot is at B: the next ticks place the shell, one a 10 ticks.
    w.ticks = 101;
    p.step(&w);
    let acts = p.take_acts();
    assert!(
        acts.iter().any(|a| matches!(a, Act::UseOn { .. })),
        "{acts:?} {:?}",
        notes(&p)
    );
    let _ = BlockPos::new(0, 0, 0);
}

#[test]
fn the_rules_load_from_the_goal_table() {
    let t = table();
    assert_eq!(
        t.pearls,
        PearlRules::default(),
        "goals.toml [pearls] and the defaults agree"
    );
}
