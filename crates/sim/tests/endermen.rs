//! Issue 17: the model's endermen and Nether biomes (crates/sim/src/enderman.rs,
//! docs/problems/endermen-model.md): biome containers a client reads, an enderman frozen beyond
//! the simulation distance and ticking within it, the stare's anger after the goal's delay, the
//! teleports, and the warped forest's natural spawning, each rule read from the 26.1.2 jar.

use data::EntityKind;
use protocol::metadata::MetaValue;
use protocol::{C2s, S2c, decode};
use sim::nether::{Biome, Stage};
use sim::{Config, In, ModelWorld, Sim, Wire};

/// The race's distances (config/race.toml): simulation 3, view 8.
const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
/// The cavern's floor top and the feet's height over it.
const FLOOR: i32 = 40;
const FEET: f64 = 41.0;

fn packets(out: Vec<Wire>) -> Vec<S2c> {
    out.iter()
        .filter_map(|w| decode(w.state, w.id, &w.body).ok())
        .collect()
}

fn send(sim: &mut Sim, p: C2s) -> Vec<S2c> {
    packets(sim.step(&In::Packet(p)).unwrap())
}

fn ticks(sim: &mut Sim, n: u32) -> Vec<S2c> {
    (0..n)
        .flat_map(|_| packets(sim.step(&In::Tick).unwrap()))
        .collect()
}

fn joined_with(world: ModelWorld, config: Config) -> (Sim, Vec<S2c>) {
    join(Sim::new(config, world).seeded(5).with_natural_spawns())
}

/// The model joined and standing at the world's spawn; with it, what the join sent.
fn join(mut sim: Sim) -> (Sim, Vec<S2c>) {
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

/// A Nether cavern 9 by 9 chunks round the origin, the player at (0.5, 41, 0.5).
fn cavern(radius: i32) -> Stage {
    let mut stage = Stage::new(radius, FLOOR, FLOOR + 20);
    stage.spawn((0, FLOOR + 1, 0));
    stage
}

fn joined(stage: &Stage) -> Sim {
    joined_with(stage.build().unwrap(), CONFIG).0
}

/// Look along (yaw, pitch) from where the player stands.
fn look(sim: &mut Sim, yaw: f32, pitch: f32) {
    let [x, y, z] = sim.pos;
    send(
        sim,
        C2s::MovePlayerPosRot {
            x,
            y,
            z,
            yaw,
            pitch,
            flags: 1,
        },
    );
}

/// The yaw and pitch from the player's eye to an enderman's eye with its feet at `at`.
fn aim_at_eye(sim: &Sim, at: [f64; 3]) -> (f32, f32) {
    let eye = [sim.pos[0], sim.pos[1] + 1.62, sim.pos[2]];
    let d = [at[0] - eye[0], at[1] + 2.55 - eye[1], at[2] - eye[2]];
    let flat = d[0].hypot(d[2]);
    let yaw = (-d[0]).atan2(d[2]).to_degrees();
    let pitch = -(d[1].atan2(flat)).to_degrees();
    (yaw as f32, pitch as f32)
}

fn data_of(out: &[S2c], id: i32, index: u8) -> Option<MetaValue> {
    out.iter().rev().find_map(|p| match p {
        S2c::SetEntityData { id: i, entries } if *i == id => entries
            .iter()
            .find(|(k, _)| *k == index)
            .map(|(_, v)| v.clone()),
        _ => None,
    })
}

fn moved(out: &[S2c], id: i32) -> usize {
    out.iter()
        .filter(|p| matches!(p, S2c::EntityPositionSync { id: i, .. } if *i == id))
        .count()
}

#[test]
fn a_stage_lays_nether_biomes_that_a_client_reads_from_the_column() {
    let mut stage = cavern(2);
    stage
        .biome((16, 31), (-32, 31), Biome::WarpedForest)
        .biome((-32, -17), (-32, 31), Biome::CrimsonForest)
        .biome((0, 3), (16, 19), Biome::SoulSandValley);
    let world = stage.build().unwrap();
    assert_eq!(world.biome_at(20, 45, 3), Some("warped_forest"));
    assert_eq!(world.biome_at(-20, 45, 3), Some("crimson_forest"));
    assert_eq!(world.biome_at(1, 45, 17), Some("soul_sand_valley"));
    assert_eq!(
        world.biome_at(4, 45, 17),
        Some("nether_wastes"),
        "the next quart"
    );
    assert_eq!(world.biome_at(0, 45, 0), Some("nether_wastes"));
    // The bytes a client gets decode to the same containers (protocol::chunk's reader).
    let (_, join) = joined_with(world, CONFIG);
    let column = join
        .iter()
        .find_map(|p| match p {
            S2c::LevelChunkWithLight { x: 1, z: 0, column } => Some(column.clone()),
            _ => None,
        })
        .unwrap();
    let section = column.section_at(45).unwrap();
    let warped = sim::nether::biome_id("warped_forest").unwrap();
    assert_eq!(
        section.biomes.get(sim::nether::biome_index(20, 45, 3)),
        Some(warped)
    );
    assert_eq!(Biome::WarpedForest.id(), warped);
}

#[test]
fn an_enderman_beyond_the_simulation_distance_is_frozen_and_one_within_it_ticks() {
    let mut sim = joined(&cavern(5));
    // Chunk 4 east (x 64 to 79): beyond the 3 chunks; chunk 1 north-east within. Both two above
    // the floor: only a ticking body falls.
    let far = sim.summon_enderman([70.5, FEET + 2.0, 0.5]);
    let near = sim.summon_enderman([30.5, FEET + 2.0, 0.5]);
    let out = ticks(&mut sim, 40);
    let mob = |sim: &Sim, id| sim.survival.life.mobs.get(&id).map(|m| m.pos).unwrap();
    assert_eq!(mob(&sim, far)[1], FEET + 2.0, "frozen in the air");
    assert_eq!(moved(&out, far), 0, "no position update for a frozen one");
    assert_eq!(sim.ender(far).unwrap().ticked, 0);
    assert_eq!(
        mob(&sim, near)[1],
        FEET,
        "the ticking one fell to the floor"
    );
    assert!(moved(&out, near) > 0);
    assert!(!sim.ticking([70.5, FEET, 0.5]) && sim.ticking([30.5, FEET, 0.5]));
    // The edge: chunk 3 (x 48 to 63) ticks, chunk 4 does not.
    assert!(sim.ticking([63.9, FEET, 0.5]) && !sim.ticking([64.0, FEET, 0.5]));
}

#[test]
fn a_frozen_enderman_starts_ticking_when_the_player_comes_within_the_simulation_distance() {
    let mut sim = joined(&cavern(5));
    let far = sim.summon_enderman([70.5, FEET + 2.0, 0.5]);
    ticks(&mut sim, 20);
    assert_eq!(sim.ender(far).unwrap().ticked, 0);
    // Walk east in steps a server accepts (under 10 blocks a move) to x 20: chunk 1, so chunk 4
    // is 3 away.
    for x in 1..=20 {
        let [_, y, z] = sim.pos;
        send(
            &mut sim,
            C2s::MovePlayerPosRot {
                x: f64::from(x) + 0.5,
                y,
                z,
                yaw: 0.0,
                pitch: 90.0,
                flags: 1,
            },
        );
        ticks(&mut sim, 1);
    }
    assert_eq!(sim.corrections, 0);
    let out = ticks(&mut sim, 30);
    assert!(sim.ender(far).unwrap().ticked > 0, "ticks now");
    assert!(moved(&out, far) > 0, "and falls");
}

#[test]
fn a_stare_angers_an_enderman_after_the_goals_delay_and_a_look_aside_does_not() {
    let mut sim = joined(&cavern(3));
    let at = [12.5, FEET, 0.5];
    let id = sim.summon_enderman(at);
    let (yaw, pitch) = aim_at_eye(&sim, at);
    // Aside by 10 degrees: no stare.
    look(&mut sim, yaw + 10.0, pitch);
    let out = ticks(&mut sim, 40);
    assert!(!sim.ender(id).unwrap().angry);
    assert_eq!(data_of(&out, id, sim::enderman::META_CREEPY), None);
    assert!(!sim.stares_at(sim.survival.life.mobs[&id].pos));
    // Wait for it to stand still, then look it in the eye.
    let pos = sim.survival.life.mobs[&id].pos;
    let (yaw, pitch) = aim_at_eye(&sim, pos);
    look(&mut sim, yaw, pitch);
    let mut angry_at = None;
    let mut out = Vec::new();
    for t in 1..=20u32 {
        out.extend(ticks(&mut sim, 1));
        // The enderman may stroll: keep the eye on it, as a player tracking it would.
        let pos = sim.survival.life.mobs[&id].pos;
        let (yaw, pitch) = aim_at_eye(&sim, pos);
        look(&mut sim, yaw, pitch);
        if angry_at.is_none() && sim.ender(id).unwrap().angry {
            angry_at = Some(t);
        }
    }
    let t = angry_at.expect("angered");
    // The goal starts on an even tick of its age and counts 3 goal ticks: 4 to 6 server ticks.
    assert!((4..=6).contains(&t), "angry after {t} ticks");
    assert_eq!(
        data_of(&out, id, sim::enderman::META_STARED_AT),
        Some(MetaValue::Bool(true))
    );
    assert_eq!(
        data_of(&out, id, sim::enderman::META_CREEPY),
        Some(MetaValue::Bool(true))
    );
}

#[test]
fn a_carved_pumpkin_on_the_head_spares_a_stare() {
    let mut sim = joined(&cavern(3));
    let at = [8.5, FEET, 0.5];
    let id = sim.summon_enderman(at);
    sim.survival.slots[5] = Some((data::ItemKind::CarvedPumpkin, 1));
    let (yaw, pitch) = aim_at_eye(&sim, at);
    look(&mut sim, yaw, pitch);
    assert!(!sim.stares_at(at));
    ticks(&mut sim, 20);
    assert!(!sim.ender(id).unwrap().angry);
}

#[test]
fn an_angry_enderman_farther_than_sixteen_teleports_toward_the_player_after_thirty_ticks() {
    let mut sim = joined(&cavern(3));
    // 50 east (chunk 3, ticking): it runs at the player, and is still past 16 when the 30 ticks
    // are up.
    let id = sim.summon_enderman([50.5, FEET, 0.5]);
    // Angered by a hit's rule (the goal's target), then left unwatched: the player looks down.
    if let Some(m) = sim.survival.life.mobs.get_mut(&id)
        && let sim::life::Brain::Enderman(e) = &mut m.brain
    {
        e.angry = true;
    }
    look(&mut sim, 0.0, 90.0);
    let mut first = None;
    for t in 1..=80u32 {
        ticks(&mut sim, 1);
        if first.is_none() && sim.ender(id).unwrap().teleports > 0 {
            first = Some(t);
        }
    }
    let t = first.expect("teleported toward");
    // `teleportTime` counts to 15 goal ticks (every other tick): about 30 server ticks; a
    // landing that does not fit waits for the next goal tick.
    assert!(t >= 30, "teleported after {t} ticks");
    let pos = sim.survival.life.mobs[&id].pos;
    let d = (pos[0] - sim.pos[0]).hypot(pos[2] - sim.pos[2]);
    assert!(d < 40.0, "nearer: {d:.1} from the player");
}

#[test]
fn an_angry_enderman_stared_at_within_four_teleports_away_and_a_hit_never_moves_it() {
    let mut sim = joined(&cavern(3));
    let at = [3.0, FEET, 0.5];
    let id = sim.summon_enderman(at);
    let (yaw, pitch) = aim_at_eye(&sim, at);
    look(&mut sim, yaw, pitch);
    let mut t = 0;
    while sim.ender(id).unwrap().teleports == 0 && t < 60 {
        ticks(&mut sim, 1);
        t += 1;
    }
    assert!(sim.ender(id).unwrap().angry);
    assert!(sim.ender(id).unwrap().teleports > 0, "teleported away");
    // A hit by the player angers one at once and never teleports it.
    let other = sim.summon_enderman([2.5, FEET, 2.5]);
    look(&mut sim, 180.0, 90.0);
    sim.survival.life.attack_ticker = 100;
    send(&mut sim, C2s::Attack { entity: other });
    let e = sim.ender(other).unwrap();
    assert!(e.angry);
    ticks(&mut sim, 1);
    assert_eq!(sim.ender(other).unwrap().teleports, 0);
}

#[test]
fn a_warped_forest_spawns_endermen_beyond_twenty_four_within_ticking_chunks() {
    let mut stage = cavern(4);
    stage.biome((-64, 79), (-64, 79), Biome::WarpedForest);
    let mut sim = joined(&stage);
    // Each new enderman judged where it appeared (they stroll after).
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..200 {
        ticks(&mut sim, 1);
        for (id, m) in &sim.survival.life.mobs {
            if m.kind != EntityKind::Enderman || !seen.insert(*id) {
                continue;
            }
            let p = m.pos;
            let d = ((p[0] - 0.5).powi(2) + (p[2] - 0.5).powi(2)).sqrt();
            assert!(d > 20.0, "spawned {d:.1} from the player");
            assert!(sim.ticking(p), "spawned in a ticking chunk: {p:?}");
        }
    }
    let spawned = sim.survival.life.ender_spawns;
    assert!(spawned > 0, "the forest spawned endermen");
    assert_eq!(seen.len() as u64, spawned);
    // The spawn cost: no two closer than 1 / 0.12 (8.3) when each spawned; they may stroll since.
    println!("{spawned} spawned, {} alive", sim.survival.life.mobs.len());
}

#[test]
fn crimson_forest_peaceful_and_spawning_off_spawn_no_endermen() {
    let mut crimson = cavern(4);
    crimson.biome((-64, 79), (-64, 79), Biome::CrimsonForest);
    let mut sim = joined(&crimson);
    ticks(&mut sim, 200);
    assert_eq!(sim.survival.life.ender_spawns, 0, "crimson forest");
    let mut forest = cavern(4);
    forest.biome((-64, 79), (-64, 79), Biome::WarpedForest);
    let (mut sim, _) = join(Sim::new(CONFIG, forest.build().unwrap()).seeded(5));
    ticks(&mut sim, 200);
    assert_eq!(
        sim.survival.life.ender_spawns, 0,
        "natural spawns off by default"
    );
    let mut stage = cavern(4);
    stage.biome((-64, 79), (-64, 79), Biome::WarpedForest);
    let peaceful = Config {
        peaceful: true,
        ..CONFIG
    };
    let (mut sim, _) = joined_with(stage.build().unwrap(), peaceful);
    ticks(&mut sim, 200);
    assert_eq!(sim.survival.life.ender_spawns, 0, "peaceful");
}

#[test]
fn a_soul_sand_valley_spawns_endermen_one_pack_in_seventy_one() {
    // Issue 17a's first target (docs/problems/endermen.md): a soul sand valley, whose monster
    // packs are skeletons (20), ghasts (50) and endermen (1); the model spawns the endermen only.
    let mut stage = cavern(4);
    stage.biome((-64, 79), (-64, 79), Biome::SoulSandValley);
    let mut sim = joined(&stage);
    ticks(&mut sim, 4000);
    let spawned = sim.survival.life.ender_spawns;
    println!("soul sand valley: {spawned} endermen in 4000 ticks");
    assert!(spawned > 0, "a valley spawns endermen, rarely");
    // Not compared with a forest: the forest's spawn cost (charge 1, budget 0.12, summed over
    // every enderman there) saturates it at a dozen or so, where the valley's (0.7, 0.15) lets
    // its rare packs land closer together (CI 45476117006: forest 13, valley 20 in 4000 ticks).
}
