//! Issue 20h, the crystal strike in the closed loop (docs/problems/end-model.md): the bot's own
//! world fed by the model of the synthetic End, standing on an end stone block 3 off a spike's
//! crystal, striking it by reach (`Script::Crystal`, parity row e03's script). With end stone at
//! 2 off at the feet and the eye the blast is hidden; with none it is not. Development evidence
//! only (measurement-rules): parity rows e03 say whether the model agrees with the server.

use data::{BlockKind, EntityKind, ItemKind};
use sim::end::{EndStage, Layout};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: false,
};
/// Full iron with boots (2 + 6 + 5 + 2), the kit 20c makes.
const IRON: f32 = 15.0;

/// The small End's open spike, the bot's stand 3 east of its crystal's column at the crystal's
/// height, and `wall` end stone at 2 off at the feet and the eye.
fn strike(wall: bool) -> bot::lockstep::Run {
    let layout = Layout::small();
    let sp = layout.spikes[0];
    let [cx, cz] = sp.centre;
    let feet = sp.height + 1;
    let mut stage = EndStage::of(&layout);
    stage.set((cx + 3, feet - 1, cz), BlockKind::EndStone);
    if wall {
        stage.fill(
            (cx + 2, cx + 2),
            (feet, feet + 1),
            (cz, cz),
            BlockKind::EndStone,
        );
    }
    stage.spawn((cx + 3, feet, cz));
    let mut sim = Sim::new(CONFIG, stage.build().unwrap()).seeded(1);
    sim.survival.give((ItemKind::IronSword, 1));
    sim.survival.life.armor = IRON;
    sim.summon_crystal(sp.crystal());
    let script = bot::Script::Crystal { every: 14 };
    bot::lockstep::run_acting(sim, "rs-strike", 1, 20 + 60, 20, |w, t| {
        (script.steer(t, w), script.acts(t, w))
    })
}

fn crystals(w: &world::World) -> usize {
    w.entities
        .values()
        .filter(|e| !e.dead && e.kind == EntityKind::EndCrystal)
        .count()
}

#[test]
fn a_crystal_struck_through_the_wall_hides_the_blast() {
    let run = strike(true);
    assert_eq!(run.sim.survival.life.crystals_struck, 1);
    assert_eq!(crystals(&run.world), 0);
    let blasts = &run.sim.survival.life.blasts;
    assert_eq!(blasts.len(), 1);
    assert_eq!(blasts[0].1, 0.0, "exposure {:?}", blasts[0]);
    // A hidden player within 12 still takes the formula's 1, cut by the armour.
    let h = run.min_health_in_script.unwrap_or(20.0);
    assert!(h > 19.0 && h < 20.0, "health {h}");
    assert!(!run.died);
}

#[test]
fn a_crystal_struck_from_an_open_stand_kills_through_iron() {
    let run = strike(false);
    assert_eq!(run.sim.survival.life.crystals_struck, 1);
    let blasts = &run.sim.survival.life.blasts;
    assert_eq!(blasts.len(), 1);
    assert!(blasts[0].1 > 0.9, "exposure {:?}", blasts[0]);
    assert!(run.died, "health {:?}", run.min_health_in_script);
}
