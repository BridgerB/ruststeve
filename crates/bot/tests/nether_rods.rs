//! Issue 16's whole stage, "7 rods at the spawner", closed loop on the real fortress
//! (docs/problems/nether-testbed.md): `fixtures/nether-fortress.rsw` (the columns round the
//! blaze spawner as the server sent them to lane 0 of race 45337739022), the spawner running by
//! `BaseSpawner`'s rules, blazes that shoot, rise to the bot's eye height and drop a rod to a
//! player's kill (0 or 1, the jar's loot table), the model's hunger, eating and healing.
//! Development evidence only: the model's blazes move and touch as their goals say and hits
//! knock both ways, but it finds no paths and spawns no other mobs unless a test summons them
//! (crates/sim/src/life.rs says what it holds).
//!
//! Each test prints its measures whether it passes or not. Run them with
//! `cargo test -p bot --test nether_rods -- --ignored --nocapture --test-threads=1`.

use std::path::Path;

use bot::captured::CapturedWorld;
use data::{BlockKind, EntityKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::life::Brain;
use sim::nether::{NETHER, nether_type};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};

/// Where the frontier snapshot's bot stands, and the spawner it stands by.
const START: [f64; 3] = [129.79, 62.17, 164.3];
const NEAR_SPAWNER: (i32, i32, i32) = (129, 64, 159);
/// Iron helmet, chestplate and leggings.
const IRON: f32 = 13.0;
/// Ten minutes of game time.
const TICKS: u64 = 12_000;

fn kind(sim: &Sim, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

/// The fortress with its spawner running; with it, where the spawner is.
fn fortress(seed: u64) -> (Sim, (i32, i32, i32)) {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/nether-fortress.rsw"
    );
    let mut world = CapturedWorld::read(Path::new(path))
        .unwrap()
        .model(START)
        .unwrap();
    world.dimension = NETHER.to_owned();
    world.dimension_type = nether_type();
    let mut sim = Sim::new(CONFIG, world).seeded(seed);
    let (sx, sy, sz) = NEAR_SPAWNER;
    let spawner = (-8..=8)
        .flat_map(|dx| {
            (-4..=4).flat_map(move |dy| (-8..=8).map(move |dz| (sx + dx, sy + dy, sz + dz)))
        })
        .filter(|p| kind(&sim, *p) == Some(BlockKind::Spawner))
        .min_by_key(|p| (p.0 - sx).abs() + (p.1 - sy).abs() + (p.2 - sz).abs())
        .expect("a spawner in the fixture");
    sim.add_spawner(spawner, EntityKind::Blaze, Brain::blaze());
    (sim, spawner)
}

struct Stage {
    run: bot::lockstep::Run,
    spawner: (i32, i32, i32),
    rods: i64,
    /// The first tick the bot held 7 rods.
    seven_at: Option<u64>,
    meals: u64,
    /// Health taken off the bot over the run, every drop summed.
    health_lost: f32,
    /// Ticks a live blaze's feet were within 1.5 of the bot's eye height, of all blaze-ticks.
    level_ticks: u64,
    blaze_ticks: u64,
    ledger: Vec<String>,
    ticks: u64,
    /// The planner's shield at the end: its counts.
    shield: plan::shield::Shield,
}

fn line(r: TaskRecord) -> String {
    match r {
        TaskRecord::Started {
            at,
            task,
            what,
            why,
            ..
        } => format!(
            "{at:>6} start {task}: {} ({})",
            what.unwrap_or_default(),
            why.unwrap_or_default()
        ),
        TaskRecord::Ended {
            at,
            task,
            ticks,
            outcome,
            why,
            ..
        } => format!(
            "{at:>6} end   {task} after {ticks}: {} ({})",
            outcome.unwrap_or_default(),
            why.unwrap_or_default()
        ),
        TaskRecord::Note { at, what, why } => format!("{at:>6} note  {what} ({why})"),
    }
}

/// The frontier bot (armour 13, an iron sword) with `food` and, when `shield`, a shield in the
/// off hand, the race's aim, for `TICKS`.
fn stage(food: (ItemKind, i32), shield: bool) -> Stage {
    stage_with(&[food], shield, SEED, |_| {})
}

/// The seed the single runs use, and the eight the loops use.
const SEED: u64 = 7;
const SEEDS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

/// The same, with what `more` adds to the model first.
fn stage_with(kit: &[(ItemKind, i32)], shield: bool, seed: u64, more: impl Fn(&mut Sim)) -> Stage {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Race, table);
    let (mut sim, spawner) = fortress(seed);
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    for stack in kit {
        sim.survival.give(*stack);
    }
    sim.survival.give((ItemKind::Cobblestone, 64));
    if shield {
        // The off hand is menu slot 45.
        sim.survival.slots[45] = Some((ItemKind::Shield, 1));
    }
    more(&mut sim);
    let mut ledger = Vec::new();
    let (mut meals, mut health_lost, mut last) = (0u64, 0.0f32, 20.0f32);
    let (mut level_ticks, mut blaze_ticks) = (0u64, 0u64);
    let mut seven_at = None;
    let run = bot::lockstep::run_acting(sim, "rs-rods", seed, TICKS, 20, |w, t| {
        let c = plan.step(w);
        for r in plan.take_records() {
            if matches!(&r, TaskRecord::Started { task, .. } if task == "eat") {
                meals += 1;
            }
            ledger.push(line(r));
        }
        if let Some(h) = w.player.health {
            health_lost += (last - h).max(0.0);
            last = h;
        }
        if let Some(b) = w.body {
            let eye = b.pos[1] + physics::EYE_HEIGHT;
            for e in w
                .entities
                .values()
                .filter(|e| e.kind == EntityKind::Blaze && !e.dead)
            {
                blaze_ticks += 1;
                level_ticks += u64::from((e.pos[1] - eye).abs() <= 1.5);
            }
        }
        if seven_at.is_none() && w.inventory.count(ItemKind::BlazeRod) >= 7 {
            seven_at = Some(t);
        }
        (c, plan.take_acts())
    });
    let shield = plan.shield.clone();
    let rods = run.world.inventory.count(ItemKind::BlazeRod);
    let ticks = run.world.session.server_ticks;
    Stage {
        run,
        spawner,
        rods,
        seven_at,
        meals,
        health_lost,
        level_ticks,
        blaze_ticks,
        ledger,
        ticks,
        shield,
    }
}

/// What hurt the bot, by the server's damage type: "mob_attack 12.0, fireball 8.4".
fn damage_by_type(life: &sim::life::Life) -> String {
    let parts: Vec<String> = life
        .hurt_by
        .iter()
        .map(|(k, v)| format!("{k} {v:.1}"))
        .collect();
    if parts.is_empty() {
        "none".to_owned()
    } else {
        parts.join(", ")
    }
}

impl Stage {
    /// The measures, and the ledger's ends.
    fn story(&self) -> String {
        let life = &self.run.sim.survival.life;
        let kills = life.kills;
        let (sx, sy, sz) = self.spawner;
        let fire = (-6..=6)
            .flat_map(|dx| (-3..=3).flat_map(move |dy| (-6..=6).map(move |dz| (dx, dy, dz))))
            .filter(|(dx, dy, dz)| {
                kind(&self.run.sim, (sx + dx, sy + dy, sz + dz)) == Some(BlockKind::Fire)
            })
            .count();
        let n = self.ledger.len();
        let (head, tail) = if n > 100 {
            (&self.ledger[..40], &self.ledger[n - 60..])
        } else {
            (&self.ledger[..], &self.ledger[..0])
        };
        format!(
            "MEASURES: rods {} (7 at tick {:?}), died {}, ticks {}, kills {} ({:.2} per 1,000 ticks), \
             blazes spawned {}, alive at the end {}, fireballs fired {} hit {} landed {} blocked {}, \
             health lost {:.1} ({:.1} a kill), lowest health {:?}, meals {}, fire cells within 6 of the \
             spawner {}, blaze-ticks within 1.5 of the eye's height {} of {}, damage by type {}, \
             blazes knocked {}, spawner at {:?}, the bot at {:?}\n\
             ledger ({n} lines):\n{}\n  ...\n{}",
            self.rods,
            self.seven_at,
            self.run.died,
            self.ticks,
            kills,
            kills as f64 * 1000.0 / self.ticks.max(1) as f64,
            life.spawners.first().map_or(0, |s| s.spawned),
            life.mobs.values().filter(|m| m.dead_at.is_none()).count(),
            life.fireballs_fired,
            life.fireballs_hit,
            life.fireballs_landed,
            life.fireballs_blocked,
            self.health_lost,
            self.health_lost / kills.max(1) as f32,
            self.run.min_health_in_script,
            self.meals,
            fire,
            self.level_ticks,
            self.blaze_ticks,
            damage_by_type(life),
            life.mobs_knocked,
            self.spawner,
            self.run
                .world
                .body
                .map(|b| b.pos.map(|v| (v * 10.0).round() / 10.0)),
            head.join("\n"),
            tail.join("\n"),
        )
    }

    fn assert_seven(&self) {
        let story = self.story();
        println!("{story}");
        assert!(!self.run.died, "died\n{story}");
        assert!(self.rods >= 7, "{} rods\n{story}", self.rods);
    }
}

/// The stage and the model under it: the spawner is found and runs, and blazes appear by the
/// bot within the spawner's first cycles.
#[test]
fn the_spawner_of_the_real_fortress_spawns_blazes_by_the_bot() {
    let (sim, spawner) = fortress(SEED);
    let run = bot::lockstep::run(sim, "rs-rods", 1, 900, 20, |_, _| {
        physics::Controls::default()
    });
    let spawned = run.sim.survival.life.spawners[0].spawned;
    println!(
        "spawner at {spawner:?}: {spawned} blazes in {} ticks",
        run.sim.ticks
    );
    assert!(spawned >= 1, "no blaze spawned at {spawner:?}");
}

/// Blaze-fight wall 20: by the spawner no standing work is safe (the snapshot-kit run made its
/// shield on the deck and died at 1,140): the fortress's own spawner, 5 blocks off, is named.
#[test]
fn no_work_is_safe_beside_the_spawner() {
    let (sim, spawner) = fortress(SEED);
    let run = bot::lockstep::run(sim, "rs-rods", 1, 40, 20, |_, _| {
        physics::Controls::default()
    });
    let why = plan::cover::unsafe_to_work(&run.world);
    assert!(
        why.as_deref().is_some_and(|w| w.contains("spawner")),
        "{why:?}, spawner at {spawner:?}"
    );
}

/// The frontier snapshot's own kit: armour, an iron sword, 8 raw beef.
#[test]
#[ignore = "blaze stage: unread; owners A4 (fight), A3 (heal cycle)"]
fn seven_rods_are_taken_at_the_spawner() {
    stage((ItemKind::Beef, 8), false).assert_seven();
}

/// The same with food to spare: 30 cooked beef.
#[test]
#[ignore = "blaze stage: unread; owners A4 (fight), A3 (heal cycle)"]
fn seven_rods_are_taken_at_the_spawner_with_thirty_cooked_beef() {
    stage((ItemKind::CookedBeef, 30), false).assert_seven();
}

/// The same with a shield in the off hand.
#[test]
#[ignore = "blaze stage: passes at 68353e3 (seed 7; 2 of 8 seeds), kept ignored as seed-sensitive; owners A11 (shield), A4"]
fn seven_rods_are_taken_at_the_spawner_with_a_shield() {
    stage((ItemKind::CookedBeef, 30), true).assert_seven();
}

/// The shield variant with company: one wither skeleton on the bridge 12 blocks south of the
/// bot as the run begins, walking in (`Melee::wither_skeleton`).
#[test]
#[ignore = "blaze stage: unread; owner A4"]
fn seven_rods_are_taken_at_the_spawner_with_a_wither_skeleton_arriving() {
    stage_with(&[(ItemKind::CookedBeef, 30)], true, SEED, |sim| {
        sim.summon_with(
            EntityKind::WitherSkeleton,
            [128.5, 61.0, 176.5],
            Brain::Melee(sim::life::Melee::wither_skeleton()),
        );
    })
    .assert_seven();
}

/// One variant over the eight seeds: a line a seed and the count that end alive with 7 rods.
/// A report: it fails only if no seed passes.
fn over_seeds(name: &str, kit: &[(ItemKind, i32)], shield: bool) {
    over_seeds_with(name, kit, shield, |_| {});
}

/// The same, with what `more` adds to the model first.
fn over_seeds_with(name: &str, kit: &[(ItemKind, i32)], shield: bool, more: impl Fn(&mut Sim)) {
    let mut passed = 0;
    let (mut all_ticks, mut all_kills, mut all_landed, mut all_rods) = (0u64, 0u64, 0u64, 0i64);
    for seed in SEEDS {
        let s = stage_with(kit, shield, seed, &more);
        let life = &s.run.sim.survival.life;
        let ok = !s.run.died && s.rods >= 7;
        passed += u32::from(ok);
        all_ticks += s.ticks;
        all_kills += life.kills;
        all_landed += life.fireballs_landed;
        all_rods += s.rods;
        println!(
            "SEED {name} seed {seed}: rods {}, alive {}, seventh at {:?}, ticks {}, kills {} \
             ({:.2} per 1,000), fireballs {} fired {} hit {} landed ({:.2} landed per 1,000) \
             {} blocked, health lost {:.1} ({}), blazes knocked {}, meals {}",
            s.rods,
            !s.run.died,
            s.seven_at,
            s.ticks,
            life.kills,
            life.kills as f64 * 1000.0 / s.ticks.max(1) as f64,
            life.fireballs_fired,
            life.fireballs_hit,
            life.fireballs_landed,
            life.fireballs_landed as f64 * 1000.0 / s.ticks.max(1) as f64,
            life.fireballs_blocked,
            s.health_lost,
            damage_by_type(life),
            life.mobs_knocked,
            s.meals,
        );
        // The shield's counts (docs/problems/shield.md wall 4).
        let sh = &s.shield;
        println!(
            "SHIELD {name} seed {seed}: raised {}, blocked {}, hit up {}, hit down {}, yields to \
             the strike {} ({} ticks), kept up for a fireball {} ticks, placing held {}, strikes \
             given back {}",
            sh.raises,
            sh.blocked,
            sh.hits_up,
            sh.hits_down,
            sh.yields,
            sh.yield_ticks,
            sh.kept_for_ball,
            sh.places_held,
            sh.strikes_held,
        );
        // A seed that died: how it ended (blaze-fight walls 17 and 19).
        if s.run.died {
            let from = s.ledger.len().saturating_sub(30);
            for l in &s.ledger[from..] {
                println!("LATE {name} seed {seed}: {l}");
            }
        }
    }
    let per = |n: u64| n as f64 * 1000.0 / all_ticks.max(1) as f64;
    println!(
        "SEEDS {name}: {passed} of {} alive with 7 rods; {all_rods} rods in all, {:.2} kills \
         and {:.2} fireballs landed per 1,000 ticks over {all_ticks} ticks",
        SEEDS.len(),
        per(all_kills),
        per(all_landed),
    );
    assert!(passed > 0, "{name}: no seed of {} passed", SEEDS.len());
}

#[test]
#[ignore = "blaze stage, eight seeds: a report, minutes long; owners A4, A3"]
fn eight_seeds_of_seven_rods_with_raw_beef() {
    over_seeds("raw beef", &[(ItemKind::Beef, 8)], false);
}

#[test]
#[ignore = "blaze stage, eight seeds: a report, minutes long; owners A4, A3"]
fn eight_seeds_of_seven_rods_with_thirty_cooked_beef() {
    over_seeds("cooked beef", &[(ItemKind::CookedBeef, 30)], false);
}

#[test]
#[ignore = "blaze stage, eight seeds: a report, minutes long; owners A11, A4"]
fn eight_seeds_of_seven_rods_with_a_shield() {
    over_seeds("shield", &[(ItemKind::CookedBeef, 30)], true);
}

/// The shield variant with a wither skeleton walking in from the bridge (as
/// `seven_rods_are_taken_at_the_spawner_with_a_wither_skeleton_arriving`): the shield and a melee
/// fight in reach (docs/problems/shield.md wall 4).
#[test]
#[ignore = "blaze stage, eight seeds: a report, minutes long; owners S2 (shield), F"]
fn eight_seeds_of_seven_rods_with_a_shield_and_a_wither_skeleton() {
    over_seeds_with(
        "shield+wither",
        &[(ItemKind::CookedBeef, 30)],
        true,
        |sim| {
            sim.summon_with(
                EntityKind::WitherSkeleton,
                [128.5, 61.0, 176.5],
                Brain::Melee(sim::life::Melee::wither_skeleton()),
            );
        },
    );
}

/// The frontier bot's own situation for the next race: what the Nether snapshot's bot holds
/// (its `snap.toml`: 8 raw iron, two logs, a plank, a crafting table, 11 raw meat, 13 sticks,
/// cobblestone, four stone pickaxes; no shield), at the fortress. A shield is 6 planks and an
/// iron ingot: it has to be made from this, and then the rods taken.
#[test]
#[ignore = "blaze stage: a shield crafted from the snapshot kit, then 7 rods; unread; owner A11"]
fn a_shield_is_made_from_the_snapshot_kit_and_seven_rods_taken() {
    let kit = [
        (ItemKind::RawIron, 8),
        (ItemKind::OakLog, 1),
        (ItemKind::BirchLog, 1),
        (ItemKind::BirchPlanks, 1),
        (ItemKind::CraftingTable, 1),
        (ItemKind::Beef, 6),
        (ItemKind::Porkchop, 5),
        (ItemKind::Stick, 13),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::Cobblestone, 326),
    ];
    let s = stage_with(&kit, false, SEED, |_| {});
    let shields = s.run.world.inventory.count(ItemKind::Shield);
    let made: Vec<&String> = s
        .ledger
        .iter()
        .filter(|l| l.contains("shield") || l.contains("furnace") || l.contains("smelt"))
        .take(20)
        .collect();
    println!("SHIELD: {shields} held at the end; ledger lines of it: {made:#?}");
    s.assert_seven();
    let blocked = s.run.sim.survival.life.fireballs_blocked;
    assert!(
        shields >= 1 || blocked > 0,
        "no shield was made or used\n{}",
        s.story()
    );
}
