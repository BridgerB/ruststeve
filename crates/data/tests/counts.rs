//! The generated tables match clojurecraft's, count for count (the oracle, at be85a92, whose
//! tables `bb datagen` generated from the same 26.1.2 server). Each expected number names the
//! clojurecraft file it was counted from.

use data::generated::blocks::{BLOCKS, STATE_COUNT};
use data::generated::packets::{
    ConfigurationClientbound, ConfigurationServerbound, HandshakeServerbound, LoginClientbound,
    LoginServerbound, PlayClientbound, PlayServerbound, StatusClientbound, StatusServerbound,
};
use data::generated::recipes::{RECIPES, SMELTING};
use data::{BlockKind, EntityKind, ItemKind, VERSION, block_of_state};

#[test]
fn version_is_26_1_2_protocol_775() {
    // resources/clojurecraft/version.edn
    assert_eq!(VERSION.id, "26.1.2");
    assert_eq!(VERSION.protocol, 775);
    assert_eq!(VERSION.world, 4790);
    assert_eq!(VERSION.java, 25);
}

#[test]
fn packet_counts_match() {
    // resources/clojurecraft/packets.edn
    assert_eq!(HandshakeServerbound::ALL.len(), 1);
    assert_eq!(StatusClientbound::ALL.len(), 2);
    assert_eq!(StatusServerbound::ALL.len(), 2);
    assert_eq!(LoginClientbound::ALL.len(), 6);
    assert_eq!(LoginServerbound::ALL.len(), 5);
    assert_eq!(ConfigurationClientbound::ALL.len(), 20);
    assert_eq!(ConfigurationServerbound::ALL.len(), 10);
    assert_eq!(PlayClientbound::ALL.len(), 141);
    assert_eq!(PlayServerbound::ALL.len(), 69);
}

#[test]
fn attack_is_play_serverbound_1() {
    // Protocol 775 split attack out of interact (brain/wire/wire-actions.md).
    assert_eq!(PlayServerbound::from_id(1), Some(PlayServerbound::Attack));
    assert_eq!(PlayServerbound::Attack.name(), "attack");
}

#[test]
fn registry_counts_match() {
    // resources/clojurecraft/blocks.edn, items.edn, entity-types.edn
    assert_eq!(BlockKind::COUNT, 1168);
    assert_eq!(BLOCKS.len(), 1168);
    assert_eq!(ItemKind::COUNT, 1506);
    assert_eq!(EntityKind::COUNT, 157);
}

#[test]
fn state_ids_run_0_to_29872() {
    // resources/clojurecraft/blocks.edn: the last row's max state is 29872.
    assert_eq!(STATE_COUNT, 29873);
    assert_eq!(block_of_state(0), Some(BlockKind::Air));
    assert_eq!(block_of_state(STATE_COUNT), None);
    for (i, b) in BLOCKS.iter().enumerate() {
        let kind = BlockKind::from_id(u32::try_from(i).unwrap());
        assert_eq!(block_of_state(b.first_state), Some(kind), "{}", b.name);
        assert_eq!(block_of_state(b.last_state), Some(kind), "{}", b.name);
        assert_eq!(kind.name(), Some(b.name));
    }
}

#[test]
fn recipe_counts_match() {
    // resources/clojurecraft/recipes.edn (shaped and shapeless only), smelting.edn
    assert_eq!(RECIPES.len(), 1030);
    assert_eq!(SMELTING.len(), 73);
}

/// The blocks that change a body's movement, as the game's classes hold them (`Blocks`:
/// soul sand `speedFactor(0.4)`, honey `speedFactor(0.4).jumpFactor(0.5)`, the ices and slime
/// by `friction`), and nothing else.
#[test]
fn soul_sand_and_honey_slow_a_body_and_nothing_else_does() {
    use data::BlockKind;
    use data::generated::movement::{MOVEMENT, movement};
    use data::types::Movement;
    let soul = movement(BlockKind::SoulSand);
    assert_eq!(
        (soul.speed_factor, soul.jump_factor, soul.friction),
        (0.4, 1.0, 0.6)
    );
    let honey = movement(BlockKind::HoneyBlock);
    assert_eq!((honey.speed_factor, honey.jump_factor), (0.4, 0.5));
    assert_eq!(movement(BlockKind::Ice).friction, 0.98);
    assert_eq!(movement(BlockKind::BlueIce).friction, 0.989);
    assert_eq!(movement(BlockKind::SlimeBlock).friction, 0.8);
    assert_eq!(movement(BlockKind::Netherrack), Movement::DEFAULT);
    assert_eq!(movement(BlockKind::SoulSoil), Movement::DEFAULT);
    assert_eq!(movement(BlockKind::MagmaBlock), Movement::DEFAULT);
    let slowing: Vec<BlockKind> = MOVEMENT
        .iter()
        .filter(|(_, m)| m.speed_factor != 1.0)
        .map(|(b, _)| *b)
        .collect();
    assert_eq!(slowing, [BlockKind::SoulSand, BlockKind::HoneyBlock]);
}

#[test]
fn names_round_trip() {
    assert_eq!(ItemKind::from_name("oak_log"), Some(ItemKind::OakLog));
    assert_eq!(ItemKind::OakLog.name(), Some("oak_log"));
    assert_eq!(ItemKind::from_id(ItemKind::OakLog.id()), ItemKind::OakLog);
    assert_eq!(ItemKind::from_id(1_000_000), ItemKind::Unknown(1_000_000));
    assert_eq!(ItemKind::Unknown(3).name(), None);
    assert_eq!(EntityKind::from_name("blaze"), Some(EntityKind::Blaze));
}
