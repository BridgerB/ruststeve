//! The data files inside the inner server jar (`META-INF/versions/<v>/server-<v>.jar` in the
//! bundler jar): recipes, tags, loot tables, dimension types, damage types, biomes, version.json.
//! None of these are in the `--reports` output.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};
use std::path::Path;

use anyhow::{Context, Result};

/// Directories whose JSON files are read, one level deep (no subfolders).
const FLAT_DIRS: [&str; 5] = [
    "data/minecraft/recipe/",
    "data/minecraft/loot_table/blocks/",
    "data/minecraft/dimension_type/",
    "data/minecraft/damage_type/",
    "data/minecraft/worldgen/biome/",
];

/// Tag directories, read at any depth (`mineable/pickaxe` lives in a subfolder).
const TAG_DIRS: [&str; 2] = ["data/minecraft/tags/item/", "data/minecraft/tags/block/"];

fn wanted(name: &str) -> bool {
    if name == "version.json" {
        return true;
    }
    if !name.ends_with(".json") {
        return false;
    }
    FLAT_DIRS
        .iter()
        .any(|d| name.strip_prefix(d).is_some_and(|rest| !rest.contains('/')))
        || TAG_DIRS.iter().any(|d| name.starts_with(d))
}

/// `{path: text}` for every wanted file in the inner jar, sorted by path.
pub fn inner_entries(outer_jar: &Path) -> Result<BTreeMap<String, String>> {
    let file =
        std::fs::File::open(outer_jar).with_context(|| format!("open {}", outer_jar.display()))?;
    let mut outer = zip::ZipArchive::new(file).context("read the bundler jar")?;
    let mut inner_bytes = None;
    for i in 0..outer.len() {
        let mut entry = outer.by_index(i)?;
        let name = entry.name().to_owned();
        if name.starts_with("META-INF/versions/")
            && name.contains("/server-")
            && name.ends_with(".jar")
        {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            inner_bytes = Some(bytes);
            break;
        }
    }
    let inner_bytes = inner_bytes.context("no META-INF/versions/*/server-*.jar in the jar")?;
    let mut inner = zip::ZipArchive::new(Cursor::new(inner_bytes)).context("read the inner jar")?;
    let mut entries = BTreeMap::new();
    for i in 0..inner.len() {
        let mut entry = inner.by_index(i)?;
        let name = entry.name().to_owned();
        if wanted(&name) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .with_context(|| format!("read {name}"))?;
            entries.insert(name, text);
        }
    }
    println!("inner jar: {} data files", entries.len());
    Ok(entries)
}
