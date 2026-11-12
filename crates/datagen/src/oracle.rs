//! Compare the probe's tables with clojurecraft's (the oracle, `resources/clojurecraft/*.edn` at
//! the pin in brain/sources), entry by entry: hardness, needs-tool, fuel, partial shapes, mobs and
//! tool materials. Both read the same 26.1.2 classes, so any difference is a bug in one of them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::probe::{Collision, Probe};

/// The tokens of an EDN table as clojurecraft's datagen writes it: comments dropped, braces,
/// commas and the set's `#` removed, keywords without their colon.
fn tokens(dir: &Path, file: &str) -> Result<Vec<String>> {
    let path = dir.join(file);
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim_start().starts_with(";;"))
        .flat_map(|l| {
            l.replace(['{', '}', ',', '#'], " ")
                .split_whitespace()
                .map(|t| t.trim_start_matches(':').to_owned())
                .collect::<Vec<_>>()
        })
        .collect())
}

fn pairs(dir: &Path, file: &str) -> Result<BTreeMap<String, String>> {
    let t = tokens(dir, file)?;
    if t.len() % 2 != 0 {
        bail!("{file}: odd number of tokens");
    }
    Ok(t.chunks(2).map(|c| (c[0].clone(), c[1].clone())).collect())
}

fn num(file: &str, key: &str, v: &str) -> Result<f64> {
    v.parse()
        .with_context(|| format!("{file} {key}: {v} is not a number"))
}

/// Count matches and keep the first few differences for the report.
struct Tally {
    name: &'static str,
    equal: usize,
    diffs: Vec<String>,
}

impl Tally {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            equal: 0,
            diffs: Vec::new(),
        }
    }

    fn check(&mut self, key: &str, ours: String, theirs: String) {
        if ours == theirs {
            self.equal += 1;
        } else {
            self.diffs
                .push(format!("{key}: ruststeve {ours}, clojurecraft {theirs}"));
        }
    }

    fn keys(&mut self, ours: &BTreeSet<String>, theirs: &BTreeSet<String>) {
        for k in ours.symmetric_difference(theirs) {
            let side = if ours.contains(k) {
                "only ruststeve"
            } else {
                "only clojurecraft"
            };
            self.diffs.push(format!("{k}: {side}"));
        }
    }

    fn report(&self) -> bool {
        println!(
            "oracle {}: {} equal, {} different",
            self.name,
            self.equal,
            self.diffs.len()
        );
        for d in self.diffs.iter().take(20) {
            println!("  {d}");
        }
        self.diffs.is_empty()
    }
}

pub fn compare(dir: &Path, p: &Probe) -> Result<()> {
    let mut all_equal = true;

    let theirs = pairs(dir, "hardness.edn")?;
    let mut t = Tally::new("hardness");
    t.keys(
        &p.blocks.keys().cloned().collect(),
        &theirs.keys().cloned().collect(),
    );
    for (k, v) in &theirs {
        if let Some(b) = p.blocks.get(k) {
            t.check(
                k,
                format!("{:?}", b.hardness),
                format!("{:?}", num("hardness", k, v)?),
            );
        }
    }
    all_equal &= t.report();

    let theirs: BTreeSet<String> = tokens(dir, "needs-tool.edn")?.into_iter().collect();
    let ours: BTreeSet<String> = p
        .blocks
        .iter()
        .filter(|(_, b)| b.needs_correct_tool)
        .map(|(k, _)| k.clone())
        .collect();
    let mut t = Tally::new("needs-tool");
    t.keys(&ours, &theirs);
    t.equal = ours.intersection(&theirs).count();
    all_equal &= t.report();

    let theirs = pairs(dir, "fuel.edn")?;
    let mut t = Tally::new("fuel");
    t.keys(
        &p.fuel.keys().cloned().collect(),
        &theirs.keys().cloned().collect(),
    );
    for (k, v) in &theirs {
        if let Some(ticks) = p.fuel.get(k) {
            t.check(k, ticks.to_string(), v.clone());
        }
    }
    all_equal &= t.report();

    let theirs = pairs(dir, "shapes.edn")?;
    let ours: BTreeMap<String, f64> = p
        .blocks
        .iter()
        .filter_map(|(k, b)| match b.collision {
            Collision::Partial(top) => Some((k.clone(), top)),
            Collision::Empty | Collision::Full => None,
        })
        .collect();
    let mut t = Tally::new("partial shapes");
    t.keys(
        &ours.keys().cloned().collect(),
        &theirs.keys().cloned().collect(),
    );
    for (k, v) in &theirs {
        if let Some(top) = ours.get(k) {
            t.check(k, format!("{top:?}"), format!("{:?}", num("shapes", k, v)?));
        }
    }
    all_equal &= t.report();

    // {:allay {:max-health 20.0 :category :creature}}: five tokens per entry.
    let toks = tokens(dir, "mobs.edn")?;
    let mut theirs = BTreeMap::new();
    for c in toks.chunks(5) {
        if let [name, _, health, _, category] = c {
            theirs.insert(name.clone(), (num("mobs", name, health)?, category.clone()));
        } else {
            bail!("mobs.edn: an entry is not five tokens");
        }
    }
    let mut t = Tally::new("mobs");
    t.keys(
        &p.mobs.keys().cloned().collect(),
        &theirs.keys().cloned().collect(),
    );
    for (k, (health, category)) in &theirs {
        if let Some(m) = p.mobs.get(k) {
            t.check(
                k,
                format!("{:?} {}", m.max_health, m.category),
                format!("{health:?} {category}"),
            );
        }
    }
    all_equal &= t.report();

    // {:wooden {:speed 2.0, :durability 59, :incorrect :incorrect_for_wooden_tool}}: seven tokens;
    // clojurecraft names tiers as the items do (wooden, golden), the game's constants WOOD, GOLD.
    let toks = tokens(dir, "materials.edn")?;
    let mut theirs = BTreeMap::new();
    for c in toks.chunks(7) {
        if let [name, _, speed, _, durability, _, incorrect] = c {
            let name = match name.as_str() {
                "wooden" => "wood".to_owned(),
                "golden" => "gold".to_owned(),
                other => other.to_owned(),
            };
            theirs.insert(
                name,
                format!(
                    "{:?} {durability} {incorrect}",
                    num("materials", "speed", speed)?
                ),
            );
        } else {
            bail!("materials.edn: an entry is not seven tokens");
        }
    }
    let mut t = Tally::new("tool materials");
    t.keys(
        &p.materials.keys().cloned().collect(),
        &theirs.keys().cloned().collect(),
    );
    for (k, v) in &theirs {
        if let Some(m) = p.materials.get(k) {
            t.check(
                k,
                format!("{:?} {} {}", m.speed, m.durability, m.incorrect),
                v.clone(),
            );
        }
    }
    all_equal &= t.report();

    if !all_equal {
        bail!("the probe's tables differ from clojurecraft's (see above)");
    }
    println!("oracle: every compared table equals clojurecraft's");
    Ok(())
}
