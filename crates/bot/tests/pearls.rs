//! Issue 18, the burrow in the closed loop (docs/problems/pearls-model.md): the bot's own world,
//! fed by the model in lockstep, in a warped forest cavern with a level burrow one wide and two
//! high running east (B the back cell, M the mouth, O outside). The scripted bot of the parity
//! rows e04, e05 and e08 (`burrow:<mode>`) provokes from M, steps back to B and holds or
//! strikes. Development evidence only (measurement-rules): the race is the metric.

use sim::nether::{Biome, Stage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};
const FLOOR: i32 = 40;
const FEET: i32 = FLOOR + 1;
/// B, M and O on x (z 0, the feet's level).
const B: i32 = 0;
const M: i32 = 1;
const O: i32 = 2;

/// A warped forest cavern with the burrow, the bot standing in M, an enderman 10 east of M.
fn forest_burrow(seed: u64) -> (Sim, i32) {
    let mut stage = Stage::new(4, FLOOR, FLOOR + 20);
    let stone = data::BlockKind::Stone;
    stage
        .biome((-64, 79), (-64, 79), Biome::WarpedForest)
        // A solid block 4 long, 5 wide, 3 high with the tunnel in it: O the only cell 2 from B
        // (parity 45535279839: walls one thick tied O with the cells beside and behind B).
        .fill((B - 2, M), (FEET, FEET + 2), (-2, 2), stone)
        .fill((B, M), (FEET, FEET + 1), (0, 0), data::BlockKind::Air)
        .spawn((M, FEET, 0));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(seed);
    let id = sim.summon_enderman([f64::from(M + 10) + 0.5, f64::from(FEET), 0.5]);
    (sim, id)
}

fn run(script: bot::Script, sim: Sim, ticks: u64) -> bot::lockstep::Run {
    bot::lockstep::run_acting(sim, "rs-burrow", 9, ticks, 20, |w, t| {
        (script.steer(t, w), script.acts(t, w))
    })
}

fn feet_cell(run: &bot::lockstep::Run) -> [i32; 3] {
    run.world
        .body
        .unwrap()
        .pos
        .map(|v| (v + 1e-7).floor() as i32)
}

#[test]
fn provoked_from_m_the_bot_steps_to_b_and_the_enderman_holds_at_o_with_no_hit() {
    for seed in 1..=3 {
        let (sim, id) = forest_burrow(seed);
        let run = run(bot::Script::Burrow(bot::BurrowMode::Hold), sim, 480);
        let e = run.sim.ender(id).unwrap();
        assert!(e.angry, "seed {seed}: provoked");
        assert_eq!(feet_cell(&run), [B, FEET, 0], "seed {seed}: in B");
        let at = run.sim.survival.life.mobs[&id].pos;
        assert_eq!(
            at.map(|v| (v + 1e-7).floor() as i32),
            [O, FEET, 0],
            "seed {seed}: at O"
        );
        assert_eq!(run.min_health_in_script, Some(20.0), "seed {seed}: hit");
        assert!(run.sim.survival.life.hurt_by_mob.is_empty());
    }
}

#[test]
fn struck_from_b_on_a_full_charge_the_enderman_dies_at_the_mouth() {
    let (mut sim, id) = forest_burrow(4);
    sim.survival.give((data::ItemKind::IronSword, 1));
    let run = run(bot::Script::Burrow(bot::BurrowMode::Strike), sim, 600);
    // A dead mob is removed a moment after it dies.
    let m = run.sim.survival.life.mobs.get(&id);
    assert!(
        m.is_none_or(|m| m.dead_at.is_some()),
        "alive at {:?} health",
        m.map(|m| m.health)
    );
    assert_eq!(run.world.kills, 1);
    assert!(run.sim.ender(id).is_none_or(|e| e.teleports == 0));
    assert_eq!(feet_cell(&run), [B, FEET, 0]);
    assert_eq!(run.min_health_in_script, Some(20.0));
}

// ---- The hunt in the closed loop (issue 18f, P3's `pearls_task`): the planner, not a script. ----
// Each test prints its ledger on failure: a failure here is a wall for the hunt
// (docs/problems/pearls-hunt.md), not for the model.

/// Iron armour's points (the frontier's helmet, chestplate and leggings).
const IRON: f32 = 13.0;

fn record_line(r: plan::TaskRecord) -> String {
    match r {
        plan::TaskRecord::Started { at, task, what, .. } => {
            format!("{at:>6} start {task}: {}", what.unwrap_or_default())
        }
        plan::TaskRecord::Ended {
            at, task, outcome, ..
        } => format!("{at:>6} end   {task}: {}", outcome.unwrap_or_default()),
        plan::TaskRecord::Note { at, what, why } => format!("{at:>6} note  {what} ({why})"),
    }
}

struct Hunt {
    plan: plan::Plan,
    run: bot::lockstep::Run,
    ledger: Vec<String>,
}

impl Hunt {
    fn has(&self, what: &str) -> usize {
        self.ledger.iter().filter(|l| l.contains(what)).count()
    }

    fn story(&self) -> String {
        let n = self.ledger.len();
        // The last 200 lines: enough to read a wall, short enough to print.
        self.ledger[n.saturating_sub(200)..].join("\n")
    }

    fn pearls(&self) -> i32 {
        self.run
            .world
            .inventory
            .slots
            .values()
            .filter(|(k, _)| *k == data::ItemKind::EnderPearl)
            .map(|(_, n)| *n)
            .sum()
    }

    fn health(&self) -> f32 {
        self.run.min_health_in_script.unwrap_or(0.0)
    }
}

/// A warped forest cavern 7 chunks round the origin (4 put the burrow searches against the
/// cavern's edge: "no burrow site" at x 69 of 79) and the forest kit of the frontier's other
/// snapshot (race-45481633896 lane 2 moved to the forest: no coal, an iron sword, a shield, a
/// porkchop, 263 filler, two stone pickaxes, 7 rods, iron armour), the planner hunting from the
/// start (the base over: the model forest has no stems). `natural` spawns the forest's endermen.
fn hunt(
    ticks: u64,
    seed: u64,
    natural: bool,
    lay: impl Fn(&mut Stage),
    summon: impl Fn(&mut Sim),
) -> Hunt {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = plan::Plan::with_table(plan::Aim::Race, table);
    plan.ender.left = true;
    plan.ender.phase = plan::endermen::EnderPhase::Hunt;
    plan.base.over = true;
    let mut stage = Stage::new(7, FLOOR, FLOOR + 20);
    stage
        .biome((-112, 127), (-112, 127), Biome::WarpedForest)
        .spawn((0, FEET, 0));
    lay(&mut stage);
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(seed);
    if natural {
        sim = sim.with_natural_spawns();
    }
    sim.survival.life.armor = IRON;
    for stack in [
        (data::ItemKind::IronSword, 1),
        (data::ItemKind::Shield, 1),
        (data::ItemKind::StonePickaxe, 1),
        (data::ItemKind::StonePickaxe, 1),
        (data::ItemKind::BlazeRod, 7),
        (data::ItemKind::Porkchop, 1),
        (data::ItemKind::Cobblestone, 64),
        (data::ItemKind::Cobblestone, 64),
        (data::ItemKind::Netherrack, 64),
        (data::ItemKind::Netherrack, 64),
        (data::ItemKind::Netherrack, 7),
    ] {
        sim.survival.give(stack);
    }
    summon(&mut sim);
    let mut ledger = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-hunt", seed, ticks, 20, |w, _| {
        let seen = plan.event_count;
        let c = plan.step(w);
        ledger.extend(
            plan.events_since(seen)
                .iter()
                .map(|e| format!("{:>6} note  {} ({})", e.tick, e.what, e.why)),
        );
        ledger.extend(plan.take_records().into_iter().map(record_line));
        (c, plan.take_acts())
    });
    Hunt { plan, run, ledger }
}

/// An enderman placed already angry (its creepy flag sent with it at the join).
fn angry_at(sim: &mut Sim, at: [f64; 3]) -> i32 {
    let id = sim.summon_enderman(at);
    if let Some(m) = sim.survival.life.mobs.get_mut(&id) {
        m.loot = false;
        if let sim::life::Brain::Enderman(e) = &mut m.brain {
            e.angry = true;
        }
    }
    id
}

fn in_b(h: &Hunt) -> bool {
    let feet = h
        .run
        .world
        .body
        .unwrap()
        .pos
        .map(|v| (v + 1e-7).floor() as i32);
    h.plan.pearls.burrow.is_some_and(|b| b.b == feet)
}

#[test]
fn a_frozen_angry_enderman_holds_the_bot_in_b() {
    // Angry, 80 east: chunk 5, beyond the simulation distance 3, so frozen where it stands. A
    // calm one penned 18 east in line is the provoke row's candidate, never to be provoked while
    // the frozen angry one is present (the issue's rule: an angry frozen enderman is present).
    let pen = |st: &mut Stage| {
        st.fill(
            (17, 19),
            (FEET, FEET + 1),
            (-1, 1),
            data::BlockKind::Netherrack,
        )
        .fill((18, 18), (FEET, FEET + 2), (0, 0), data::BlockKind::Air);
    };
    let h = hunt(2400, 11, false, pen, |sim| {
        angry_at(sim, [80.5, f64::from(FEET), 0.5]);
        let calm = sim.summon_enderman([18.5, f64::from(FEET), 0.5]);
        if let Some(m) = sim.survival.life.mobs.get_mut(&calm) {
            m.loot = false;
        }
    });
    let story = h.story();
    assert!(h.plan.pearls.built, "no burrow built:\n{story}");
    assert_eq!(h.has("pearls row provoke"), 0, "provoked:\n{story}");
    assert!(h.has("pearls row hold") > 0, "never held:\n{story}");
    assert!(in_b(&h), "not in B at the end:\n{story}");
    assert_eq!(h.health(), 20.0, "hurt:\n{story}");
}

#[test]
fn an_angry_enderman_at_the_bot_out_of_b_is_a_breach_sealed() {
    // Angry and ticking 20 east at the start: it walks at the bot while the burrow is being
    // found and built, so the bot is out of B with an angry enderman near: the breach row.
    let h = hunt(
        2400,
        12,
        false,
        |_| {},
        |sim| {
            angry_at(sim, [20.5, f64::from(FEET), 0.5]);
        },
    );
    let story = h.story();
    assert!(h.has("breach") > 0, "no breach:\n{story}");
    assert!(h.has("breach: sealed") > 0, "never sealed:\n{story}");
    assert!(!h.run.died, "died:\n{story}");
}

#[test]
fn an_angry_enderman_out_of_reach_is_parked_and_the_bot_waits_in_b() {
    // Provoked from the mouth, then penned where it cannot come to O: a 1 by 1 pen 8 east and 3
    // north of the mouth's line, open on top so it is seen. Angry at the start, in sight.
    let pen = |st: &mut Stage| {
        st.fill(
            (7, 9),
            (FEET, FEET + 3),
            (-4, -2),
            data::BlockKind::Netherrack,
        )
        .fill((8, 8), (FEET, FEET + 3), (-3, -3), data::BlockKind::Air);
    };
    let h = hunt(3000, 13, false, pen, |sim| {
        angry_at(sim, [8.5, f64::from(FEET), -2.5]);
    });
    let story = h.story();
    assert!(h.has("parked") > 0, "never parked:\n{story}");
    assert_eq!(h.health(), 20.0, "hurt:\n{story}");
}

/// The issue's goal in the model: 12 pearls in a warped forest with its natural spawns, from the
/// forest kit, with no damage taken. Long (36,000 ticks a seed, the fight races' cap): run with
/// `--include-ignored` in release.
#[test]
#[ignore = "36,000 ticks a seed: run in release with --include-ignored"]
fn twelve_pearls_in_a_model_forest_over_seeds_with_no_damage() {
    let mut lines = Vec::new();
    for seed in [21, 22, 23] {
        let h = hunt(36_000, seed, true, |_| {}, |_| {});
        let kills = h.run.world.kills;
        println!(
            "SEED {seed} pearls {} kills {kills} min health {} spawns {} died {}",
            h.pearls(),
            h.health(),
            h.run.sim.survival.life.ender_spawns,
            h.run.died
        );
        if h.pearls() < 12 || h.health() < 20.0 {
            lines.push(format!(
                "seed {seed}: {} pearls, {kills} kills, min health {}\n{}",
                h.pearls(),
                h.health(),
                h.story()
            ));
        }
    }
    assert!(lines.is_empty(), "{}", lines.join("\n\n"));
}

/// The model forest's enderman supply against the real one (P1's race 45534565509: two idle bots
/// in the warped forest, 12,000 ticks, a senses row every 200 ticks; docs/problems/
/// pearls-observe.md): an enderman within 128 in 60 of 61 rows, within 32 in 60 and 22 of 61,
/// within 16 in 27 of 61 (lane 0), 18 distinct within 128 a lane over 10 minutes (13 to 15 in
/// the first). The model's idle bot in its 7-chunk forest, the same rows; printed, not asserted.
#[test]
#[ignore = "a measurement: 12,000 ticks a seed"]
fn the_model_forest_supply_against_the_real_counts() {
    for seed in [21, 22, 23, 24] {
        let mut stage = Stage::new(7, FLOOR, FLOOR + 20);
        stage
            .biome((-112, 127), (-112, 127), Biome::WarpedForest)
            .spawn((0, FEET, 0));
        let sim = Sim::new(CONFIG, stage.build().unwrap())
            .seeded(seed)
            .with_natural_spawns();
        let mut rows = 0u32;
        let mut within = [0u32; 3];
        let mut first_minute = 0usize;
        let mut seen: std::collections::BTreeSet<i32> = std::collections::BTreeSet::new();
        let run = bot::lockstep::run_acting(sim, "rs-supply", seed, 12_000, 20, |w, t| {
            if t % 200 == 0
                && let Some(b) = w.body
            {
                rows += 1;
                let d: Vec<(i32, f64)> = w
                    .entities
                    .iter()
                    .filter(|(_, e)| !e.dead && e.kind == data::EntityKind::Enderman)
                    .map(|(id, e)| {
                        (
                            *id,
                            (0..3)
                                .map(|i| (e.pos[i] - b.pos[i]).powi(2))
                                .sum::<f64>()
                                .sqrt(),
                        )
                    })
                    .collect();
                for (k, r) in [16.0, 32.0, 128.0].iter().enumerate() {
                    if d.iter().any(|(_, x)| x <= r) {
                        within[k] += 1;
                    }
                }
                seen.extend(d.iter().filter(|(_, x)| *x <= 128.0).map(|(id, _)| *id));
                if t <= 1200 {
                    first_minute = seen.len();
                }
            }
            // Eyes down, as an idle bot: no stare angers one.
            (
                physics::Controls {
                    pitch: Some(90.0),
                    ..physics::Controls::default()
                },
                Vec::new(),
            )
        });
        println!(
            "MEASURES supply seed {seed}: rows {rows}, within 16 {}, within 32 {}, within 128 {}, distinct within 128 {} ({} in the first minute), spawns {}, despawns {}",
            within[0],
            within[1],
            within[2],
            seen.len(),
            first_minute,
            run.sim.survival.life.ender_spawns,
            run.sim.survival.life.ender_despawns
        );
    }
}
