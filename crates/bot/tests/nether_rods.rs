//! Issue 16's whole stage, "7 rods at the spawner", closed loop on the real fortress
//! (docs/problems/nether-testbed.md): `fixtures/nether-fortress.rsw` (the columns round the
//! blaze spawner as the server sent them to lane 0 of race 45337739022), the spawner running by
//! `BaseSpawner`'s rules, blazes that shoot, rise to the bot's eye height and drop a rod to a
//! player's kill (0 or 1, the jar's loot table), the model's hunger, eating and healing.
//! Development evidence only: the model has no blaze that closes in, no other mobs, no
//! knockback (crates/sim/src/life.rs says what it holds).
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
fn fortress() -> (Sim, (i32, i32, i32)) {
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
    let mut sim = Sim::new(CONFIG, world).seeded(7);
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
    stage_with(food, shield, |_| {})
}

/// The same, with what `more` adds to the model first.
fn stage_with(food: (ItemKind, i32), shield: bool, more: impl Fn(&mut Sim)) -> Stage {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Race, table);
    let (mut sim, spawner) = fortress();
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give(food);
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
    let run = bot::lockstep::run_acting(sim, "rs-rods", 1, TICKS, 20, |w, t| {
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
             blazes spawned {}, alive at the end {}, fireballs fired {} hit {} blocked {}, \
             health lost {:.1} ({:.1} a kill), lowest health {:?}, meals {}, fire cells within 6 of the \
             spawner {}, blaze-ticks within 1.5 of the eye's height {} of {}, spawner at {:?}, the bot at {:?}\n\
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
            life.fireballs_blocked,
            self.health_lost,
            self.health_lost / kills.max(1) as f32,
            self.run.min_health_in_script,
            self.meals,
            fire,
            self.level_ticks,
            self.blaze_ticks,
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
    let (sim, spawner) = fortress();
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
#[ignore = "blaze stage: unread; owners A11 (shield), A4"]
fn seven_rods_are_taken_at_the_spawner_with_a_shield() {
    stage((ItemKind::CookedBeef, 30), true).assert_seven();
}

/// The shield variant with company: one wither skeleton on the bridge 12 blocks south of the
/// bot as the run begins, walking in (`Melee::wither_skeleton`).
#[test]
#[ignore = "blaze stage: unread; owner A4"]
fn seven_rods_are_taken_at_the_spawner_with_a_wither_skeleton_arriving() {
    stage_with((ItemKind::CookedBeef, 30), true, |sim| {
        sim.summon_with(
            EntityKind::WitherSkeleton,
            [128.5, 61.0, 176.5],
            Brain::Melee(sim::life::Melee::wither_skeleton()),
        );
    })
    .assert_seven();
}
