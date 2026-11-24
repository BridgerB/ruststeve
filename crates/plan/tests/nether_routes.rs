//! Issue 16, the Nether walk (docs/problems/nether-testbed.md): the pathfinder alone on the
//! Nether walls of `sim::nether::walls`, with no bot and no ticks: which wall has a route, which
//! has none, and what the search says of it. The walls with no route today are `#[ignore]`d with
//! the reason; `crates/bot/tests/nether_walls.rs` crosses the same stages closed loop.
//!
//! Run the ignored ones with `cargo test -p plan --test nether_routes -- --ignored --nocapture`.

use data::BlockKind;
use plan::path::{Budget, Cell, Cells, Goal, Route, Status, plan};
use protocol::S2c;
use sim::nether::{NETHER, Wall, nether_type, walls};
use world::{BlockPos, Event, World};

/// The bot's world of a wall: every column loaded through the packets a server sends, the
/// player standing at the wall's start.
fn world_of(wall: &Wall) -> World {
    let model = wall.world().unwrap();
    let mut w = World::new();
    w.step(&Event::Packet(S2c::Login {
        entity_id: 1,
        view_distance: 8,
        simulation_distance: 3,
        dimension_type: nether_type(),
        dimension: NETHER.to_owned(),
        game_type: 0,
    }));
    for ((x, z), c) in &model.columns {
        w.step(&Event::Packet(S2c::LevelChunkWithLight {
            x: *x,
            z: *z,
            column: c.column.clone(),
        }));
    }
    let [x, y, z] = model.spawn;
    w.step(&Event::Packet(S2c::PlayerPosition {
        teleport_id: 1,
        x,
        y,
        z,
        yaw: 0.0,
        pitch: 0.0,
        flags: 0,
    }));
    w
}

/// The route the fortress walk asks for (`fortress_task`): within reach of the nearest bricks,
/// on the far budget.
fn to_bricks(wall: &Wall, w: &World) -> Route {
    let goal = Goal::Near {
        pos: wall.bricks,
        range: world::stage::FORTRESS_NEAR - 1.0,
    };
    plan(w, wall.start, &goal, Budget::Far.expansions())
}

/// What a failed search has to say: its status and spend, and what the start and the cells
/// round it are to the pathfinder.
fn said(wall: &Wall, w: &World, r: &Route) -> String {
    let mut cells = Cells::new(w);
    let (x, y, z) = wall.start;
    let under = w.block_at(BlockPos::new(x, y - 1, z));
    let near: Vec<String> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .map(|(dx, dz)| {
            let c = (x + dx, y, z + dz);
            format!("{c:?} standable {}", cells.standable(c))
        })
        .collect();
    format!(
        "{}: {:?} after {} expansions, {} waypoints, from {:?}, last {:?}; the start {:?} stands on \
         {under:?}, standable {}, clear {}; {}",
        wall.name,
        r.status,
        r.expanded,
        r.waypoints.len(),
        r.from,
        r.waypoints.last(),
        wall.start,
        cells.standable(wall.start),
        cells.clear(wall.start),
        near.join(", ")
    )
}

fn lava(w: &World, (x, y, z): Cell) -> bool {
    w.block_at(BlockPos::new(x, y, z)) == Some(BlockKind::Lava)
}

/// The control: a netherrack flat has a route, in the Nether as anywhere.
#[test]
fn a_netherrack_flat_has_a_route() {
    let wall = walls::netherrack_flat();
    let w = world_of(&wall);
    let r = to_bricks(&wall, &w);
    assert_eq!(r.status, Status::Found, "{}", said(&wall, &w, &r));
}

/// Whatever comes back on any wall, found or partial, no waypoint has the body in lava: the
/// feet's cell and the head's are never lava.
#[test]
fn no_route_on_any_wall_puts_the_body_in_lava() {
    for wall in walls::all() {
        let w = world_of(&wall);
        let r = to_bricks(&wall, &w);
        let wet = r
            .waypoints
            .iter()
            .find(|c| lava(&w, **c) || lava(&w, (c.0, c.1 + 1, c.2)));
        assert_eq!(wet, None, "{}", said(&wall, &w, &r));
    }
}

/// (a) Soul sand is ground: the flat has a route as the netherrack one does.
#[test]
fn a_soul_sand_flat_has_a_route() {
    let wall = walls::soul_sand_flat();
    let w = world_of(&wall);
    let r = to_bricks(&wall, &w);
    assert_eq!(r.status, Status::Found, "{}", said(&wall, &w, &r));
}

/// (b) A moat 4 wide has a route for a bot that holds blocks (the bridging move).
#[test]
fn a_lava_moat_has_a_route() {
    let wall = walls::lava_moat();
    let mut w = world_of(&wall);
    w.inventory
        .slots
        .insert(36, (data::ItemKind::Cobblestone, 64));
    // The far search is the one that bridges (`fortress_task`'s far walk).
    let goal = Goal::Near {
        pos: wall.bricks,
        range: world::stage::FORTRESS_NEAR - 1.0,
    };
    let r = plan::path::plan_far(&w, wall.start, &goal, Budget::Far.expansions(), 1.0);
    assert_eq!(r.status, Status::Found, "{}", said(&wall, &w, &r));
}

/// (c) and (d) are the tunnel's: no walk crosses 49 blocks of lava or solid rock, and none
/// should for a bot that holds no blocks (this one holds nothing). What the pathfinder owes
/// them is an honest no: it spends its budget or its frontier and says so, with no route that
/// claims to arrive.
#[test]
fn the_lake_and_the_stair_cut_have_no_walk_and_say_so() {
    for wall in [walls::lake_under_a_hill(), walls::stair_cut()] {
        let w = world_of(&wall);
        let r = to_bricks(&wall, &w);
        assert_ne!(r.status, Status::Found, "{}", said(&wall, &w, &r));
    }
}
