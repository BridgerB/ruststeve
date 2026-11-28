//! Issue 20h, the synthetic End (docs/problems/end-model.md): what `sim::end` lays, checked
//! cell by cell against the features it copies (`EndPlatformFeature`, `SpikeFeature`).

use data::BlockKind;
use sim::end::{ARRIVAL, EndStage, Layout, PLATFORM, THE_END, cage};
use sim::{Config, Sim};

const CONFIG: Config = Config {
    view_distance: 4,
    simulation_distance: 3,
    peaceful: false,
};

fn kind(sim: &Sim, x: i32, y: i32, z: i32) -> Option<BlockKind> {
    sim.state_at(x, y, z).and_then(data::block_of_state)
}

#[test]
fn the_small_end_holds_the_platform_the_gap_the_island_and_two_spikes() {
    let layout = Layout::small();
    let world = EndStage::of(&layout).build().unwrap();
    assert_eq!(world.dimension, THE_END);
    assert_eq!(world.spawn, ARRIVAL);
    let sim = Sim::new(CONFIG, world);
    let (px, py, pz) = PLATFORM;
    assert_eq!(kind(&sim, px, py, pz), Some(BlockKind::Obsidian));
    assert_eq!(kind(&sim, px + 2, py, pz - 2), Some(BlockKind::Obsidian));
    assert_eq!(kind(&sim, px, py + 1, pz), Some(BlockKind::Air));
    // The void between the island's edge (x 78) and the platform's (x 98).
    for x in 79..98 {
        assert_eq!(kind(&sim, x, 60, 0), Some(BlockKind::Air), "x {x}");
        assert_eq!(kind(&sim, x, 48, 0), Some(BlockKind::Air), "x {x}");
    }
    assert_eq!(kind(&sim, 78, 60, 0), Some(BlockKind::EndStone));
    // The fountain's pillar and floor.
    assert_eq!(kind(&sim, 0, 64, 0), Some(BlockKind::Bedrock));
    assert_eq!(kind(&sim, 2, 60, 0), Some(BlockKind::Bedrock));
    // The open spike: obsidian under its height within the radius, bedrock on top.
    let open = layout.spikes[0];
    let [cx, cz] = open.centre;
    assert_eq!(kind(&sim, cx, open.height, cz), Some(BlockKind::Bedrock));
    assert_eq!(
        kind(&sim, cx + 3, open.height - 1, cz),
        Some(BlockKind::Obsidian)
    );
    assert_eq!(
        kind(&sim, cx + 3, open.height - 1, cz + 1),
        Some(BlockKind::Obsidian)
    );
    assert_eq!(
        kind(&sim, cx + 3, open.height - 1, cz + 2),
        Some(BlockKind::Air)
    );
    assert_eq!(kind(&sim, cx + 1, open.height, cz), Some(BlockKind::Air));
    // The caged spike: bars on the edges two out, the roof three up.
    let caged = layout.spikes[1];
    let [gx, gz] = caged.centre;
    assert_eq!(
        kind(&sim, gx + 2, caged.height + 1, gz),
        Some(BlockKind::IronBars)
    );
    assert_eq!(
        kind(&sim, gx, caged.height + 3, gz),
        Some(BlockKind::IronBars)
    );
    assert_eq!(
        kind(&sim, gx + 1, caged.height + 1, gz),
        Some(BlockKind::Air)
    );
    assert_eq!(cage(&caged).len(), 16 * 3 + 25);
}

#[test]
fn a_cage_bar_on_an_x_edge_runs_north_south_as_worldgen_joins_it() {
    let sp = Layout::small().spikes[1];
    let bars = cage(&sp);
    let (_, st) = bars
        .iter()
        .find(|((x, y, z), _)| *x == sp.centre[0] + 2 && *y == sp.height + 1 && *z == sp.centre[1])
        .copied()
        .unwrap();
    let p = |k| sim::survival::property(st, k);
    assert_eq!(
        (p("north"), p("south"), p("east"), p("west")),
        (Some("true"), Some("true"), Some("false"), Some("false"))
    );
}
