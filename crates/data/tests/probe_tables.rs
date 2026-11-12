//! Spot checks of the tables the JNI probe reads from the game's classes. The full tables are
//! compared with clojurecraft's, entry by entry, when datagen runs (`--oracle`); these pin the
//! values the bot leans on first. Expected values: clojurecraft's resources/clojurecraft/*.edn
//! at be85a92 and brain/wire/wire-entity-metadata.md.

use data::generated::fuel::fuel_ticks;
use data::generated::hardness::{hardness, needs_correct_tool};
use data::generated::metadata::ENTITY_DATA_SERIALIZERS;
use data::generated::mobs::{MobCategory, mob};
use data::generated::shapes::collision;
use data::generated::tools::TOOL_MATERIALS;
use data::types::Collision;
use data::{BlockKind, EntityKind, ItemKind};

#[test]
fn hardness_matches_the_game() {
    // hardness.edn
    assert_eq!(hardness(BlockKind::Stone), Some(1.5));
    assert_eq!(hardness(BlockKind::Obsidian), Some(50.0));
    assert_eq!(hardness(BlockKind::Bedrock), Some(-1.0));
    assert_eq!(hardness(BlockKind::OakLog), Some(2.0));
    assert_eq!(hardness(BlockKind::Unknown(99_999)), None);
}

#[test]
fn needs_correct_tool_matches_the_game() {
    // needs-tool.edn: stone dug by hand drops nothing; a log drops to a hand.
    assert_eq!(needs_correct_tool(BlockKind::Stone), Some(true));
    assert_eq!(needs_correct_tool(BlockKind::IronOre), Some(true));
    assert_eq!(needs_correct_tool(BlockKind::OakLog), Some(false));
    assert_eq!(needs_correct_tool(BlockKind::Dirt), Some(false));
}

#[test]
fn fuel_matches_the_game() {
    // fuel.edn
    assert_eq!(fuel_ticks(ItemKind::Coal), Some(1600));
    assert_eq!(fuel_ticks(ItemKind::OakPlanks), Some(300));
    assert_eq!(fuel_ticks(ItemKind::Stick), Some(100));
    assert_eq!(fuel_ticks(ItemKind::Stone), None);
}

#[test]
fn collision_shapes() {
    assert_eq!(collision(BlockKind::Air), Some(Collision::Empty));
    assert_eq!(collision(BlockKind::Stone), Some(Collision::Full));
    // A portal has no collision (a load-bearing fix in ruststeve v1).
    assert_eq!(collision(BlockKind::NetherPortal), Some(Collision::Empty));
    // shapes.edn: :oak_slab 0.5
    assert_eq!(
        collision(BlockKind::OakSlab),
        Some(Collision::Partial { top: 0.5 })
    );
}

#[test]
fn mobs_match_the_game() {
    // mobs.edn
    let blaze = mob(EntityKind::Blaze).unwrap();
    assert_eq!(blaze.max_health, 20.0);
    assert_eq!(blaze.category, MobCategory::Monster);
    assert_eq!(mob(EntityKind::Enderman).unwrap().max_health, 40.0);
    assert!(mob(EntityKind::Arrow).is_none());
}

#[test]
fn tool_materials() {
    // materials.edn: :stone {:speed 4.0, :durability 131}
    let stone = TOOL_MATERIALS.iter().find(|m| m.name == "stone").unwrap();
    assert_eq!(stone.speed, 4.0);
    assert_eq!(stone.durability, 131);
}

#[test]
fn entity_data_serializer_order() {
    // brain/wire/wire-entity-metadata.md: 0 byte, 1 int, 2 long, 3 float, 4 string,
    // 7 item stack, 8 boolean.
    let id = |name: &str| {
        ENTITY_DATA_SERIALIZERS
            .iter()
            .find(|s| s.name == name)
            .map(|s| s.id)
    };
    assert_eq!(id("BYTE"), Some(0));
    assert_eq!(id("INT"), Some(1));
    assert_eq!(id("LONG"), Some(2));
    assert_eq!(id("FLOAT"), Some(3));
    assert_eq!(id("STRING"), Some(4));
    assert_eq!(id("ITEM_STACK"), Some(7));
    assert_eq!(id("BOOLEAN"), Some(8));
    for (i, s) in ENTITY_DATA_SERIALIZERS.iter().enumerate() {
        assert_eq!(usize::try_from(s.id).unwrap(), i);
    }
}
