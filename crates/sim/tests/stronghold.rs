//! Issue 19g: the synthetic stronghold (crates/sim/src/stronghold.rs): the portal room laid as
//! the 26.1.2 template has it, buried in flat rock, with a corridor from its door.

use data::BlockKind;
use sim::stronghold::stronghold_world;

const GROUND: i32 = 64;
const ORIGIN: (i32, i32, i32) = (20, 20, 30);

fn kind(world: &sim::ModelWorld, (x, y, z): (i32, i32, i32)) -> Option<BlockKind> {
    world
        .columns
        .get(&(x >> 4, z >> 4))?
        .column
        .section_at(y)?
        .blocks
        .get(protocol::chunk::block_index(x, y, z))
        .and_then(data::block_of_state)
}

#[test]
fn the_room_has_its_ring_pool_spawner_and_an_air_face_on_every_frame() {
    let (world, room) = stronghold_world(5, GROUND, ORIGIN, 12).unwrap();
    assert_eq!(room.frames.len(), 12);
    assert_eq!(room.centre, (25, 23, 40));
    assert_eq!(room.floor_y, 21);
    for f in &room.frames {
        assert_eq!(kind(&world, *f), Some(BlockKind::EndPortalFrame), "{f:?}");
        let (x, y, z) = *f;
        let open = [
            (x, y - 1, z),
            (x, y + 1, z),
            (x, y, z - 1),
            (x, y, z + 1),
            (x - 1, y, z),
            (x + 1, y, z),
        ]
        .iter()
        .any(|n| matches!(kind(&world, *n), Some(BlockKind::Air)));
        assert!(
            open,
            "frame {f:?} has no air face: memory would never see it"
        );
    }
    // The pool under the ring, two under the frames.
    for x in 24..=26 {
        for z in 39..=41 {
            assert_eq!(kind(&world, (x, 21, z)), Some(BlockKind::Lava));
            assert_eq!(kind(&world, (x, 22, z)), Some(BlockKind::Air));
        }
    }
    assert_eq!(kind(&world, room.spawner), Some(BlockKind::Spawner));
    assert_eq!(room.lava.len(), 9 + 3 + 3);
    assert!(!room.infested.is_empty());
    for p in &room.infested {
        assert_eq!(kind(&world, *p), Some(BlockKind::InfestedStoneBricks));
    }
    // The door is open at the feet and the head; the corridor runs north from it.
    assert_eq!(kind(&world, room.door), Some(BlockKind::Air));
    assert_eq!(kind(&world, (25, 21, 25)), Some(BlockKind::Air));
    // The rock round the room is the world's stone; the surface is untouched.
    assert_eq!(kind(&world, (10, 21, 40)), Some(BlockKind::Stone));
    assert_eq!(kind(&world, (25, GROUND, 40)), Some(BlockKind::GrassBlock));
}
