//! Issue 20h, the End's paths closed loop (docs/problems/end-model.md): the planner itself (E3's
//! `end`, E4's `spikes`) driving the bot's own world, fed by the model of a synthetic End
//! (`sim::end`). The bridge from the platform with rails over the void onto the island's lip;
//! the pillar beside a spike, the wall, the strike and the dig back down, open and caged; the
//! island's walk and bunker with endermen about, never stared at. Development evidence only
//! (measurement-rules): the model has no dragon, and the blast's blocks are not broken.

use data::{BlockKind, ItemKind};
use plan::{Aim, Plan, TaskRecord};
use sim::end::{EndStage, Layout, Spike};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 6,
    simulation_distance: 3,
    peaceful: false,
};
/// Full iron with boots (2 + 6 + 5 + 2), the kit 20c makes.
const IRON: f32 = 15.0;
/// E1's measure of the seed (docs/problems/end-observe.md, `plan::end::BRIDGE_FLOOR`): the
/// island's lip is end stone at y 52 from x 68 to 72, the void from 73 to 97.
const LIP_TOP: i32 = 52;
const LIP_EDGE: i32 = 72;
/// The island's end stone is this deep (a test's, well over the bunker's three down: vanilla's is
/// noise tens deep near the fountain); E1's lip is the one thin part.
const DEPTH: i32 = 20;

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

struct Run {
    plan: Plan,
    run: bot::lockstep::Run,
    ledger: Vec<String>,
    /// The lowest the feet went while the planner ran.
    low: f64,
}

impl Run {
    fn has(&self, what: &str) -> usize {
        self.ledger.iter().filter(|l| l.contains(what)).count()
    }

    /// The last 80 lines: the story's end, where a failure is.
    fn story(&self) -> String {
        let from = self.ledger.len().saturating_sub(80);
        self.ledger[from..].join("\n")
    }

    fn kind(&self, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
        self.run
            .sim
            .state_at(x, y, z)
            .and_then(data::block_of_state)
    }

    /// The model's endermen ever stared at (metadata 18) or angered.
    fn stared_or_angry(&self) -> Vec<(i32, bool, bool)> {
        let sim = &self.run.sim;
        sim.survival
            .life
            .mobs
            .keys()
            .filter_map(|id| sim.ender(*id).map(|e| (*id, e.stared_at, e.angry)))
            .filter(|(_, s, a)| *s || *a)
            .collect()
    }
}

/// The racer, with `prime` setting where the End's order already is, on `sim` for `ticks`.
fn drive(sim: Sim, ticks: u64, prime: impl FnOnce(&mut Plan)) -> Run {
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut plan = Plan::with_table(Aim::Race, table);
    prime(&mut plan);
    let mut ledger = Vec::new();
    let mut low = f64::MAX;
    let run = bot::lockstep::run_acting(sim, "rs-end", 7, ticks, 20, |w, _| {
        if let Some(b) = w.body {
            low = low.min(b.pos[1]);
        }
        let seen = plan.event_count;
        let c = plan.step(w);
        ledger.extend(
            plan.events_since(seen)
                .iter()
                .map(|e| format!("{:>6} note  {} ({})", e.tick, e.what, e.why)),
        );
        ledger.extend(plan.take_records().into_iter().map(line));
        (c, plan.take_acts())
    });
    Run {
        plan,
        run,
        ledger,
        low,
    }
}

/// The kit the End is entered with (20c): blocks to bridge, two stone pickaxes, a sword, food,
/// full iron.
fn kit(sim: &mut Sim, end_stone: i32) {
    sim.survival.life.armor = IRON;
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::StonePickaxe, 1));
    sim.survival.give((ItemKind::CookedBeef, 32));
    for _ in 0..3 {
        sim.survival.give((ItemKind::Cobblestone, 64));
    }
    let mut left = end_stone;
    while left > 0 {
        sim.survival.give((ItemKind::EndStone, left.min(64)));
        left -= 64;
    }
}

/// The island flat at `top`, `DEPTH` deep, round (0, 0) out to `radius`, with the fountain; the
/// platform; `spikes`.
fn island(radius: i32, top: i32, spikes: Vec<Spike>) -> EndStage {
    EndStage::of(&Layout {
        island_radius: radius,
        island_top: top,
        island_depth: DEPTH,
        spikes,
        platform: true,
        fountain: true,
    })
}

/// The arrival (20e): on the platform at (100, 49, 0) with the island's lip at y 52 out to x 72,
/// as E1 measured the seed. The bot pillars up to the bridge's floor, bridges west with its rails
/// on both sides, sneaking, and steps onto the lip's end stone under the sky; never lower than the
/// platform.
#[test]
fn the_bridge_from_the_platform_has_rails_and_reaches_the_lip() {
    // The island deep to x 64; the lip from 65 to 72 three thick, with a straight edge across the
    // bridge's line so the step off is onto a full row.
    let mut stage = island(LIP_EDGE - 8, LIP_TOP, Vec::new());
    stage.island(LIP_EDGE, LIP_TOP, 3);
    stage.fill(
        (LIP_EDGE - 4, LIP_EDGE),
        (LIP_TOP - 2, LIP_TOP),
        (-3, 3),
        BlockKind::EndStone,
    );
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(7);
    kit(&mut sim, 0);
    let r = drive(sim, 9000, |_| {});
    let story = r.story();
    assert!(!r.run.died, "{story}");
    assert!(r.has("end: arrived at (100, 49, 0)") >= 1, "{story}");
    assert!(r.has("end: pillar up 4 at") >= 1, "{story}");
    assert!(r.has("end: bridge west from") >= 1, "{story}");
    assert!(r.has("end: on the island at") >= 1, "{story}");
    assert!(r.plan.end.bridged, "{story}");
    assert!(
        r.low >= 49.0,
        "the feet never under the platform's top: {}\n{story}",
        r.low
    );
    // The floor over the void, and its rails at the feet's height on both sides.
    let floor = (LIP_EDGE + 1..98)
        .filter(|x| r.kind((*x, LIP_TOP, 0)) == Some(BlockKind::Cobblestone))
        .count();
    assert_eq!(floor, 25, "the floor from x 73 to 97:\n{story}");
    let rails = (LIP_EDGE + 1..98)
        .flat_map(|x| [(x, LIP_TOP + 1, -1), (x, LIP_TOP + 1, 1)])
        .filter(|c| r.kind(*c) == Some(BlockKind::Cobblestone))
        .count();
    assert!(rails >= 46, "rails {rails} of 50:\n{story}");
    assert!(r.plan.end.guards >= 46, "guards {}", r.plan.end.guards);
    assert!(r.plan.end.laid >= 25, "laid {}", r.plan.end.laid);
}

/// A spike's crystal (20f), the bot on the island with the stock held and a bunker on record: it
/// walks to the pillar's foot, pillars up in end stone, rings its stand, walls the blast's line,
/// strikes by reach, and digs its pillar back down to the ground; the hidden blast takes 1.
fn crystal(sp: Spike) -> Run {
    let top = 60;
    let mut stage = island(64, top, vec![sp]);
    stage.spawn((sp.centre[0] - 12, top + 1, sp.centre[1]));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(7);
    kit(&mut sim, 192);
    sim.summon_crystal(sp.crystal());
    drive(sim, 2500, |p| {
        p.end.arrived = true;
        p.end.bridged = true;
        p.end.stock_over = true;
        p.bunker = Some([12, top - 2, 0]);
    })
}

fn struck_and_down(r: &Run) {
    let story = r.story();
    assert!(!r.run.died, "{story}");
    assert!(r.has("spikes: pillar up from") >= 1, "{story}");
    assert!(r.has("spikes: pillar up to") >= 1, "{story}");
    assert!(r.has("spikes: crystal gone") >= 1, "{story}");
    assert_eq!(r.run.sim.survival.life.crystals_struck, 1, "{story}");
    assert_eq!(r.run.sim.survival.life.blasts.len(), 1, "{story}");
    assert!(r.has("spikes: dig down at") >= 1, "{story}");
    assert!(r.has("spikes: pillar down at") >= 1, "{story}");
    assert!(r.has("spikes: no crystal left") >= 1, "{story}");
    assert_eq!(r.plan.spikes.pillars_down, r.plan.spikes.pillars_up);
    assert!(r.plan.spikes.pillars.is_empty(), "{story}");
    let feet = r.run.world.body.map_or(0.0, |b| b.pos[1]);
    assert!(
        feet <= 61.01,
        "back on the ground at the end, feet {feet}:\n{story}"
    );
    assert!(r.run.min_health_in_script.unwrap_or(0.0) > 14.0, "{story}");
}

#[test]
fn an_open_spikes_crystal_is_pillared_walled_struck_and_dug_down_from() {
    let sp = Layout::small().spikes[0];
    let r = crystal(sp);
    struck_and_down(&r);
    let blasts = &r.run.sim.survival.life.blasts;
    assert_eq!(blasts[0].1, 0.0, "the blast's line shut: {:?}", blasts[0]);
    assert!(r.has("Wall block at") >= 2, "{}", r.story());
}

#[test]
fn a_caged_spikes_crystal_is_struck_through_its_bars() {
    let sp = Layout::small().spikes[1];
    let r = crystal(sp);
    struck_and_down(&r);
}

/// On the island with three endermen about at eye height (20e's gaze): the bot walks to the
/// bunker's site, digs and roofs it and tunnels for its stock looking down, and never stares
/// at one or angers it.
#[test]
fn on_the_island_endermen_about_are_never_stared_at() {
    let top = 60;
    let mut stage = island(64, top, Vec::new());
    stage.spawn((40, top + 1, 0));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(7);
    kit(&mut sim, 0);
    let foot = f64::from(top + 1);
    for at in [[30.5, foot, 6.5], [20.5, foot, -8.5], [12.5, foot, 14.5]] {
        let id = sim.summon_enderman(at);
        if let Some(m) = sim.survival.life.mobs.get_mut(&id) {
            m.loot = false;
        }
    }
    let r = drive(sim, 6000, |p| {
        p.end.arrived = true;
        p.end.bridged = true;
    });
    let story = r.story();
    assert_eq!(r.stared_or_angry(), Vec::new(), "never stared at:\n{story}");
    assert!(!r.run.died, "{story}");
    assert!(r.has("end: walk to the bunker site") >= 1, "{story}");
    assert!(r.has("end: bunker dug at") >= 1, "{story}");
    assert!(r.has("end: stock run 1") >= 1, "{story}");
}

/// The dragon (20g), every crystal gone: the model's dragon (`sim::end::Dragon`, scripted) sits
/// on the fountain's pillar facing north. The bot, on the island with the stock held, digs its
/// bunker, hands over to the fight, strikes the perched dragon until it is dead (it flies off
/// after each quarter of its health and sits again), walks into the exit portal, and answers the
/// credits with a respawn; never under the danger line.
#[test]
fn the_perched_dragon_is_struck_dead_and_the_credits_answered() {
    let top = 60;
    let mut stage = island(64, top, Vec::new());
    stage.spawn((0, top + 1, 20));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(7);
    kit(&mut sim, 192);
    sim.summon_dragon(top + 1, 0.0);
    let r = drive(sim, 16000, |p| {
        p.end.arrived = true;
        p.end.bridged = true;
        p.end.stock_over = true;
    });
    let story = r.story();
    let d = r.run.sim.dragon.clone().unwrap();
    let parts = |p: i32| d.hits.iter().filter(|h| h.1 == p).count();
    let tally = format!(
        "hits head {} neck {} body {} own {}; perches {}, blows {}, died {:?}, won {:?}, respawns {}",
        parts(1),
        parts(2),
        parts(3),
        parts(0),
        d.perches,
        d.blows,
        d.died_at,
        d.won_at,
        d.respawns_asked
    );
    assert!(!r.run.died, "{tally}\n{story}");
    assert!(r.has("dragon fight") >= 1, "{tally}\n{story}");
    assert!(d.died_at.is_some(), "{tally}\n{story}");
    assert!(d.perches >= 2, "{tally}\n{story}");
    assert!(d.portal_lit && d.won_at.is_some(), "{tally}\n{story}");
    assert!(r.run.world.won, "{tally}\n{story}");
    assert!(r.has("credits") >= 1, "{tally}\n{story}");
    assert!(
        r.has("respawn after the credits (1)") >= 1,
        "{tally}\n{story}"
    );
    assert!(d.respawns_asked >= 1, "{tally}\n{story}");
    assert!(
        r.run.min_health_in_script.unwrap_or(0.0) > 8.0,
        "never under the danger line: {:?}\n{tally}\n{story}",
        r.run.min_health_in_script
    );
    assert!(
        r.run.sim.violations.is_empty(),
        "{:?}",
        r.run.sim.violations
    );
}
