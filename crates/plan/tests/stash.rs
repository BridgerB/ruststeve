//! Issue 77 (brain/ruststeve/design/chests.md): the chest tasks against the server model, the
//! bot's world and the model folded together tick by tick. A chest placed, a stash, then a
//! fetch: the counts go in and come back, checked on both sides and in memory.

use data::ItemKind;
use plan::stash::{ChestStatus, Fetch, Stash};
use protocol::decode;
use sim::{Config, In, Sim, flat_world};
use world::{Act, BlockPos, Effect, Event, World};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// The bot and the model, joined, and a tick count.
struct Pair {
    w: World,
    sim: Sim,
    tick: u64,
}

impl Pair {
    fn new(give: &[(ItemKind, i32)]) -> Self {
        let mut sim = Sim::new(CONFIG, flat_world(2, 63).unwrap()).seeded(5);
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
        for c in protocol::session::opening(protocol, "model", 0, "rs-chest") {
            first.extend(p.sim.step(&In::Packet(c)).unwrap());
        }
        p.deliver(first);
        for _ in 0..40 {
            p.step(None);
        }
        assert!(p.w.body.is_some(), "joined and placed");
        p
    }

    /// The model's packets to the bot and the bot's answers back, until both are quiet.
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

    /// One server tick, with the bot's act (if any) sent first.
    fn step(&mut self, act: Option<Act>) {
        let mut to_bot = self.sim.step(&In::Tick).unwrap();
        if let Some(a) = act {
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

    fn dim(&self) -> String {
        self.w.dimension.as_ref().unwrap().name.clone()
    }
}

fn run(p: &mut Pair, mut task: impl FnMut(&World, u64) -> (Option<Act>, ChestStatus)) {
    for _ in 0..600 {
        let (act, status) = task(&p.w, p.tick);
        p.step(act);
        if status != ChestStatus::Running {
            assert_eq!(status, ChestStatus::Done);
            // The close act went with the last step.
            assert!(p.w.window.is_none(), "closed");
            return;
        }
    }
    panic!("the task did not end");
}

const CHEST: (i32, i32, i32) = (2, 64, 0);

#[test]
fn a_stash_then_a_fetch_round_trips_the_counts() {
    let mut p = Pair::new(&[
        (ItemKind::Chest, 1),
        (ItemKind::Cobblestone, 64),
        (ItemKind::Cobblestone, 64),
        (ItemKind::Cobblestone, 10),
        (ItemKind::String, 3),
    ]);
    let dim = p.dim();
    // Not yet placed: no chest known, so no task (no X-ray).
    assert!(Stash::new(&p.w, CHEST, &[(ItemKind::Cobblestone, 1)], p.tick).is_none());
    // The chest placed on the grass beside the bot: memory has it from the block update.
    p.step(Some(Act::UseOn {
        pos: BlockPos::new(2, 63, 0),
        face: 1,
    }));
    p.step(None);
    let seen =
        p.w.memory
            .chest(&dim, CHEST)
            .expect("the placed chest is known");
    assert_eq!(seen.contents, None, "never opened");

    // Stash 74 cobblestone and the string: the 64 and the 10, never the other 64.
    let mut stash = Stash::new(
        &p.w,
        CHEST,
        &[(ItemKind::Cobblestone, 74), (ItemKind::String, 3)],
        p.tick,
    )
    .unwrap();
    run(&mut p, |w, t| {
        let (_, act, status) = stash.step(w, t);
        (act, status)
    });
    assert_eq!(stash.0.moved(ItemKind::Cobblestone), 74);
    assert_eq!(stash.0.moved(ItemKind::String), 3);
    let held = plan::craft::count;
    assert_eq!(held(&p.w, ItemKind::Cobblestone), 64);
    assert_eq!(held(&p.w, ItemKind::String), 0);
    let model = p.sim.survival.chests.get(&CHEST).unwrap();
    assert_eq!(model.count(ItemKind::Cobblestone), 74);
    assert_eq!(model.count(ItemKind::String), 3);
    let seen = p.w.memory.chest(&dim, CHEST).unwrap();
    assert_eq!(seen.count(ItemKind::Cobblestone), Some(74));
    assert_eq!(seen.count(ItemKind::String), Some(3));
    let notes = stash.0.notes.join("\n");
    assert!(notes.contains("chest opened at (2, 64, 0)"), "{notes}");
    assert!(notes.contains("stash moved 64 cobblestone"), "{notes}");
    assert!(notes.contains("stash moved 10 cobblestone"), "{notes}");
    assert!(notes.contains("stash moved 3 string"), "{notes}");
    assert!(notes.contains("chest closed at (2, 64, 0)"), "{notes}");

    // Fetch them back: the chest empties and the pack is as it began.
    let mut fetch = Fetch::new(
        &p.w,
        CHEST,
        &[(ItemKind::Cobblestone, 74), (ItemKind::String, 3)],
        p.tick,
    )
    .unwrap();
    run(&mut p, |w, t| {
        let (_, act, status) = fetch.step(w, t);
        (act, status)
    });
    assert_eq!(fetch.0.moved(ItemKind::Cobblestone), 74);
    assert_eq!(fetch.0.moved(ItemKind::String), 3);
    assert_eq!(held(&p.w, ItemKind::Cobblestone), 138);
    assert_eq!(held(&p.w, ItemKind::String), 3);
    let model = p.sim.survival.chests.get(&CHEST).unwrap();
    assert_eq!(model.count(ItemKind::Cobblestone), 0);
    assert_eq!(model.count(ItemKind::String), 0);
    let seen = p.w.memory.chest(&dim, CHEST).unwrap();
    assert_eq!(seen.contents, Some(Default::default()), "seen empty");
    assert!(p.sim.violations.is_empty(), "{:?}", p.sim.violations);
}

/// A stash asked for more than a chest of 27 slots takes stops at the click that moves nothing,
/// and says so.
#[test]
fn a_full_chest_ends_the_stash_and_says_so() {
    let mut give = vec![(ItemKind::Chest, 1)];
    give.extend(std::iter::repeat_n((ItemKind::Cobblestone, 64), 30));
    let mut p = Pair::new(&give);
    p.step(Some(Act::UseOn {
        pos: BlockPos::new(2, 63, 0),
        face: 1,
    }));
    p.step(None);
    let mut stash = Stash::new(&p.w, CHEST, &[(ItemKind::Cobblestone, 64 * 30)], p.tick).unwrap();
    run(&mut p, |w, t| {
        let (_, act, status) = stash.step(w, t);
        (act, status)
    });
    assert_eq!(stash.0.moved(ItemKind::Cobblestone), 64 * 27);
    assert!(
        stash
            .0
            .notes
            .iter()
            .any(|n| n.contains("the chest is full")),
        "{:?}",
        stash.0.notes
    );
}
