//! Issue 16, the fight at the fortress (docs/problems/nether-testbed.md): the shapes of the
//! fight deaths as arenas of the model (`sim::nether::walls`), closed loop with the goal table's
//! always-rows. The model's foes are small: a blaze only hovers (no fireballs); a wither
//! skeleton walks straight at the bot and hits for 8 with the wither effect
//! (`sim::life::Melee::wither_skeleton`); the bot wears 13 armour points, the kit's iron.
//! A shape the bot does not get through today is `#[ignore]`d with its reason and owner.
//!
//! Run the ignored ones with `cargo test -p bot --test nether_fight -- --ignored --nocapture`.

use data::{BlockKind, EntityKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::life::{Brain, Melee};
use sim::nether::{Wall, walls};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};

/// Iron helmet, chestplate and leggings: 2 + 6 + 5 armour points.
const IRON: f32 = 13.0;

struct Fight {
    plan: Plan,
    run: bot::lockstep::Run,
    ledger: Vec<String>,
    /// Ticks the body was in lava, and ticks its feet's cell held fire.
    lava_ticks: u64,
    fire_ticks: u64,
    /// The lowest the feet went.
    lowest: f64,
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

/// The bot in `wall` with an iron sword, the kit's armour and `health`, `aim`ed, the foes set by
/// `summon`, for `ticks`.
fn fight(wall: &Wall, aim: Aim, health: f32, ticks: u64, summon: impl Fn(&mut Sim)) -> Fight {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(aim, table);
    let mut sim = wall.sim(CONFIG).unwrap().seeded(3);
    sim.health = health;
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give((ItemKind::Cobblestone, 64));
    sim.survival.give((ItemKind::CookedBeef, 16));
    summon(&mut sim);
    let mut ledger = Vec::new();
    let (mut lava_ticks, mut fire_ticks, mut lowest) = (0, 0, f64::MAX);
    let run = bot::lockstep::run_acting(sim, "rs-fight", 1, ticks, 20, |w, _| {
        let c = plan.step(w);
        ledger.extend(plan.take_records().into_iter().map(line));
        if let Some(b) = w.body {
            lava_ticks += u64::from(b.in_lava);
            let feet = world::BlockPos::new(
                b.pos[0].floor() as i32,
                b.pos[1].floor() as i32,
                b.pos[2].floor() as i32,
            );
            fire_ticks += u64::from(w.block_at(feet) == Some(BlockKind::Fire));
            lowest = lowest.min(b.pos[1]);
        }
        (c, plan.take_acts())
    });
    Fight {
        plan,
        run,
        ledger,
        lava_ticks,
        fire_ticks,
        lowest,
    }
}

impl Fight {
    fn story(&self) -> String {
        let n = self.ledger.len();
        let (head, tail) = if n > 80 {
            (&self.ledger[..40], &self.ledger[n - 40..])
        } else {
            (&self.ledger[..], &self.ledger[..0])
        };
        format!(
            "at {:?}, health {:?} (lowest {:?}), died {}, kills {}, swings {} sent {} landed, \
             {} ticks in lava, {} in fire, lowest y {:.1}, mobs left {:?}\nledger ({n} lines):\n{}\n  ...\n{}",
            self.run.world.body.map(|b| b.pos),
            self.run.world.player.health,
            self.run.min_health_in_script,
            self.run.died,
            self.run.sim.survival.life.kills,
            self.plan.swings_sent,
            self.plan.swings_landed,
            self.lava_ticks,
            self.fire_ticks,
            self.lowest,
            self.run
                .sim
                .survival
                .life
                .mobs
                .values()
                .map(|m| (m.kind, m.pos.map(|v| v.round()), m.health))
                .collect::<Vec<_>>(),
            head.join("\n"),
            tail.join("\n"),
        )
    }
}

fn deck() -> f64 {
    f64::from(walls::fortress_bridge(3, true).start.1)
}

/// The model's wither skeleton by itself: it walks the bridge to a bot that does nothing and
/// kills it, armour and all. The arenas below are not safe by default.
#[test]
fn a_wither_skeleton_walks_the_bridge_and_kills_an_idle_bot() {
    let wall = walls::fortress_bridge(3, true);
    let mut sim = wall.sim(CONFIG).unwrap();
    sim.survival.life.armor = IRON;
    sim.summon_with(
        EntityKind::WitherSkeleton,
        [10.5, deck(), 0.5],
        Brain::Melee(Melee::wither_skeleton()),
    );
    let run = bot::lockstep::run(sim, "rs-fight", 1, 600, 20, |_, _| {
        physics::Controls::default()
    });
    assert!(run.died, "health {:?}", run.world.player.health);
    assert!(
        run.world
            .entities
            .values()
            .any(|e| e.kind == EntityKind::WitherSkeleton && (e.pos[0] - 0.5).abs() < 2.0)
    );
}

/// (a) A blaze hovers 17 over the eye, out of any reach, and a wither skeleton comes along the
/// bridge: the bot fights the foe it can hit and kills it.
#[test]
#[ignore = "blaze fight (a): 'defend WitherSkeleton (Retreat)', 0 swings, dead on the bridge; owner A4"]
fn the_melee_foe_in_reach_is_fought_under_a_blaze_out_of_reach() {
    let wall = walls::fortress_bridge(3, true);
    let f = fight(&wall, Aim::Hold, 20.0, 1200, |sim| {
        sim.summon(EntityKind::Blaze, [4.5, deck() + 1.62 + 17.0, 0.5]);
        sim.summon_with(
            EntityKind::WitherSkeleton,
            [12.5, deck(), 0.5],
            Brain::Melee(Melee::wither_skeleton()),
        );
    });
    assert!(!f.run.died, "died\n{}", f.story());
    assert!(
        f.run.sim.survival.life.kills >= 1,
        "the wither skeleton lives\n{}",
        f.story()
    );
    assert_eq!(f.lava_ticks, 0, "in lava\n{}", f.story());
}

/// (b) The same foe on a bridge one wide with no rails: the fight never leaves the deck.
#[test]
#[ignore = "blaze fight (b): stays on the deck but retreats with 0 swings and dies there; owner A4"]
fn a_fight_on_a_one_wide_bridge_never_ends_in_lava() {
    let wall = walls::fortress_bridge(1, false);
    let f = fight(&wall, Aim::Hold, 20.0, 1200, |sim| {
        sim.summon_with(
            EntityKind::WitherSkeleton,
            [12.5, deck(), 0.5],
            Brain::Melee(Melee::wither_skeleton()),
        );
    });
    assert!(!f.run.died, "died\n{}", f.story());
    assert!(f.lowest >= deck() - 0.01, "off the deck\n{}", f.story());
    assert_eq!(f.lava_ticks, 0, "in lava\n{}", f.story());
    assert!(
        f.run.sim.survival.life.kills >= 1,
        "the wither skeleton lives\n{}",
        f.story()
    );
}

/// (b) again, three wide and open-edged, the foe coming at an angle: a fight that backs or
/// circles has an edge on each side.
#[test]
#[ignore = "blaze fight (b): stays on the deck but retreats with 0 swings and dies there; owner A4"]
fn a_fight_on_an_open_bridge_never_ends_in_lava() {
    let wall = walls::fortress_bridge(3, false);
    let f = fight(&wall, Aim::Hold, 20.0, 1200, |sim| {
        sim.summon_with(
            EntityKind::WitherSkeleton,
            [12.5, deck(), 1.5],
            Brain::Melee(Melee::wither_skeleton()),
        );
    });
    assert!(!f.run.died, "died\n{}", f.story());
    assert!(f.lowest >= deck() - 0.01, "off the deck\n{}", f.story());
    assert_eq!(f.lava_ticks, 0, "in lava\n{}", f.story());
}

/// (c) A blaze on the ground four blocks east (a foe the bot fights, where a wither skeleton is
/// one it retreats from) with a fire cell between it and the bot: the fight goes round the
/// fire, never through it.
#[test]
#[ignore = "blaze fight (c): unread with a blaze as the foe; owner A4"]
fn a_fight_never_stands_in_fire() {
    let wall = walls::fire_beside();
    let y = f64::from(wall.start.1);
    let f = fight(&wall, Aim::Hold, 20.0, 600, |sim| {
        sim.summon(EntityKind::Blaze, [4.5, y, 0.5]);
    });
    assert!(!f.run.died, "died\n{}", f.story());
    assert_eq!(f.fire_ticks, 0, "stood in fire\n{}", f.story());
    assert!(
        f.run.sim.survival.life.kills >= 1,
        "the foe lives\n{}",
        f.story()
    );
}

/// (d) A hurt bot at the fortress with a blaze in sight and a wall near: when it rests, it ends
/// out of the blaze's sight.
#[test]
#[ignore = "blaze fight (d): takes cover at (-5, 34, 0) by tick 60, then leaves it; unread at the end; owner A3"]
fn a_hurt_bot_rests_out_of_the_blazes_sight() {
    let wall = walls::shelter_wall();
    let y = f64::from(wall.start.1);
    let f = fight(&wall, Aim::Race, 8.0, 600, |sim| {
        sim.summon(EntityKind::Blaze, [7.5, y + 3.0, 0.5]);
    });
    assert!(!f.run.died, "died\n{}", f.story());
    let w = &f.run.world;
    let body = w.body.unwrap();
    // The blaze's eye as `cover::shooters` takes it, from the model: wherever the bot ended.
    let eyes: Vec<[f64; 3]> = f
        .run
        .sim
        .survival
        .life
        .mobs
        .values()
        .map(|m| [m.pos[0], m.pos[1] + 1.2, m.pos[2]])
        .collect();
    assert_eq!(eyes.len(), 1, "the blaze is gone\n{}", f.story());
    assert!(
        !plan::cover::seen_at(w, &eyes, plan::path::feet_cell(body.pos)),
        "in the blaze's sight\n{}",
        f.story()
    );
}

/// The shield and the dodge: a blaze that shoots (`Brain::blaze`: three small fireballs each
/// 178 ticks) hovers twelve blocks along a railed bridge, eight over the deck; the bot holds a
/// shield with its sword. Over five volleys no fireball lands.
#[test]
fn no_fireball_of_five_volleys_lands() {
    let wall = walls::fortress_bridge(3, true);
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Hold, table);
    let mut sim = wall.sim(CONFIG).unwrap().seeded(3);
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give((ItemKind::Shield, 1));
    sim.summon_with(EntityKind::Blaze, [12.5, deck() + 8.0, 0.5], Brain::blaze());
    let mut ledger = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-fight", 1, 900, 20, |w, _| {
        let c = plan.step(w);
        ledger.extend(plan.take_records().into_iter().map(line));
        (c, plan.take_acts())
    });
    let life = &run.sim.survival.life;
    let story = format!(
        "at {:?}, health {:?} (lowest {:?}), died {}, fireballs {} hit {} blocked, kills {}\n{}",
        run.world.body.map(|b| b.pos),
        run.world.player.health,
        run.min_health_in_script,
        run.died,
        life.fireballs_hit,
        life.fireballs_blocked,
        life.kills,
        ledger.join("\n"),
    );
    assert!(!run.died, "died\n{story}");
    assert_eq!(life.fireballs_hit, 0, "burned\n{story}");
    assert!(life.fireballs_blocked > 0, "none blocked\n{story}");
    assert!(
        run.sim.violations.is_empty(),
        "{:?}\n{story}",
        run.sim.violations
    );
}
