//! Issue 77 (brain/ruststeve/design/chests.md, "The planner"): what the stash planner puts away
//! before a leg and fetches after a respawn, on hand-built need tables, then the planner on the
//! server model: the home chest made from planks, the surplus-for-leg stashed, fetched back
//! after a respawn; and with no wood to spare no chest is begun.

use std::collections::BTreeMap;
use std::sync::Arc;

use data::ItemKind;
use plan::keep::{KEEP_TOML, Keep};
use plan::stash::{Leg, STASH_TOML, fetch_list, parse_legs, stash_list, wood_to_spare};
use plan::{Aim, Plan, Task};
use protocol::decode;
use sim::{Config, In, Sim, flat_world};
use world::stage::Stage;
use world::{Act, Effect, Event, World};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// The keep table as compiled in: the bow's feathers (16) and string (8) needed by `bow_armed`
/// and `win`, the stone (64) by `win` and `during` the Nether.
fn needs() -> Keep {
    Keep::parse(KEEP_TOML).unwrap()
}

fn nether() -> Leg {
    parse_legs(STASH_TOML)
        .unwrap()
        .into_iter()
        .find(|l| l.name == "nether")
        .unwrap()
}

fn counts(c: &[(ItemKind, i32)]) -> BTreeMap<ItemKind, i32> {
    c.iter().copied().collect()
}

#[test]
fn the_legs_parse_and_the_end_has_nothing_after_it() {
    let legs = parse_legs(STASH_TOML).unwrap();
    let names: Vec<&str> = legs.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["nether", "end"]);
    assert_eq!(legs[0].portal, data::BlockKind::NetherPortal);
    assert_eq!(legs[1].portal, data::BlockKind::EndPortal);
    assert_eq!(legs[0].after().first(), Some(&Stage::Overworld));
    assert!(legs[1].after().is_empty(), "nothing after the End");
}

#[test]
fn the_stash_takes_what_is_needed_after_the_leg_and_keeps_the_legs_own() {
    let held = counts(&[
        (ItemKind::Feather, 20),
        (ItemKind::String, 3),
        (ItemKind::Cobblestone, 100),
        (ItemKind::Dirt, 10),
    ]);
    let list = stash_list(&held, &needs(), &nether());
    // Feathers up to the bow's 16 (4 over are junk, not stashed); all 3 string; the stone over
    // the leg's 64; no row for dirt.
    assert_eq!(
        counts(&list),
        counts(&[
            (ItemKind::Cobblestone, 36),
            (ItemKind::String, 3),
            (ItemKind::Feather, 16),
        ])
    );
    // The End: no stage after it, nothing stashed.
    let end = parse_legs(STASH_TOML).unwrap().pop().unwrap();
    assert!(stash_list(&held, &needs(), &end).is_empty());
}

#[test]
fn the_fetch_takes_what_a_stage_ahead_wants_and_no_more() {
    let chest = counts(&[(ItemKind::Feather, 16), (ItemKind::Cobblestone, 36)]);
    let held = counts(&[(ItemKind::Feather, 4), (ItemKind::Cobblestone, 64)]);
    assert_eq!(
        fetch_list(&chest, &held, &needs(), Some(Stage::Nether)),
        vec![(ItemKind::Feather, 12)]
    );
    // Past the bow line's stages nothing is wanted but what the win still counts.
    assert_eq!(
        counts(&fetch_list(
            &chest,
            &counts(&[]),
            &needs(),
            Some(Stage::End)
        )),
        counts(&[(ItemKind::Cobblestone, 36), (ItemKind::Feather, 16)])
    );
}

#[test]
fn wood_to_spare_keeps_the_legs_logs() {
    // READY_LOGS 4 logs are 16 planks kept; a chest is 8 more, a table 4 besides.
    assert_eq!(
        wood_to_spare(&counts(&[(ItemKind::OakLog, 4)]), true),
        (0, 8)
    );
    assert_eq!(
        wood_to_spare(
            &counts(&[(ItemKind::OakLog, 4), (ItemKind::OakPlanks, 8)]),
            true
        ),
        (8, 8)
    );
    assert_eq!(
        wood_to_spare(&counts(&[(ItemKind::OakLog, 7)]), false),
        (12, 12)
    );
}

/// The bot, the model and the planner, joined.
struct Pair {
    w: World,
    sim: Sim,
    tick: u64,
}

impl Pair {
    fn new(give: &[(ItemKind, i32)]) -> Self {
        Self::on(flat_world(3, 63).unwrap(), give)
    }

    fn on(world: sim::ModelWorld, give: &[(ItemKind, i32)]) -> Self {
        let mut sim = Sim::new(CONFIG, world).seeded(7);
        for g in give {
            assert_eq!(sim.survival.give(*g), 0);
        }
        let mut p = Self {
            w: World::new(),
            sim,
            tick: 0,
        };
        let protocol = i32::try_from(data::VERSION.protocol).unwrap();
        let mut first = Vec::new();
        for c in protocol::session::opening(protocol, "model", 0, "rs-stash") {
            first.extend(p.sim.step(&In::Packet(c)).unwrap());
        }
        p.deliver(first);
        for _ in 0..40 {
            p.step(Vec::new());
        }
        assert!(p.w.body.is_some(), "joined and placed");
        p
    }

    fn deliver(&mut self, mut to_bot: Vec<sim::Wire>) {
        for _ in 0..64 {
            if to_bot.is_empty() {
                return;
            }
            let mut to_model = Vec::new();
            for wire in to_bot.drain(..) {
                if let Ok(p) = decode(wire.state, wire.id, &wire.body) {
                    to_model.extend(self.w.step(&Event::Packet(p)));
                }
            }
            for e in to_model {
                if let Effect::Send(c) = e {
                    to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
                }
            }
        }
    }

    fn step(&mut self, acts: Vec<Act>) {
        let mut to_bot = self.sim.step(&In::Tick).unwrap();
        for a in acts {
            for e in self.w.step(&Event::Act(a)) {
                if let Effect::Send(c) = e {
                    to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
                }
            }
        }
        for e in self.w.step(&Event::Tick {
            now_ms: self.tick * 50,
            seed: self.tick | 1,
        }) {
            if let Effect::Send(c) = e {
                to_bot.extend(self.sim.step(&In::Packet(c)).unwrap());
            }
        }
        self.deliver(to_bot);
        self.tick += 1;
    }

    fn held(&self, k: ItemKind) -> i32 {
        plan::craft::count(&self.w, k)
    }
}

fn planner() -> Plan {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let goals: plan::make::Goals = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let table = plan::make::Table::new(&goals).unwrap();
    Plan::with_table(Aim::Race, Arc::new(table))
}

fn ledger(plan: &Plan) -> String {
    plan.events
        .iter()
        .map(|e| format!("{}: {} ({})", e.tick, e.what, e.why))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs the tasks `next` gives, one after another, until it gives none: each task is stepped by
/// the planner (its own end handling), its acts sent to the model. The task names begun.
fn run(
    p: &mut Pair,
    plan: &mut Plan,
    mut next: impl FnMut(&mut Plan, &World, u64) -> Option<Task>,
) -> Vec<&'static str> {
    let mut begun = Vec::new();
    for _ in 0..4000 {
        if matches!(plan.task, Task::Idle) {
            match next(plan, &p.w, p.tick) {
                Some(t) => {
                    begun.push(t.name());
                    plan.task = t;
                }
                None => return begun,
            }
        }
        plan.step(&p.w);
        let acts = plan.take_acts();
        p.step(acts);
    }
    panic!("the tasks did not end: {begun:?}\n{}", ledger(plan));
}

#[test]
fn before_the_nether_the_home_chest_is_made_and_the_surplus_for_the_leg_stashed_then_fetched() {
    let mut p = Pair::new(&[
        (ItemKind::OakPlanks, 32),
        (ItemKind::CraftingTable, 1),
        (ItemKind::Feather, 16),
        (ItemKind::String, 8),
        (ItemKind::Cobblestone, 64),
    ]);
    let mut plan = planner();
    let leg = nether();
    let begun = run(&mut p, &mut plan, |pl, w, t| pl.stash_before(w, &leg, t));
    let log = ledger(&plan);
    assert!(begun.contains(&"place"), "{begun:?}\n{log}");
    assert!(begun.contains(&"craft"), "{begun:?}\n{log}");
    assert_eq!(begun.last(), Some(&"stash"), "{begun:?}\n{log}");
    let (dim, home) = plan.stash.home.clone().expect("a home chest");
    assert!(log.contains("home chest at"), "{log}");
    assert!(log.contains("stash before the nether at"), "{log}");
    assert!(log.contains("stash moved 16 feather"), "{log}");
    assert!(log.contains("stash done"), "{log}");
    assert_eq!(plan.stash.stashes, 1);
    assert_eq!(plan.stash.legs, vec!["nether".to_owned()]);
    // The bow's feathers and string are in the chest; the leg's 64 stone stays held.
    let model = p.sim.survival.chests.get(&home).expect("the model's chest");
    assert_eq!(model.count(ItemKind::Feather), 16);
    assert_eq!(model.count(ItemKind::String), 8);
    assert_eq!(model.count(ItemKind::Cobblestone), 0);
    assert_eq!(p.held(ItemKind::Feather), 0);
    assert_eq!(p.held(ItemKind::Cobblestone), 64);
    // The table put down for the chest came along again.
    assert_eq!(p.held(ItemKind::CraftingTable), 1, "{log}");
    // The trigger has run for this leg: never again.
    assert!(plan.stash_before(&p.w, &leg, p.tick).is_none());

    // A death and a respawn: what the stages ahead want is fetched back.
    plan.revived(p.tick, "test", 1);
    let begun = run(&mut p, &mut plan, |pl, w, t| {
        match pl.stash_gate(w, Task::Idle, t) {
            Task::Idle => None,
            other => Some(other),
        }
    });
    let log = ledger(&plan);
    assert_eq!(begun, vec!["fetch"], "{log}");
    assert!(log.contains("fetch after the respawn at"), "{log}");
    assert_eq!(plan.stash.fetches, 1, "{log}");
    assert_eq!(p.held(ItemKind::Feather), 16, "{log}");
    assert_eq!(p.held(ItemKind::String), 8, "{log}");
    let seen = p.w.memory.chest(&dim, home).unwrap();
    assert_eq!(seen.count(ItemKind::Feather), Some(0));
    assert!(!plan.stash.fetch_due);
    assert!(p.sim.violations.is_empty(), "{:?}", p.sim.violations);
}

#[test]
fn a_run_without_planks_to_spare_never_begins_a_chest() {
    let mut p = Pair::new(&[
        (ItemKind::OakLog, 4),
        (ItemKind::CraftingTable, 1),
        (ItemKind::Feather, 16),
    ]);
    let mut plan = planner();
    let leg = nether();
    let begun = run(&mut p, &mut plan, |pl, w, t| pl.stash_before(w, &leg, t));
    let log = ledger(&plan);
    assert!(begun.is_empty(), "{begun:?}\n{log}");
    assert!(plan.stash.home.is_none());
    assert!(
        log.contains("stash before the nether skipped: no home chest"),
        "{log}"
    );
    assert!(log.contains("wood to spare 0 planks of 8"), "{log}");
    assert_eq!(plan.stash.legs, vec!["nether".to_owned()]);
    assert_eq!(p.held(ItemKind::Feather), 16);
}

#[test]
fn a_fetch_is_due_only_after_a_respawn_and_says_why_it_ends() {
    let p = Pair::new(&[(ItemKind::Feather, 4)]);
    let mut plan = planner();
    // No respawn: the chosen task passes.
    assert!(matches!(
        plan.stash_gate(&p.w, Task::Idle, p.tick),
        Task::Idle
    ));
    plan.revived(p.tick, "test", 1);
    assert!(plan.stash.fetch_due);
    assert!(matches!(
        plan.stash_gate(&p.w, Task::Idle, p.tick),
        Task::Idle
    ));
    assert!(!plan.stash.fetch_due);
    assert!(ledger(&plan).contains("fetch after the respawn skipped (no home chest)"));
}

/// The development aim `chest-trip` (parity only): the held chest put down, 74 cobblestone and
/// 3 string stashed and fetched back, through the planner's own tasks, each step noted.
#[test]
fn the_chest_trip_aim_places_stashes_and_fetches_back() {
    let mut p = Pair::new(&[
        (ItemKind::Chest, 1),
        (ItemKind::Cobblestone, 64),
        (ItemKind::Cobblestone, 64),
        (ItemKind::Cobblestone, 10),
        (ItemKind::String, 3),
    ]);
    let mut plan = planner();
    plan.aim = Aim::ChestTrip;
    let mut begun = Vec::new();
    for _ in 0..3000 {
        if plan.met {
            break;
        }
        plan.step(&p.w);
        if begun.last() != Some(&plan.task.name()) && !matches!(plan.task, Task::Idle) {
            begun.push(plan.task.name());
        }
        let acts = plan.take_acts();
        p.step(acts);
    }
    let log = ledger(&plan);
    assert!(plan.met, "{begun:?}\n{log}");
    assert_eq!(begun, vec!["place", "stash", "fetch"], "{log}");
    assert!(log.contains("chest trip: place the chest"), "{log}");
    assert!(
        log.contains("chest trip: stash 74 cobblestone, 3 string"),
        "{log}"
    );
    assert!(log.contains("stash moved 10 cobblestone"), "{log}");
    assert!(log.contains("chest trip: fetch"), "{log}");
    assert!(log.contains("chest trip met"), "{log}");
    assert_eq!((plan.stash.stashes, plan.stash.fetches), (1, 1), "{log}");
    assert_eq!(p.held(ItemKind::Cobblestone), 138);
    assert_eq!(p.held(ItemKind::String), 3);
    let (_, home) = plan.stash.home.clone().unwrap();
    let model = p.sim.survival.chests.get(&home).unwrap();
    assert_eq!(model.count(ItemKind::Cobblestone), 0);
    assert!(p.sim.violations.is_empty(), "{:?}", p.sim.violations);
}

/// The home chest is the one the bot put down, found in memory from the block update: a chest
/// buried under the bot (stone round it, grass over it, no face exposed) is never seen, never
/// remembered and never taken for the home chest, though it is the nearest chest there is.
#[test]
fn the_home_chest_is_the_one_put_down_never_a_buried_one() {
    const BURIED: (i32, i32, i32) = (1, 62, 0);
    let mut world = flat_world(3, 63).unwrap();
    let chest = data::BlockKind::Chest.info().unwrap().default_state;
    world
        .set_block(BURIED.0, BURIED.1, BURIED.2, chest)
        .unwrap();
    let mut p = Pair::on(
        world,
        &[
            (ItemKind::OakPlanks, 32),
            (ItemKind::CraftingTable, 1),
            (ItemKind::Feather, 16),
        ],
    );
    let dim = p.w.dimension.as_ref().unwrap().name.clone();
    assert!(p.w.memory.chest(&dim, BURIED).is_none(), "never seen");
    let mut plan = planner();
    let leg = nether();
    let begun = run(&mut p, &mut plan, |pl, w, t| pl.stash_before(w, &leg, t));
    let log = ledger(&plan);
    assert_eq!(begun.last(), Some(&"stash"), "{begun:?}\n{log}");
    let (_, home) = plan.stash.home.clone().expect("a home chest");
    assert_ne!(home, BURIED, "{log}");
    assert!(p.w.memory.chest(&dim, BURIED).is_none(), "still never seen");
    // On the ground, where the place put it: its floor seen under an open cell.
    assert_eq!(home.1, 64, "{log}");
    assert!(p.w.memory.chest(&dim, home).is_some());
    assert_eq!(
        p.sim
            .survival
            .chests
            .get(&home)
            .map(|c| c.count(ItemKind::Feather)),
        Some(16)
    );
}

/// Wall 5: the chest carried to the portal is never dropped from a full pack, and never
/// stashed in itself.
#[test]
fn a_full_pack_keeps_the_carried_chest() {
    use plan::keep::{Context, Rank, least_needed};
    let k = needs();
    let ctx = [Context::Always];
    for reached in [
        None,
        Some(Stage::Ready),
        Some(Stage::Nether),
        Some(Stage::Win),
    ] {
        assert_eq!(
            k.rank(ItemKind::Chest, 1, reached, &ctx),
            Rank::Unlisted,
            "{reached:?}"
        );
        let junk = k.junk(reached, &ctx);
        assert!(junk.iter().all(|(j, _)| *j != ItemKind::Chest));
        // A full pack: the chest, two stacks of stone over the keep, buckets elsewhere.
        let mut slots: BTreeMap<i16, (ItemKind, i32)> =
            (9..=44).map(|s| (s, (ItemKind::Bucket, 1))).collect();
        slots.insert(9, (ItemKind::Chest, 1));
        slots.insert(10, (ItemKind::Cobblestone, 64));
        slots.insert(11, (ItemKind::Cobblestone, 64));
        let rank = |kind: ItemKind, held: i32| k.rank(kind, held, reached, &ctx);
        let dropped = least_needed(&slots, &junk, &rank).map(|(_, kind, _)| kind);
        assert_ne!(dropped, Some(ItemKind::Chest), "{reached:?}");
        // Only the chest and what is never dropped: nothing goes.
        slots.insert(10, (ItemKind::Bucket, 1));
        slots.insert(11, (ItemKind::Bucket, 1));
        assert_eq!(least_needed(&slots, &junk, &rank), None, "{reached:?}");
    }
    assert_eq!(k.need_in(ItemKind::Chest, &nether().stages), 1);
    assert!(
        stash_list(&counts(&[(ItemKind::Chest, 1)]), &k, &nether()).is_empty(),
        "never stashed"
    );
}

/// The early craft's gate as the planner chooses: a task, or none.
fn early(pl: &mut Plan, w: &World, t: u64) -> Option<Task> {
    match pl.stash_gate(w, Task::Idle, t) {
        Task::Idle => None,
        other => Some(other),
    }
}

/// Wall 5: with wood to spare before `ready` the home chest is crafted early and carried; at
/// the portal it is put down (no craft there) and the stash follows.
#[test]
fn the_home_chest_is_crafted_early_and_put_down_at_the_portal() {
    let mut p = Pair::new(&[
        (ItemKind::OakPlanks, 32),
        (ItemKind::CraftingTable, 1),
        (ItemKind::Feather, 16),
    ]);
    let mut plan = planner();
    let begun = run(&mut p, &mut plan, early);
    let log = ledger(&plan);
    assert!(begun.contains(&"craft"), "{begun:?}\n{log}");
    assert!(
        log.contains("home chest crafted early, 16 planks spare"),
        "{log}"
    );
    assert_eq!(p.held(ItemKind::Chest), 1, "{log}");
    // The table put down for it came along again.
    assert_eq!(p.held(ItemKind::CraftingTable), 1, "{log}");
    assert!(plan.stash.home.is_none(), "carried, not put down");
    // Once: the gate is quiet after.
    assert!(early(&mut plan, &p.w, p.tick).is_none());
    // At the portal: the carried chest goes down, no craft, then the stash.
    let leg = nether();
    let begun = run(&mut p, &mut plan, |pl, w, t| pl.stash_before(w, &leg, t));
    let log = ledger(&plan);
    assert_eq!(begun, vec!["place", "stash"], "{log}");
    let (_, home) = plan.stash.home.clone().expect("a home chest");
    assert_eq!(
        p.sim
            .survival
            .chests
            .get(&home)
            .map(|c| c.count(ItemKind::Feather)),
        Some(16),
        "{log}"
    );
    assert_eq!(p.held(ItemKind::Chest), 0);
}

/// Wall 5: with only the leg's logs the early craft never begins, and says nothing.
#[test]
fn the_early_craft_waits_for_wood_to_spare() {
    let mut p = Pair::new(&[(ItemKind::OakLog, 4), (ItemKind::CraftingTable, 1)]);
    let mut plan = planner();
    let begun = run(&mut p, &mut plan, early);
    let log = ledger(&plan);
    assert!(begun.is_empty(), "{begun:?}\n{log}");
    assert!(!log.contains("crafted early"), "{log}");
    assert_eq!(p.held(ItemKind::Chest), 0);
    assert_eq!(p.held(ItemKind::OakLog), 4);
}
