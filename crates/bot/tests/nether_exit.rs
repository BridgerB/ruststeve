//! Issue 16, the Nether walk on the real ground (docs/problems/nether-testbed.md): the chunk
//! columns of this world's Nether portal exit, as the server sent them to lane 5 of race
//! 45194253932 on arrival (`fixtures/nether-exit.rsw`, cut by `nether_capture.rs`: columns x -1
//! to 5, z -4 to 6, so blocks x -16 to 95, z -64 to 111), played by the model. The portal stands
//! at (22 to 23, 34 to 35, -20); the nearest nether bricks are at (61, 30 to 37, 95), 121 blocks
//! off. Outside the columns kept the model's world is solid and the bot's unloaded.
//!
//! Run the ignored one with `cargo test -p bot --test nether_exit -- --ignored --nocapture`.

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
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/nether-exit.rsw"
    );
    let mut world = CapturedWorld::read(Path::new(path))
        .unwrap()
        .model(START)
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
        ((33, 32, -18), BlockKind::SoulSand),
        ((33, 31, -18), BlockKind::SoulSand),
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
#[ignore = "blaze wall: the kit bot at the real portal exit does not reach the bricks; owners A2, A7"]
fn the_kit_bot_reaches_the_bricks_from_the_real_exit() {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Fortress, table);
    let mut sim = Sim::new(CONFIG, exit());
    for stack in KIT {
        assert_eq!(sim.survival.give(stack), 0, "{stack:?} did not fit");
    }
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
        (c, plan.take_acts())
    });
    let n = ledger.len();
    let (head, tail) = if n > 120 {
        (&ledger[..60], &ledger[n - 60..])
    } else {
        (&ledger[..], &ledger[..0])
    };
    let story = format!(
        "at {:?}, health {:?} (lowest {:?}), died {}, reached at {reached_at:?}, escapes {}, \
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
