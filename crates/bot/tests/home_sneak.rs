//! The way home's lava edges (docs/problems/eyes-home.md wall 21): a walk along a one-wide bridge
//! over lava holds sneak, and the server's edge back-off (`physics::back_off_from_edge`) keeps the
//! body on the cells though it is turned off the line.

use data::BlockKind;
use plan::path::Goal;
use plan::walk::{Arrive, TaskStatus, Walk};
use sim::{Config, Sim, flat_world};

const CONFIG: Config = Config {
    view_distance: 2,
    simulation_distance: 2,
    peaceful: true,
};

/// A flat Nether (ground at y 63, the feet at y 64) with a lava sea at x 2 to 18 and z -4 to 4,
/// crossed by a one-wide netherrack bridge along z 0.
fn bridge_over_lava() -> Sim {
    let mut m = flat_world(2, 63).unwrap();
    m.dimension = "minecraft:the_nether".to_owned();
    m.dimension_type = 3;
    let lava = BlockKind::Lava.info().unwrap().default_state;
    for x in 2..=18 {
        for z in -4..=4 {
            if z != 0 {
                m.set_block(x, 63, z, lava).unwrap();
            }
        }
    }
    Sim::new(CONFIG, m)
}

struct Out {
    done: bool,
    lowest: f64,
    sneaked: u64,
    said: Vec<String>,
    died: bool,
}

/// The walk to (20, 64, 0) over the bridge; from tick `from` for 30 ticks the body is turned a
/// quarter off the line (as a knock or a bad step would), sneak as the walk says unless `plain`.
fn cross(plain: bool, from: u64) -> Out {
    let mut walk = Walk::new(
        (20, 64, 0),
        Goal::Block { pos: (20, 64, 0) },
        Arrive::Goal,
        0,
    );
    let mut done = false;
    let mut lowest = f64::MAX;
    let mut said = Vec::new();
    let run = bot::lockstep::run_acting(bridge_over_lava(), "rs-hs", 1, 900, 20, |w, t| {
        if let Some(p) = w.player.pos
            && t > 25
        {
            lowest = lowest.min(p[1]);
        }
        if done {
            return (physics::Controls::default(), Vec::new());
        }
        let (mut c, status) = walk.step(w, t);
        done = status == TaskStatus::Done;
        if (from..from + 30).contains(&t) {
            c.yaw += 90.0;
            c.forward = true;
        }
        if plain {
            c.sneak = false;
        }
        let (acts, notes) = walk.take_out();
        said.extend(notes.into_iter().map(|(what, _)| what));
        (c, acts)
    });
    Out {
        done,
        lowest,
        sneaked: walk.more.sneak_ticks,
        said,
        died: run.died,
    }
}

#[test]
fn a_walk_along_a_bridge_over_lava_sneaks_and_never_leaves_it() {
    // Turned off the line at about x 6 (the walk's 40th tick or so), held 30 ticks.
    let out = cross(false, 60);
    assert!(out.sneaked > 0, "no sneak over the lava");
    assert!(
        out.said
            .iter()
            .any(|s| s.starts_with("walk: sneak at a lava edge")),
        "{:?}",
        out.said
    );
    assert!(out.lowest > 63.9, "fell to y {}", out.lowest);
    assert!(!out.died);
    assert!(out.done, "the walk did not arrive");
}

#[test]
fn the_same_turn_without_sneak_falls_into_the_lava() {
    // The control: the push the sneak holds is a fall without it.
    let out = cross(true, 60);
    assert!(out.lowest < 63.5, "lowest {}", out.lowest);
}
