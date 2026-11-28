//! A fleet guard for strict no X-ray (docs/problems/iron-search.md, the near miss): against the
//! model, from the `stone` kit with the iron kit as the aim, on a flat world whose rock holds
//! sealed caves and sealed iron ore (no face of theirs open, no line to them from the bot) beside
//! iron ore exposed on the surface. The whole planner runs (`Plan::step`, closed loop): the walk
//! to exposed ore, stairs down to the iron rows' depth, the iron search's strips, caves and rises,
//! the explore. Every tick, every target the planner holds (a walk's goal, a dig's cell, the dig
//! toward's aim, the iron search's cave) is checked: none may lie in a sealed feature the bot has
//! not opened. A feature counts as opened once a cell of its shell, or the ore's own neighbour, is
//! no longer the stone it was built in (the bot cut into it and saw it).

use data::{BlockKind, ItemKind};
use plan::path::Cell;
use plan::{Aim, Plan, Task};
use sim::{Config, Sim, flat_world};
use world::{BlockPos, World};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// A sealed feature: its open (or ore) cells, inclusive box corners.
#[derive(Clone, Copy, Debug)]
struct Sealed {
    lo: Cell,
    hi: Cell,
}

impl Sealed {
    fn holds(&self, c: Cell) -> bool {
        (self.lo.0..=self.hi.0).contains(&c.0)
            && (self.lo.1..=self.hi.1).contains(&c.1)
            && (self.lo.2..=self.hi.2).contains(&c.2)
    }

    /// A target within a block of it counts as aimed at it (a walk's stand cell beside ore).
    fn near(&self, c: Cell) -> bool {
        (self.lo.0 - 1..=self.hi.0 + 1).contains(&c.0)
            && (self.lo.1 - 1..=self.hi.1 + 1).contains(&c.1)
            && (self.lo.2 - 1..=self.hi.2 + 1).contains(&c.2)
    }

    /// Opened: a cell of the shell round it is no longer stone (or ore, for the ore's shell).
    fn opened(&self, w: &World) -> bool {
        let rock = |k: Option<BlockKind>| matches!(k, Some(BlockKind::Stone) | None);
        (self.lo.0 - 1..=self.hi.0 + 1).any(|x| {
            (self.lo.1 - 1..=self.hi.1 + 1).any(|y| {
                (self.lo.2 - 1..=self.hi.2 + 1).any(|z| {
                    let c = (x, y, z);
                    !self.holds(c) && !rock(w.block_at(BlockPos::new(x, y, z)))
                })
            })
        })
    }
}

/// The cells the planner aims at now.
fn targets(p: &Plan) -> Vec<(&'static str, Cell)> {
    let mut out = Vec::new();
    match &p.task {
        Task::ToLog(w) => out.push(("walk-to-log", w.target)),
        Task::Explore(w) => out.push(("explore", w.target)),
        Task::Dig(d) => out.push(("dig", d.target)),
        _ => {}
    }
    if let Some((c, _, _)) = p.toward {
        out.push(("dig toward", c));
    }
    if let Some((c, _)) = p.iron.cave_aim {
        out.push(("cave aim", c));
    }
    out
}

#[test]
fn no_target_in_rock_never_seen() {
    let mut m = flat_world(2, 63).unwrap();
    let st = |k: BlockKind| k.info().unwrap().default_state;
    // Three iron ore exposed on the surface: enough to start, not the kit, so the bot goes down.
    for x in 4..7 {
        m.set_block(x, 64, -3, st(BlockKind::IronOre)).unwrap();
    }
    let mut sealed: Vec<Sealed> = Vec::new();
    // Sealed caves, 5 by 5 by 3, at the iron rows' depth on every side, 20 to 28 off.
    for (cx, cz) in [(0, 26), (0, -26), (26, 0), (-26, 0), (20, 20), (-20, -20)] {
        let (lo, hi) = ((cx - 2, 14, cz - 2), (cx + 2, 16, cz + 2));
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    m.set_block(x, y, z, st(BlockKind::Air)).unwrap();
                }
            }
        }
        sealed.push(Sealed { lo, hi });
    }
    // Sealed iron ore, single blocks in the rock round the descent.
    for (x, y, z) in [
        (6, 14, 6),
        (-6, 14, 6),
        (6, 14, -6),
        (-6, 14, -6),
        (10, 18, 0),
        (0, 18, 10),
        (-10, 18, 0),
        (0, 18, -10),
    ] {
        m.set_block(x, y, z, st(BlockKind::IronOre)).unwrap();
        sealed.push(Sealed {
            lo: (x, y, z),
            hi: (x, y, z),
        });
    }
    m.spawn = [0.5, 64.0, 0.5];
    let goals = concat!(env!("CARGO_MANIFEST_DIR"), "/../../config/goals.toml");
    let table = std::sync::Arc::new(bot::goals_table(goals).unwrap());
    let mut p = Plan::with_table(Aim::IronKit, table);
    let mut sim = Sim::new(CONFIG, m).seeded(7);
    for g in [
        (ItemKind::StonePickaxe, 1),
        (ItemKind::StoneSword, 1),
        (ItemKind::WoodenPickaxe, 1),
        (ItemKind::CraftingTable, 1),
        (ItemKind::OakLog, 3),
        (ItemKind::Stick, 3),
        (ItemKind::Cobblestone, 4),
    ] {
        sim.survival.give(g);
    }
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0u64;
    let mut lowest = i32::MAX;
    let run = bot::lockstep::run_acting(sim, "rs-noxray", 1, 20000, 20, |w, tick| {
        let c = p.step(w);
        if let Some(b) = w.body {
            lowest = lowest.min(plan::path::feet_cell(b.pos).1);
        }
        for (what, t) in targets(&p) {
            checked += 1;
            if let Some(s) = sealed.iter().find(|s| s.near(t) && !s.opened(w))
                && bad.len() < 20
            {
                bad.push(format!("tick {tick}: {what} at {t:?}, in sealed {s:?}"));
            }
        }
        (c, p.take_acts())
    });
    let fired: Vec<String> = p
        .events
        .iter()
        .filter(|e| {
            [
                "strip ",
                "rise toward",
                "walk into the cave",
                "dig toward a cave",
                "no fresh rock",
            ]
            .iter()
            .any(|k| e.what.starts_with(k))
        })
        .map(|e| format!("{} {}", e.tick, e.what))
        .collect();
    assert!(
        bad.is_empty(),
        "targets in rock never seen: {bad:#?}\nlast events {:#?}",
        p.events.iter().rev().take(20).collect::<Vec<_>>()
    );
    // The guard means something only if the bot went underground and searched.
    assert!(checked > 0);
    assert!(
        lowest <= 20,
        "the bot never went down (lowest feet y {lowest}); last events {:#?}",
        p.events.iter().rev().take(20).collect::<Vec<_>>()
    );
    eprintln!(
        "lowest feet y {lowest}; iron search counts {:?}; {} search notes, first: {:#?}",
        p.iron.counts,
        fired.len(),
        fired.iter().take(10).collect::<Vec<_>>()
    );
    assert!(!run.died);
}
