//! The game's tables read from the reports and the inner jar, checked, in the order they are
//! emitted. Each reader follows clojurecraft's `dev/clojurecraft/datagen.clj` (the oracle), so the
//! counts can be compared table for table.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;

/// A vanilla id without its namespace: `minecraft:oak_log` → `oak_log`.
pub fn strip_ns(s: &str) -> &str {
    s.strip_prefix("minecraft:").unwrap_or(s)
}

/// The file name of a `.json` path without directory or extension.
fn file_stem(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".json").unwrap_or(name)
}

fn parse(path: &str, text: &str) -> Result<Value> {
    serde_json::from_str(text).with_context(|| format!("parse {path}"))
}

fn read_json(path: &Path) -> Result<Value> {
    let text = std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    parse(&path.display().to_string(), &text)
}

pub struct Version {
    pub id: String,
    pub protocol: u64,
    pub world: u64,
    pub java: u64,
}

/// One connection state and direction's packets, names in id order (ids are 0..n).
pub struct PacketSet {
    pub state: String,
    pub direction: String,
    pub names: Vec<String>,
}

/// A registry's entries in protocol-id order (ids are 0..n).
pub struct Registry {
    pub names: Vec<String>,
}

pub struct Block {
    pub name: String,
    pub first_state: u64,
    pub last_state: u64,
    pub default_state: u64,
    /// In the report's order: a state's id counts through them with the last property fastest.
    pub properties: Vec<(String, Vec<String>)>,
}

/// An ingredient: the item names it accepts.
pub type Ingredient = BTreeSet<String>;

pub enum Shape {
    Shaped {
        width: usize,
        height: usize,
        /// Row by row; `None` is an empty cell.
        cells: Vec<Option<Ingredient>>,
    },
    Shapeless {
        ingredients: Vec<Ingredient>,
    },
}

pub struct Recipe {
    pub id: String,
    pub result: String,
    pub count: u64,
    pub shape: Shape,
}

pub struct Smelt {
    pub id: String,
    pub input: Ingredient,
    pub result: String,
    pub count: u64,
    pub ticks: u64,
}

pub struct Dimension {
    pub name: String,
    pub min_y: i64,
    pub height: u64,
    pub logical_height: u64,
    pub coordinate_scale: f64,
}

pub struct Harvest {
    pub tool: Option<&'static str>,
    pub needs: Option<&'static str>,
}

pub struct Game {
    pub version: Version,
    pub packets: Vec<PacketSet>,
    pub items: Registry,
    pub entities: Registry,
    pub blocks: Vec<Block>,
    /// Registries the wire sends as indexes, by registry name (`menu`, `sound_event`, ...).
    pub registries: BTreeMap<&'static str, Registry>,
    pub item_tags: BTreeMap<String, BTreeSet<String>>,
    pub block_tags: BTreeMap<String, BTreeSet<String>>,
    pub recipes: Vec<Recipe>,
    pub smelting: Vec<Smelt>,
    /// `{block: [(item, chance)]}`: the first loot pool's tries for a breaker with a plain tool.
    pub drops: BTreeMap<String, Vec<(String, f64)>>,
    pub dimensions: Vec<Dimension>,
    pub damage_types: Vec<String>,
    pub biomes: Vec<String>,
    pub harvest: BTreeMap<String, Harvest>,
}

/// The states in the order the protocol walks them, and the two directions.
const STATES: [&str; 5] = ["handshake", "status", "login", "configuration", "play"];
const DIRECTIONS: [&str; 2] = ["clientbound", "serverbound"];

/// Registries from `registries.json` kept as name tables (the wire sends their indexes).
const NAME_REGISTRIES: [&str; 8] = [
    "attribute",
    "data_component_type",
    "fluid",
    "menu",
    "mob_effect",
    "particle_type",
    "potion",
    "sound_event",
];

/// The `mineable/*` block tags and the tool kind each names.
const MINEABLE: [(&str, &str); 4] = [
    ("mineable/pickaxe", "Pickaxe"),
    ("mineable/axe", "Axe"),
    ("mineable/shovel", "Shovel"),
    ("mineable/hoe", "Hoe"),
];

/// The `needs_*_tool` tags, lowest tier first: a block in a later one needs at least that tier.
const NEEDS: [(&str, &str); 3] = [
    ("needs_stone_tool", "Stone"),
    ("needs_iron_tool", "Iron"),
    ("needs_diamond_tool", "Diamond"),
];

impl Game {
    pub fn read(reports: &Path, entries: &BTreeMap<String, String>) -> Result<Self> {
        let registries_json = read_json(&reports.join("registries.json"))?;
        let registry = |name: &str| -> Result<Registry> {
            read_registry(&registries_json, &format!("minecraft:{name}"))
        };
        let items = registry("item")?;
        let entities = registry("entity_type")?;
        let block_registry = registry("block")?;
        let mut registries = BTreeMap::new();
        for name in NAME_REGISTRIES {
            registries.insert(name, registry(name)?);
        }
        let blocks = read_blocks(&reports.join("blocks.json"), &block_registry)?;
        let item_tags = tags_of(entries, "item")?;
        let block_tags = tags_of(entries, "block")?;
        let game = Game {
            version: read_version(entries)?,
            packets: read_packets(&reports.join("packets.json"))?,
            recipes: read_recipes(entries, &item_tags)?,
            smelting: read_smelting(entries, &item_tags)?,
            drops: read_drops(entries)?,
            dimensions: read_dimensions(entries)?,
            damage_types: sorted_stems(entries, "data/minecraft/damage_type/"),
            biomes: sorted_stems(entries, "data/minecraft/worldgen/biome/"),
            harvest: harvest(&block_tags),
            items,
            entities,
            blocks,
            registries,
            item_tags,
            block_tags,
        };
        game.check_names()?;
        Ok(game)
    }

    /// Every item and block a table names exists in its registry: an unknown name fails the run.
    fn check_names(&self) -> Result<()> {
        let items: BTreeSet<&str> = self.items.names.iter().map(String::as_str).collect();
        let blocks: BTreeSet<&str> = self.blocks.iter().map(|b| b.name.as_str()).collect();
        let item = |n: &str, at: &str| -> Result<()> {
            ensure!(items.contains(n), "{at}: unknown item {n}");
            Ok(())
        };
        for (tag, vs) in &self.item_tags {
            vs.iter()
                .try_for_each(|v| item(v, &format!("item tag {tag}")))?;
        }
        for (tag, vs) in &self.block_tags {
            for v in vs {
                ensure!(
                    blocks.contains(v.as_str()),
                    "block tag {tag}: unknown block {v}"
                );
            }
        }
        for r in &self.recipes {
            item(&r.result, &r.id)?;
            let ings: Vec<&Ingredient> = match &r.shape {
                Shape::Shaped { cells, .. } => cells.iter().flatten().collect(),
                Shape::Shapeless { ingredients } => ingredients.iter().collect(),
            };
            for i in ings {
                i.iter().try_for_each(|v| item(v, &r.id))?;
            }
        }
        for s in &self.smelting {
            item(&s.result, &s.id)?;
            s.input.iter().try_for_each(|v| item(v, &s.id))?;
        }
        for (block, tries) in &self.drops {
            ensure!(
                blocks.contains(block.as_str()),
                "loot: unknown block {block}"
            );
            tries
                .iter()
                .try_for_each(|(i, _)| item(i, &format!("loot {block}")))?;
        }
        for b in self.harvest.keys() {
            ensure!(blocks.contains(b.as_str()), "harvest: unknown block {b}");
        }
        Ok(())
    }

    pub fn print_counts(&self) {
        println!(
            "version {} protocol {}",
            self.version.id, self.version.protocol
        );
        for p in &self.packets {
            println!("packets {} {}: {}", p.state, p.direction, p.names.len());
        }
        let states = self
            .blocks
            .iter()
            .map(|b| b.last_state + 1)
            .max()
            .unwrap_or(0);
        println!("blocks {} states {states}", self.blocks.len());
        println!("items {}", self.items.names.len());
        println!("entity types {}", self.entities.names.len());
        println!("crafting recipes {}", self.recipes.len());
        println!("smelting recipes {}", self.smelting.len());
        println!(
            "item tags {} block tags {}",
            self.item_tags.len(),
            self.block_tags.len()
        );
        println!("blocks with drops {}", self.drops.len());
        println!(
            "dimension types {} damage types {} biomes {}",
            self.dimensions.len(),
            self.damage_types.len(),
            self.biomes.len()
        );
    }
}

fn read_version(entries: &BTreeMap<String, String>) -> Result<Version> {
    let text = entries
        .get("version.json")
        .context("no version.json in the jar")?;
    let v = parse("version.json", text)?;
    let num = |k: &str| v[k].as_u64().with_context(|| format!("version.json {k}"));
    Ok(Version {
        id: v["id"].as_str().context("version.json id")?.to_owned(),
        protocol: num("protocol_version")?,
        world: num("world_version")?,
        java: num("java_version")?,
    })
}

/// Names ordered by id, with the ids checked to run 0..n with no gaps.
fn by_id(what: &str, obj: &serde_json::Map<String, Value>) -> Result<Vec<String>> {
    let mut rows = Vec::new();
    for (name, m) in obj {
        let id = m["protocol_id"]
            .as_u64()
            .with_context(|| format!("{what} {name}: no protocol_id"))?;
        rows.push((id, strip_ns(name).to_owned()));
    }
    rows.sort();
    for (i, (id, name)) in rows.iter().enumerate() {
        ensure!(*id == i as u64, "{what}: {name} has id {id}, expected {i}");
    }
    Ok(rows.into_iter().map(|(_, n)| n).collect())
}

fn read_packets(path: &Path) -> Result<Vec<PacketSet>> {
    let j = read_json(path)?;
    let obj = j.as_object().context("packets.json is not an object")?;
    for state in obj.keys() {
        ensure!(
            STATES.contains(&state.as_str()),
            "packets.json: unknown state {state}"
        );
    }
    let mut sets = Vec::new();
    for state in STATES {
        for direction in DIRECTIONS {
            if let Some(ps) = j[state][direction].as_object() {
                sets.push(PacketSet {
                    state: state.to_owned(),
                    direction: direction.to_owned(),
                    names: by_id(&format!("packets {state} {direction}"), ps)?,
                });
            }
        }
    }
    Ok(sets)
}

fn read_registry(j: &Value, name: &str) -> Result<Registry> {
    let entries = j[name]["entries"]
        .as_object()
        .with_context(|| format!("registries.json has no {name}"))?;
    Ok(Registry {
        names: by_id(name, entries)?,
    })
}

/// Blocks in registry-id order, with their state ranges and properties from blocks.json.
fn read_blocks(path: &Path, registry: &Registry) -> Result<Vec<Block>> {
    let j = read_json(path)?;
    let obj = j.as_object().context("blocks.json is not an object")?;
    ensure!(
        obj.len() == registry.names.len(),
        "blocks.json has {} blocks, the block registry {}",
        obj.len(),
        registry.names.len()
    );
    let mut blocks = Vec::new();
    for name in &registry.names {
        let m = &j[format!("minecraft:{name}")];
        let states = m["states"]
            .as_array()
            .with_context(|| format!("blocks.json: no states for {name}"))?;
        let mut ids = Vec::new();
        let mut default = None;
        for s in states {
            let id = s["id"]
                .as_u64()
                .with_context(|| format!("{name}: state id"))?;
            if s["default"].as_bool() == Some(true) {
                default = Some(id);
            }
            ids.push(id);
        }
        let first = *ids.iter().min().context("no states")?;
        let last = *ids.iter().max().context("no states")?;
        let mut properties = Vec::new();
        if let Some(props) = m["properties"].as_object() {
            for (k, vs) in props {
                let values = vs
                    .as_array()
                    .with_context(|| format!("{name}.{k}: values"))?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .with_context(|| format!("{name}.{k}: a value is not a string"))?;
                properties.push((k.clone(), values));
            }
        }
        let product: u64 = properties.iter().map(|(_, v)| v.len() as u64).product();
        ensure!(
            last - first + 1 == ids.len() as u64 && ids.len() as u64 == product,
            "{name}: states {first}..={last} ({}) do not match its properties ({product})",
            ids.len()
        );
        blocks.push(Block {
            name: name.clone(),
            first_state: first,
            last_state: last,
            default_state: default.with_context(|| format!("{name}: no default state"))?,
            properties,
        });
    }
    let mut ranges: Vec<(u64, u64)> = blocks
        .iter()
        .map(|b| (b.first_state, b.last_state))
        .collect();
    ranges.sort();
    for (i, w) in ranges.windows(2).enumerate() {
        ensure!(
            w[0].1 + 1 == w[1].0,
            "state ids have a gap or overlap after range {i}"
        );
    }
    ensure!(
        ranges.first().map(|r| r.0) == Some(0),
        "state ids do not start at 0"
    );
    Ok(blocks)
}

/// The tag a `data/minecraft/tags/<kind>/*.json` path defines (subfolders kept).
fn tag_name<'a>(kind: &str, path: &'a str) -> Option<&'a str> {
    path.strip_prefix(&format!("data/minecraft/tags/{kind}/"))?
        .strip_suffix(".json")
}

/// `{tag: names}` for every tag of one kind, nested `#tag`s resolved, cycles cut (as
/// clojurecraft's `tags-of`).
fn tags_of(
    entries: &BTreeMap<String, String>,
    kind: &str,
) -> Result<BTreeMap<String, BTreeSet<String>>> {
    let mut raw: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (path, text) in entries {
        let Some(tag) = tag_name(kind, path) else {
            continue;
        };
        let j = parse(path, text)?;
        let values = j["values"]
            .as_array()
            .with_context(|| format!("{path}: no values"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .or_else(|| v["id"].as_str())
                    .map(str::to_owned)
                    .with_context(|| format!("{path}: odd value {v}"))
            })
            .collect::<Result<Vec<_>>>()?;
        raw.insert(tag.to_owned(), values);
    }
    fn resolve(
        raw: &BTreeMap<String, Vec<String>>,
        tag: &str,
        seen: &BTreeSet<String>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for v in raw.get(tag).into_iter().flatten() {
            if let Some(nested) = v.strip_prefix('#') {
                let t = strip_ns(nested);
                if !seen.contains(t) {
                    let mut seen = seen.clone();
                    seen.insert(t.to_owned());
                    out.extend(resolve(raw, t, &seen));
                }
            } else {
                out.insert(strip_ns(v).to_owned());
            }
        }
        out
    }
    Ok(raw
        .keys()
        .map(|t| {
            let seen = BTreeSet::from([t.clone()]);
            (t.clone(), resolve(&raw, t, &seen))
        })
        .collect())
}

/// The items an ingredient accepts: an id, a `#tag` (empty when unknown), or a list of either.
fn ingredient(tags: &BTreeMap<String, BTreeSet<String>>, v: &Value) -> Result<Ingredient> {
    match v {
        Value::Array(vs) => {
            let mut out = Ingredient::new();
            for x in vs {
                out.extend(ingredient(tags, x)?);
            }
            Ok(out)
        }
        Value::String(s) => Ok(match s.strip_prefix('#') {
            Some(t) => tags.get(strip_ns(t)).cloned().unwrap_or_default(),
            None => Ingredient::from([strip_ns(s).to_owned()]),
        }),
        other => bail!("unexpected ingredient {other}"),
    }
}

/// A shaped pattern with blank rows and columns trimmed, as vanilla does when it loads it.
fn shrink(pattern: &[String]) -> Vec<Vec<char>> {
    let width = pattern.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let rows: Vec<Vec<char>> = pattern
        .iter()
        .map(|r| {
            let mut cs: Vec<char> = r.chars().collect();
            cs.resize(width, ' ');
            cs
        })
        .filter(|r| r.iter().any(|c| *c != ' '))
        .collect();
    let cols: Vec<usize> = (0..width)
        .filter(|&c| rows.iter().any(|r| r[c] != ' '))
        .collect();
    let (Some(&c0), Some(&c1)) = (cols.first(), cols.last()) else {
        return Vec::new();
    };
    rows.into_iter().map(|r| r[c0..=c1].to_vec()).collect()
}

fn recipe_files(entries: &BTreeMap<String, String>) -> impl Iterator<Item = (&str, &str)> {
    entries.iter().filter_map(|(p, t)| {
        p.strip_prefix("data/minecraft/recipe/")
            .filter(|rest| !rest.contains('/'))
            .map(|_| (p.as_str(), t.as_str()))
    })
}

fn result_of(path: &str, r: &Value) -> Result<(String, u64)> {
    let id = r["result"]["id"]
        .as_str()
        .with_context(|| format!("{path}: no result id"))?;
    Ok((
        strip_ns(id).to_owned(),
        r["result"]["count"].as_u64().unwrap_or(1),
    ))
}

/// Every `crafting_shaped` and `crafting_shapeless` recipe (the two clojurecraft counts), tags
/// resolved, shaped patterns shrunk like vanilla; sorted by id.
fn read_recipes(
    entries: &BTreeMap<String, String>,
    tags: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<Recipe>> {
    let mut out = Vec::new();
    for (path, text) in recipe_files(entries) {
        let r = parse(path, text)?;
        let kind = r["type"].as_str().unwrap_or("");
        let shape = match kind {
            "minecraft:crafting_shaped" => {
                let pattern = r["pattern"]
                    .as_array()
                    .with_context(|| format!("{path}: no pattern"))?
                    .iter()
                    .map(|row| row.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .with_context(|| format!("{path}: odd pattern"))?;
                let rows = shrink(&pattern);
                let mut key = BTreeMap::new();
                for (k, v) in r["key"]
                    .as_object()
                    .with_context(|| format!("{path}: no key"))?
                {
                    let c = k
                        .chars()
                        .next()
                        .with_context(|| format!("{path}: empty key"))?;
                    key.insert(c, ingredient(tags, v)?);
                }
                let mut cells = Vec::new();
                for row in &rows {
                    for c in row {
                        cells.push(if *c == ' ' {
                            None
                        } else {
                            Some(
                                key.get(c)
                                    .cloned()
                                    .with_context(|| format!("{path}: no key for {c}"))?,
                            )
                        });
                    }
                }
                Shape::Shaped {
                    width: rows.first().map_or(0, Vec::len),
                    height: rows.len(),
                    cells,
                }
            }
            "minecraft:crafting_shapeless" => Shape::Shapeless {
                ingredients: r["ingredients"]
                    .as_array()
                    .with_context(|| format!("{path}: no ingredients"))?
                    .iter()
                    .map(|v| ingredient(tags, v))
                    .collect::<Result<_>>()?,
            },
            _ => continue,
        };
        let (result, count) = result_of(path, &r)?;
        out.push(Recipe {
            id: file_stem(path).to_owned(),
            result,
            count,
            shape,
        });
    }
    Ok(out)
}

/// Every furnace (`minecraft:smelting`) recipe; cooking time 200 ticks when absent.
fn read_smelting(
    entries: &BTreeMap<String, String>,
    tags: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<Smelt>> {
    let mut out = Vec::new();
    for (path, text) in recipe_files(entries) {
        let r = parse(path, text)?;
        if r["type"].as_str() != Some("minecraft:smelting") {
            continue;
        }
        let (result, count) = result_of(path, &r)?;
        out.push(Smelt {
            id: file_stem(path).to_owned(),
            input: ingredient(tags, &r["ingredient"])?,
            result,
            count,
            ticks: r["cookingtime"].as_u64().unwrap_or(200),
        });
    }
    Ok(out)
}

/// Does a loot condition ask for a particular tool (silk touch, shears)?
fn tool_gated(c: &Value) -> bool {
    match c["condition"].as_str() {
        Some("minecraft:match_tool") => true,
        Some("minecraft:any_of" | "minecraft:all_of") => c["terms"]
            .as_array()
            .is_some_and(|ts| ts.iter().any(tool_gated)),
        _ => false,
    }
}

/// A number, or a constant number provider.
fn number(v: &Value) -> Result<f64> {
    v.as_f64()
        .or_else(|| {
            (v["type"] == "minecraft:constant")
                .then(|| v["value"].as_f64())
                .flatten()
        })
        .with_context(|| format!("not a constant number: {v}"))
}

/// The chance an entry drops for a breaker with no enchanted tool: `None` when it needs a tool.
fn entry_chance(conditions: &Value) -> Result<Option<f64>> {
    let mut chance = 1.0;
    for c in conditions.as_array().into_iter().flatten() {
        match c["condition"].as_str() {
            Some("minecraft:match_tool" | "minecraft:any_of" | "minecraft:all_of") => {
                if tool_gated(c) {
                    return Ok(None);
                }
            }
            Some("minecraft:table_bonus") => {
                chance *= number(&c["chances"][0])?;
            }
            Some("minecraft:random_chance") => {
                chance *= number(&c["chance"])?;
            }
            _ => {}
        }
    }
    Ok(Some(chance))
}

fn walk(e: &Value, out: &mut Vec<(String, f64)>) -> Result<()> {
    match e["type"].as_str() {
        Some("minecraft:item") => {
            if let Some(c) = entry_chance(&e["conditions"])? {
                let name = e["name"].as_str().context("loot item with no name")?;
                out.push((strip_ns(name).to_owned(), c));
            }
        }
        Some("minecraft:alternatives") if entry_chance(&e["conditions"])?.is_some() => {
            for child in e["children"].as_array().into_iter().flatten() {
                walk(child, out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// `{block: [(item, chance)]}`: the tries of each block's first loot pool for a breaker with no
/// enchanted tool, tool-gated entries and pools skipped (clojurecraft's `drops`).
fn read_drops(entries: &BTreeMap<String, String>) -> Result<BTreeMap<String, Vec<(String, f64)>>> {
    let mut out = BTreeMap::new();
    for (path, text) in entries {
        let Some(rest) = path.strip_prefix("data/minecraft/loot_table/blocks/") else {
            continue;
        };
        if rest.contains('/') {
            continue;
        }
        let j = parse(path, text)?;
        let pool = &j["pools"][0];
        let mut tries = Vec::new();
        if !pool.is_null() && entry_chance(&pool["conditions"])?.is_some() {
            for e in pool["entries"].as_array().into_iter().flatten() {
                walk(e, &mut tries).with_context(|| path.clone())?;
            }
        }
        if !tries.is_empty() {
            out.insert(file_stem(path).to_owned(), tries);
        }
    }
    Ok(out)
}

/// Every dimension type, sorted by name: the registry order the server syncs them in.
fn read_dimensions(entries: &BTreeMap<String, String>) -> Result<Vec<Dimension>> {
    let mut out = Vec::new();
    for (path, text) in entries {
        if !path.starts_with("data/minecraft/dimension_type/") {
            continue;
        }
        let d = parse(path, text)?;
        let num = |k: &str| d[k].as_u64().with_context(|| format!("{path}: {k}"));
        out.push(Dimension {
            name: file_stem(path).to_owned(),
            min_y: d["min_y"]
                .as_i64()
                .with_context(|| format!("{path}: min_y"))?,
            height: num("height")?,
            logical_height: num("logical_height")?,
            coordinate_scale: d["coordinate_scale"]
                .as_f64()
                .with_context(|| format!("{path}: coordinate_scale"))?,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The file stems under a directory, sorted: the registry order the server syncs data-driven
/// registries in (damage types, biomes).
fn sorted_stems(entries: &BTreeMap<String, String>, dir: &str) -> Vec<String> {
    entries
        .keys()
        .filter(|p| p.starts_with(dir))
        .map(|p| file_stem(p).to_owned())
        .collect()
}

/// `{block: harvest}` from the block tags: the tool kind that mines it, the lowest tier that
/// drops it when one is required. A block in no such tag is absent (any tool, by hand).
fn harvest(block_tags: &BTreeMap<String, BTreeSet<String>>) -> BTreeMap<String, Harvest> {
    let mut out: BTreeMap<String, Harvest> = BTreeMap::new();
    for (tag, tool) in MINEABLE {
        for b in block_tags.get(tag).into_iter().flatten() {
            out.entry(b.clone())
                .or_insert(Harvest {
                    tool: None,
                    needs: None,
                })
                .tool = Some(tool);
        }
    }
    for (tag, tier) in NEEDS {
        for b in block_tags.get(tag).into_iter().flatten() {
            out.entry(b.clone())
                .or_insert(Harvest {
                    tool: None,
                    needs: None,
                })
                .needs = Some(tier);
        }
    }
    out
}
