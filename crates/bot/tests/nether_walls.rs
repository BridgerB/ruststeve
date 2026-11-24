//! Issue 16, the Nether walk (docs/problems/nether-testbed.md): the walls between the portal and
//! the fortress, each laid as a Nether stage of the model (`sim::nether::walls`) and crossed
//! closed loop by the planner with the fortress aim. A wall the bot does not cross today is
//! `#[ignore]`d with its reason; whoever gets a bot across removes the ignore.
//!
//! Run the ignored ones with `cargo test -p bot --test nether_walls -- --ignored --nocapture`.

use data::ItemKind;
use plan::{Aim, Plan, TaskRecord};
use sim::Config;
use sim::nether::{Wall, walls};

/// The race's distances (config/race.toml), and Normal's hunger and healing, not Peaceful's.
const CONFIG: Config = Config {
    view_distance: 8,
    simulation_distance: 3,
    peaceful: false,
};

/// What the brief's bot holds: a stone pickaxe and a stack of cobblestone.
const KIT: [(ItemKind, i32); 2] = [(ItemKind::StonePickaxe, 1), (ItemKind::Cobblestone, 64)];

struct Crossing {
    wall: Wall,
    plan: Plan,
    run: bot::lockstep::Run,
    /// The ledger: every task's start and end and every note, in order.
    ledger: Vec<String>,
    /// The first tick the feet were within `FORTRESS_NEAR` of bricks seen exposed.
    reached_at: Option<u64>,
}

/// The bot with `kit` at the wall's start, the planner aiming at the fortress, for `ticks`.
fn cross(wall: Wall, kit: &[(ItemKind, i32)], ticks: u64) -> Crossing {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Fortress, table);
    let mut sim = wall.sim(CONFIG).unwrap();
    for stack in kit {
        sim.survival.give(*stack);
    }
    let mut ledger = Vec::new();
    let mut reached_at = None;
    let run = bot::lockstep::run_acting(sim, "rs-nether", 1, ticks, 20, |w, t| {
        let c = plan.step(w);
        ledger.extend(plan.take_records().into_iter().map(line));
        if reached_at.is_none() && world::stage::at_fortress(w) {
            reached_at = Some(t);
        }
        (c, plan.take_acts())
    });
    Crossing {
        wall,
        plan,
        run,
        ledger,
        reached_at,
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

impl Crossing {
    /// Where the run ended and the planner's ledger: its first 40 lines and its last 40, which
    /// is where a crossing that fails says why.
    fn story(&self) -> String {
        let body = self.run.world.body.map(|b| b.pos);
        let from_bricks = body.map(|p| {
            let b = self.wall.bricks;
            ((p[0] - f64::from(b.0) - 0.5).powi(2)
                + (p[1] - f64::from(b.1)).powi(2)
                + (p[2] - f64::from(b.2) - 0.5).powi(2))
            .sqrt()
        });
        let n = self.ledger.len();
        let (head, tail) = if n > 80 {
            (&self.ledger[..40], &self.ledger[n - 40..])
        } else {
            (&self.ledger[..], &self.ledger[..0])
        };
        format!(
            "{}: at {body:?}, {from_bricks:.1?} from the bricks at {:?}; health {:?} (lowest {:?}), \
             died {}, reached at {:?}, escapes {}, task {:?}\nledger ({n} lines):\n{}\n  ...\n{}",
            self.wall.name,
            self.wall.bricks,
            self.run.world.player.health,
            self.run.min_health_in_script,
            self.run.died,
            self.reached_at,
            self.plan.escapes,
            self.plan.task,
            head.join("\n"),
            tail.join("\n"),
        )
    }

    /// The outcome every wall asks for: at the fortress, alive, and nothing a server would
    /// refuse on the way.
    fn assert_reached(&self) {
        assert!(!self.run.died, "died\n{}", self.story());
        assert!(
            self.reached_at.is_some(),
            "never at the fortress\n{}",
            self.story()
        );
        assert!(
            self.run.sim.violations.is_empty(),
            "{:?}\n{}",
            self.run.sim.violations,
            self.story()
        );
    }
}

/// The stage itself: the bot joins in the Nether, sees the bricks from where it stands (an
/// exposed face in a loaded column, no X-ray) and is not yet at them.
#[test]
fn every_wall_is_joined_in_the_nether_with_its_bricks_in_view() {
    for wall in walls::all() {
        let run = bot::lockstep::run(wall.sim(CONFIG).unwrap(), "rs-nether", 1, 5, 0, |_, _| {
            physics::Controls::default()
        });
        let w = &run.world;
        assert!(
            world::stage::in_nether(w),
            "{}: not in the Nether",
            wall.name
        );
        let seen = world::stage::bricks_near(w, 8);
        assert!(
            seen.contains(&wall.bricks),
            "{}: the bricks at {:?} are not on record: {:?}",
            wall.name,
            wall.bricks,
            seen.first()
        );
        assert!(
            !world::stage::at_fortress(w),
            "{}: met at the start",
            wall.name
        );
        assert_eq!(run.decode_errors, 0, "{}", wall.name);
    }
}

/// The control: 30 blocks of netherrack are walked.
#[test]
fn a_netherrack_flat_is_walked_to_the_bricks() {
    let c = cross(walls::netherrack_flat(), &KIT, 1200);
    c.assert_reached();
    assert_eq!(c.run.min_health_in_script, Some(20.0), "{}", c.story());
}

/// (a) Soul sand all the way. Crossed today, though not by a route (the pathfinder has none:
/// plan/tests/nether_routes.rs): the fortress task's level tunnel steps across it. Kept as a
/// guard for whoever changes either.
#[test]
fn a_soul_sand_flat_is_crossed_to_the_bricks() {
    let c = cross(walls::soul_sand_flat(), &KIT, 2400);
    c.assert_reached();
}

/// (b) A lava moat 4 wide with no way round.
#[test]
#[ignore = "blaze wall: stands at the moat's bank, alive, tunnel tasks ending with no refusal; owner A2"]
fn a_lava_moat_is_crossed_to_the_bricks() {
    let c = cross(walls::lava_moat(), &KIT, 2400);
    c.assert_reached();
}

/// (c) The lake of the lava sea under a hill: through the rock over it. Four pickaxes, the kit
/// bot's count: 60 blocks of tunnel two high is more than one stone pickaxe's 131 uses.
#[test]
#[ignore = "blaze wall: level tunnels end at the lake's shore, never up into the hill; owner A7"]
fn a_lava_lake_under_a_hill_is_tunnelled_over_to_the_bricks() {
    let kit = [
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StonePickaxe, 1),
        (ItemKind::Cobblestone, 64),
    ];
    let c = cross(walls::lake_under_a_hill(), &kit, 8000);
    c.assert_reached();
}

/// (d) Lava down the bot's own stairs: sealed out, then on to the bricks.
#[test]
#[ignore = "blaze wall 22: the head cell sealed at ticks 61 and 71, then dead in the lava; owners A2, A7"]
fn lava_at_the_door_of_a_stair_cut_is_sealed_out() {
    let c = cross(walls::stair_cut(), &KIT, 2400);
    c.assert_reached();
    // Sealed out, not swum through: lava takes 4 a hit.
    assert!(
        c.run.min_health_in_script.is_some_and(|h| h >= 16.0),
        "burned\n{}",
        c.story()
    );
}
