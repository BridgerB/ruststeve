//! Issue 16, the Nether walk on the real ground (docs/problems/nether-testbed.md): the chunk
//! columns of this world's Nether portal exit, as the server sent them to lane 5 of race
//! 45194253932 on arrival (`fixtures/nether-exit.rsw`, cut by `nether_capture.rs`: columns x -1
//! to 5, z -4 to 6, so blocks x -16 to 95, z -64 to 111), played by the model. The portal stands
//! at (22 to 23, 34 to 35, -20); the nearest nether bricks are at (61, 30 to 37, 95), 121 blocks
//! off. Outside the columns kept the model's world is solid and the bot's unloaded.
//!
//! Nothing here is ignored any more: the kit bot reaches the bricks (probe 45343484013).

use std::path::Path;

use bot::captured::CapturedWorld;
use data::{BlockKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::nether::{NETHER, nether_type};
use sim::{Config, ModelWorld, Sim};

const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};

/// Where the equipped Nether snapshot's bot stands (its `snap.toml`: rs-l05-b00 at server tick
/// 24,960), and the bricks it saw.
const START: [f64; 3] = [22.415797335549968, 34.0, -19.5];
const BRICKS: (i32, i32, i32) = (61, 33, 95);

/// What that bot holds (`snap.toml`'s `[items]`), all of it into the inventory: the model wears
/// no armour and has no mobs to wear it against.
const KIT: [(ItemKind, i32); 26] = [
    (ItemKind::StonePickaxe, 1),
    (ItemKind::StonePickaxe, 1),
    (ItemKind::StonePickaxe, 1),
    (ItemKind::StonePickaxe, 1),
    (ItemKind::IronSword, 1),
    (ItemKind::Cobblestone, 390),
    (ItemKind::CobbledDeepslate, 130),
    (ItemKind::Granite, 89),
    (ItemKind::Diorite, 43),
    (ItemKind::Dirt, 24),
    (ItemKind::Tuff, 23),
    (ItemKind::Gravel, 8),
    (ItemKind::SmoothBasalt, 1),
    (ItemKind::Beef, 6),
    (ItemKind::Porkchop, 5),
    (ItemKind::Leather, 4),
    (ItemKind::RawIron, 8),
    (ItemKind::OakLog, 1),
    (ItemKind::BirchLog, 1),
    (ItemKind::BirchPlanks, 1),
    (ItemKind::Stick, 13),
    (ItemKind::CraftingTable, 1),
    (ItemKind::FlintAndSteel, 1),
    (ItemKind::Bucket, 1),
    (ItemKind::WaterBucket, 1),
    (ItemKind::WaterBucket, 1),
];

/// The exit as a model world in the Nether, the player at the snapshot bot's place.
fn exit() -> ModelWorld {
    captured("nether-exit.rsw", START)
}

/// A captured fixture as a model world in the Nether.
fn captured(file: &str, spawn: [f64; 3]) -> ModelWorld {
    let path = format!("{}/tests/fixtures/{file}", env!("CARGO_MANIFEST_DIR"));
    let mut world = CapturedWorld::read(Path::new(&path))
        .unwrap()
        .model(spawn)
        .unwrap();
    world.dimension = NETHER.to_owned();
    world.dimension_type = nether_type();
    world
}

fn kind(sim: &Sim, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

/// The fixture is the ground the Nether bots died on: the cells of blaze wall 22's block view
/// (probe 45202757009) that the bot had not dug, the portal, and the bricks.
#[test]
fn the_exit_is_the_ground_of_blaze_wall_22() {
    let world = exit();
    assert_eq!(world.columns.len(), 77);
    assert!(world.fast_lava());
    let sim = Sim::new(CONFIG, world);
    let expect = [
        ((21, 34, -20), BlockKind::Obsidian),
        ((22, 34, -20), BlockKind::NetherPortal),
        ((22, 35, -20), BlockKind::NetherPortal),
        ((27, 34, -21), BlockKind::CaveAir),
        ((33, 34, -17), BlockKind::MagmaBlock),
        ((28, 34, -15), BlockKind::Gravel),
        // The block view's "soul_s" cells are soul soil, a full block, not soul sand.
        ((33, 32, -18), BlockKind::SoulSoil),
        ((33, 31, -18), BlockKind::SoulSoil),
        ((27, 31, -18), BlockKind::Gravel),
        ((30, 31, -18), BlockKind::Netherrack),
        ((32, 30, -21), BlockKind::Lava),
        ((30, 29, -19), BlockKind::MagmaBlock),
        ((29, 29, -18), BlockKind::MagmaBlock),
        ((30, 29, -21), BlockKind::Lava),
        ((33, 29, -21), BlockKind::Lava),
        (BRICKS, BlockKind::NetherBricks),
        ((61, 30, 95), BlockKind::NetherBricks),
    ];
    for (pos, want) in expect {
        assert_eq!(kind(&sim, pos), Some(want), "at {pos:?}");
    }
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

/// The kit bot's own situation: the equipped Nether snapshot's bot, with what it holds, where it
/// stands, the fortress aim. Twelve thousand ticks is ten minutes; the bricks are 121 blocks off.
#[test]
fn the_kit_bot_reaches_the_bricks_from_the_real_exit() {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Fortress, table);
    let mut sim = Sim::new(CONFIG, exit());
    for stack in KIT {
        assert_eq!(sim.survival.give(stack), 0, "{stack:?} did not fit");
    }
    // The lava falls beside the portal run once the bot's arrival sets the chunks ticking: the
    // ledge floods 700 to 900 ticks in (docs/problems/nether-walk.md).
    let woken = sim.wake_fluids();
    let mut ledger = Vec::new();
    let mut reached_at = None;
    let mut track = Vec::new();
    let run = bot::lockstep::run_acting(sim, "rs-exit", 1, 12_000, 20, |w, t| {
        let c = plan.step(w);
        ledger.extend(plan.take_records().into_iter().map(line));
        if reached_at.is_none() && world::stage::at_fortress(w) {
            reached_at = Some(t);
        }
        if t % 600 == 0 {
            let p = w.body.map(|b| b.pos.map(|v| v.floor() as i32));
            track.push(format!("{t}: {p:?} health {:?}", w.player.health));
        }
        // A walk in hand, ten ticks apart over one stalled far walk: where the body is and
        // the waypoint and move it is on.
        if (4130..4470).contains(&t)
            && t % 10 == 0
            && let Some(walk) = plan.task.walk()
        {
            track.push(format!(
                "{t}: body {:?} ground {:?}; at {} of {}, next {:?} by {:?}, from {:?}, replans {}, laying {:?} tries {}",
                w.body.map(|b| b.pos.map(|v| (v * 100.0).round() / 100.0)),
                w.body.map(|b| b.on_ground),
                walk.at,
                walk.waypoints.as_ref().map_or(0, Vec::len),
                walk.waypoints.as_ref().and_then(|p| p.get(walk.at)),
                walk.more.kinds.get(walk.at),
                walk.more.from,
                walk.replans,
                walk.more.laying,
                walk.more.place_tries,
            ));
        }
        (c, plan.take_acts())
    });
    let n = ledger.len();
    let (head, tail) = if n > 120 {
        (&ledger[..60], &ledger[n - 60..])
    } else {
        (&ledger[..], &ledger[..0])
    };
    let story = format!(
        "{woken} fluid cells woken; at {:?}, health {:?} (lowest {:?}), died {}, reached at {reached_at:?}, escapes {}, \
         violations {:?}\nevery 600 ticks:\n{}\nledger ({n} lines):\n{}\n  ...\n{}",
        run.world.body.map(|b| b.pos),
        run.world.player.health,
        run.min_health_in_script,
        run.died,
        plan.escapes,
        run.sim.violations,
        track.join("\n"),
        head.join("\n"),
        tail.join("\n"),
    );
    assert!(!run.died, "died\n{story}");
    assert!(reached_at.is_some(), "never at the fortress\n{story}");
}

/// The fortress itself (`fixtures/nether-fortress.rsw`): the 81 chunk columns x 2 to 10, z 4 to
/// 12 (blocks x 32 to 175, z 64 to 207) as the server sent them to lane 0 of race 45337739022,
/// the bot that tunnelled to the blaze spawner, cut at its server tick 12,000 where it stood at
/// (129.8, 62.2, 164.3) on the bridges at y 61 to 62. The stage for the fight on the real
/// ground: the bridges are there and so is a spawner near that place.
#[test]
fn the_fortress_fixture_holds_the_bridges_and_a_spawner() {
    let world = captured("nether-fortress.rsw", [129.79, 62.17, 164.3]);
    assert_eq!(world.columns.len(), 81);
    assert!(world.fast_lava());
    let sim = Sim::new(CONFIG, world);
    for pos in [(129, 61, 163), (130, 61, 163), (129, 60, 164)] {
        assert_eq!(kind(&sim, pos), Some(BlockKind::NetherBricks), "at {pos:?}");
    }
    let spawners: Vec<(i32, i32, i32)> = (113..=145)
        .flat_map(|x| (56..=72).flat_map(move |y| (148..=180).map(move |z| (x, y, z))))
        .filter(|p| kind(&sim, *p) == Some(BlockKind::Spawner))
        .collect();
    assert!(
        !spawners.is_empty(),
        "no spawner within 16 of the bot's place"
    );
    println!("spawners: {spawners:?}");
}
