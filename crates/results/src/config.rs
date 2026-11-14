//! The race's shared config and its starts, as data (`config/race.toml`, `config/starts.toml`).
//! The race, the server it starts and (from issue 08) the model read the same files
//! (brain/ruststeve/decisions/measurement-rules.md).

use data::ItemKind;
use serde::{Deserialize, Serialize};
use world::Stage;

/// `config/race.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceConfig {
    pub server: ServerConfig,
    pub race: RaceRules,
    /// The brain's CPU budget per bot per tick (issue 09).
    #[serde(default)]
    pub budget: Option<Budget>,
}

/// `[budget]`: microseconds of the planner's step per tick, mean and 99th percentile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub plan_tick_mean_us: u64,
    pub plan_tick_p99_us: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerConfig {
    pub simulation_distance: u32,
    pub view_distance: u32,
    pub seed: String,
    pub difficulty: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RaceRules {
    pub bots_per_server: u32,
    pub join_stagger_ms: u64,
    pub cap_ticks: u64,
}

/// Where a start puts the bot: where it spawned, or a dimension and a position.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Place {
    /// `place = "spawn"`.
    Named(String),
    At {
        dimension: String,
        x: f64,
        y: f64,
        z: f64,
    },
}

/// One row of `config/starts.toml`: a kit and a place for a stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Start {
    pub name: String,
    /// The stage this start begins (a `Stage` name).
    pub stage: String,
    pub place: Place,
    /// `[item name, count]` pairs, given over RCON.
    #[serde(default)]
    pub kit: Vec<(String, u32)>,
    /// The cap for this start, in server ticks; the race config's when absent.
    #[serde(default)]
    pub cap_ticks: Option<u64>,
    /// Each bot moved to a random surface spot within this many blocks of where it joined
    /// (vanilla's `spreadplayers`): a lane's bots apart, so they do not dig up each other's tables
    /// and furnaces (issue 13, docs/problems/iron-kit.md wall 4).
    #[serde(default)]
    pub spread: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Starts {
    pub start: Vec<Start>,
}

impl RaceConfig {
    pub fn parse(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| format!("race config: {e}"))
    }

    pub fn read(path: &str) -> Result<(Self, String), String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Ok((Self::parse(&text)?, fnv_hex(text.as_bytes())))
    }
}

impl Starts {
    pub fn parse(text: &str) -> Result<Self, String> {
        let s: Starts = toml::from_str(text).map_err(|e| format!("starts: {e}"))?;
        s.check()?;
        Ok(s)
    }

    pub fn read(path: &str) -> Result<Self, String> {
        Self::parse(&std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?)
    }

    pub fn find(&self, name: &str) -> Option<&Start> {
        self.start.iter().find(|s| s.name == name)
    }

    /// Every stage, item and place named must exist: a typo fails here, not in a race.
    fn check(&self) -> Result<(), String> {
        for s in &self.start {
            Stage::from_name(&s.stage)
                .ok_or_else(|| format!("start {}: no stage {}", s.name, s.stage))?;
            for (item, n) in &s.kit {
                ItemKind::from_name(item)
                    .ok_or_else(|| format!("start {}: no item {item}", s.name))?;
                if *n == 0 {
                    return Err(format!("start {}: {item} count 0", s.name));
                }
            }
            if let Place::Named(p) = &s.place
                && p != "spawn"
            {
                return Err(format!("start {}: no place {p}", s.name));
            }
        }
        Ok(())
    }
}

/// The RCON commands that apply a start to a bot: the kit, then the place.
pub fn start_commands(start: &Start, bot: &str) -> Vec<String> {
    let mut out: Vec<String> = start
        .kit
        .iter()
        .map(|(item, n)| format!("give {bot} minecraft:{item} {n}"))
        .collect();
    if let Place::At { dimension, x, y, z } = &start.place {
        out.push(format!("execute in {dimension} run tp {bot} {x} {y} {z}"));
    }
    if let Some(r) = start.spread {
        out.push(format!(
            "execute at {bot} run spreadplayers ~ ~ 0 {} false {bot}",
            r.max(1)
        ));
    }
    out
}

/// FNV-1a 64 of the config's bytes: which config a batch ran under.
pub fn fnv_hex(bytes: &[u8]) -> String {
    let h = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")
}
