//! Rust source for the tables the JNI probe reads from the game's classes (02b): hardness and
//! needs-correct-tool, collision shapes, tool materials, fuel, mobs, entity data serializers.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Context, Result, ensure};

use crate::emit::{header, pascal};
use crate::model::Game;
use crate::probe::{BlockProbe, Collision, Probe};

fn probe_header() -> String {
    let mut s = header();
    s.push_str(
        "// Read by datagen's JNI probe from the game's own classes (not in any data file);\n\
         // see crates/datagen/src/probe.rs.\n\n",
    );
    s
}

/// The probe's row for every block, in block-id order; a block the probe missed fails the run.
fn block_rows<'p>(g: &Game, p: &'p Probe) -> Result<Vec<&'p BlockProbe>> {
    ensure!(
        p.blocks.len() == g.blocks.len(),
        "the probe read {} blocks, the registry has {}",
        p.blocks.len(),
        g.blocks.len()
    );
    g.blocks
        .iter()
        .map(|b| {
            p.blocks
                .get(&b.name)
                .with_context(|| format!("the probe has no hardness for {}", b.name))
        })
        .collect()
}

/// A probe value as an `f32` literal, checked to be exact (the game stores these as `float`).
fn f32_lit(what: &str, v: f64) -> Result<String> {
    #[allow(clippy::cast_possible_truncation)]
    let f = v as f32;
    ensure!(f64::from(f) == v, "{what}: {v} is not a float");
    Ok(format!("{f:?}"))
}

pub fn hardness_rs(g: &Game, p: &Probe) -> Result<String> {
    let rows = block_rows(g, p)?;
    let n = rows.len();
    let mut s = probe_header();
    writeln!(s, "use super::blocks::BlockKind;\n")?;
    writeln!(
        s,
        "/// Each block's hardness (its default state's `getDestroySpeed`), by block id; -1.0 is"
    )?;
    writeln!(s, "/// unbreakable (bedrock).")?;
    writeln!(s, "pub static HARDNESS: [f32; {n}] = [")?;
    for (b, r) in g.blocks.iter().zip(&rows) {
        writeln!(s, "    {}, // {}", f32_lit(&b.name, r.hardness)?, b.name)?;
    }
    writeln!(s, "];\n")?;
    writeln!(
        s,
        "/// Whether a block drops only to its correct tool (`requiresCorrectToolForDrops`): stone"
    )?;
    writeln!(s, "/// dug by hand drops nothing. By block id.")?;
    writeln!(s, "pub static NEEDS_CORRECT_TOOL: [bool; {n}] = [")?;
    for (b, r) in g.blocks.iter().zip(&rows) {
        writeln!(s, "    {}, // {}", r.needs_correct_tool, b.name)?;
    }
    writeln!(s, "];\n")?;
    writeln!(s, "/// A block's hardness; `None` for `Unknown`.")?;
    writeln!(s, "pub fn hardness(block: BlockKind) -> Option<f32> {{")?;
    writeln!(s, "    if let BlockKind::Unknown(_) = block {{")?;
    writeln!(s, "        return None;")?;
    writeln!(s, "    }}")?;
    writeln!(
        s,
        "    usize::try_from(block.id()).ok().and_then(|i| HARDNESS.get(i)).copied()"
    )?;
    writeln!(s, "}}\n")?;
    writeln!(
        s,
        "/// Whether a block drops only to its correct tool; `None` for `Unknown`."
    )?;
    writeln!(
        s,
        "pub fn needs_correct_tool(block: BlockKind) -> Option<bool> {{"
    )?;
    writeln!(s, "    if let BlockKind::Unknown(_) = block {{")?;
    writeln!(s, "        return None;")?;
    writeln!(s, "    }}")?;
    writeln!(
        s,
        "    usize::try_from(block.id()).ok().and_then(|i| NEEDS_CORRECT_TOOL.get(i)).copied()"
    )?;
    writeln!(s, "}}")?;
    Ok(s)
}

pub fn shapes_rs(g: &Game, p: &Probe) -> Result<String> {
    let rows = block_rows(g, p)?;
    let mut s = probe_header();
    writeln!(s, "use super::blocks::BlockKind;")?;
    writeln!(s, "use crate::types::Collision;\n")?;
    writeln!(
        s,
        "/// Each block's default-state collision shape (`getCollisionShape`), by block id."
    )?;
    writeln!(s, "pub static COLLISION: [Collision; {}] = [", rows.len())?;
    for (b, r) in g.blocks.iter().zip(&rows) {
        let c = match r.collision {
            Collision::Empty => "Collision::Empty".to_owned(),
            Collision::Full => "Collision::Full".to_owned(),
            Collision::Partial(top) => format!("Collision::Partial {{ top: {top:?} }}"),
        };
        writeln!(s, "    {c}, // {}", b.name)?;
    }
    writeln!(s, "];\n")?;
    writeln!(
        s,
        "/// A block's default-state collision shape; `None` for `Unknown`."
    )?;
    writeln!(
        s,
        "pub fn collision(block: BlockKind) -> Option<Collision> {{"
    )?;
    writeln!(s, "    if let BlockKind::Unknown(_) = block {{")?;
    writeln!(s, "        return None;")?;
    writeln!(s, "    }}")?;
    writeln!(
        s,
        "    usize::try_from(block.id()).ok().and_then(|i| COLLISION.get(i)).copied()"
    )?;
    writeln!(s, "}}")?;
    Ok(s)
}

pub fn tools_rs(p: &Probe) -> Result<String> {
    let mut s = probe_header();
    writeln!(s, "use crate::types::ToolMaterial;\n")?;
    writeln!(s, "/// Every `ToolMaterial` constant, sorted by name.")?;
    writeln!(
        s,
        "pub static TOOL_MATERIALS: [ToolMaterial; {}] = [",
        p.materials.len()
    )?;
    for (name, m) in &p.materials {
        writeln!(
            s,
            "    ToolMaterial {{ name: {name:?}, speed: {}, durability: {}, incorrect_for_drops: {:?} }},",
            f32_lit(name, m.speed)?,
            u32::try_from(m.durability)?,
            m.incorrect
        )?;
    }
    writeln!(s, "];")?;
    Ok(s)
}

pub fn fuel_rs(g: &Game, p: &Probe) -> Result<String> {
    let ids: BTreeMap<&str, usize> = g
        .items
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut rows = Vec::new();
    for (item, ticks) in &p.fuel {
        let id = *ids
            .get(item.as_str())
            .with_context(|| format!("fuel: unknown item {item}"))?;
        rows.push((id, item, u32::try_from(*ticks)?));
    }
    rows.sort_unstable();
    let mut s = probe_header();
    writeln!(s, "use super::items::ItemKind;")?;
    writeln!(s, "use crate::types::Fuel;\n")?;
    writeln!(
        s,
        "/// Every item a furnace burns and its burn time in game ticks (`FuelValues`), sorted by item id."
    )?;
    writeln!(s, "pub static FUEL: [Fuel; {}] = [", rows.len())?;
    for (_, item, ticks) in &rows {
        writeln!(
            s,
            "    Fuel {{ item: ItemKind::{}, ticks: {ticks} }},",
            pascal(item)
        )?;
    }
    writeln!(s, "];\n")?;
    writeln!(
        s,
        "/// How many game ticks an item burns for; `None` when it is not fuel."
    )?;
    writeln!(s, "pub fn fuel_ticks(item: ItemKind) -> Option<u32> {{")?;
    writeln!(
        s,
        "    FUEL.binary_search_by_key(&item.id(), |f| f.item.id()).ok().and_then(|i| FUEL.get(i)).map(|f| f.ticks)"
    )?;
    writeln!(s, "}}")?;
    Ok(s)
}

pub fn mobs_rs(g: &Game, p: &Probe) -> Result<String> {
    let ids: BTreeMap<&str, usize> = g
        .entities
        .names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut categories: Vec<&str> = p.mobs.values().map(|m| m.category.as_str()).collect();
    categories.sort_unstable();
    categories.dedup();
    let mut rows = Vec::new();
    for (entity, m) in &p.mobs {
        let id = *ids
            .get(entity.as_str())
            .with_context(|| format!("mobs: unknown entity {entity}"))?;
        rows.push((id, entity, m));
    }
    rows.sort_by_key(|r| r.0);
    let mut s = probe_header();
    writeln!(s, "use super::entities::EntityKind;")?;
    writeln!(s, "use crate::types::Mob;\n")?;
    writeln!(
        s,
        "/// The game's mob categories (`MobCategory`) that living entities use."
    )?;
    writeln!(
        s,
        "#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]"
    )?;
    writeln!(s, "pub enum MobCategory {{")?;
    for c in &categories {
        writeln!(s, "    {},", pascal(c))?;
    }
    writeln!(s, "}}\n")?;
    writeln!(
        s,
        "/// Every entity type the game gives attributes to (the living ones): base max health"
    )?;
    writeln!(s, "/// (`MAX_HEALTH`) and category. Sorted by entity id.")?;
    writeln!(s, "pub static MOBS: [Mob; {}] = [", rows.len())?;
    for (_, entity, m) in &rows {
        writeln!(
            s,
            "    Mob {{ entity: EntityKind::{}, max_health: {:?}, category: MobCategory::{} }},",
            pascal(entity),
            m.max_health,
            pascal(&m.category)
        )?;
    }
    writeln!(s, "];\n")?;
    writeln!(
        s,
        "/// The entity type's health and category; `None` for a non-living type."
    )?;
    writeln!(
        s,
        "pub fn mob(entity: EntityKind) -> Option<&'static Mob> {{"
    )?;
    writeln!(
        s,
        "    MOBS.binary_search_by_key(&entity.id(), |m| m.entity.id()).ok().and_then(|i| MOBS.get(i))"
    )?;
    writeln!(s, "}}")?;
    Ok(s)
}

pub fn metadata_rs(p: &Probe) -> Result<String> {
    let mut rows: Vec<(i64, &String)> = p.serializers.iter().map(|(n, id)| (*id, n)).collect();
    rows.sort_unstable();
    let mut s = probe_header();
    writeln!(s, "use crate::types::DataSerializer;\n")?;
    writeln!(
        s,
        "/// The entity data serializers in id order: an entity-data entry names its value type by"
    )?;
    writeln!(
        s,
        "/// this id, so the order is load-bearing (brain/wire/wire-entity-metadata.md)."
    )?;
    writeln!(
        s,
        "pub static ENTITY_DATA_SERIALIZERS: [DataSerializer; {}] = [",
        rows.len()
    )?;
    for (id, name) in &rows {
        writeln!(s, "    DataSerializer {{ name: {name:?}, id: {id} }},")?;
    }
    writeln!(s, "];")?;
    Ok(s)
}
