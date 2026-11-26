//! What counts as progress for the race's `NoProgress` end (issue 16e; config/race.toml
//! `no_progress_ticks`). Pure: fed the run's facts and place each tick.
//!
//! Progress is a new stage, an item kind the run has not held before, a goal item's count
//! passing the most the run has held of it (up to the most any goal row asks for), or a 16-block
//! cube not stood in before. It is not "some item count went up": a bot digging in place gains
//! netherrack every few ticks and so never ended (docs/problems/race-tooling.md).

use std::collections::{BTreeMap, BTreeSet};

/// A place is a 16-block cube: entering one not seen before is progress.
pub const REGION_SHIFT: i32 = 4;

/// The cube a position is in.
pub fn region(pos: [f64; 3]) -> (i32, i32, i32) {
    (
        (pos[0].floor() as i32) >> REGION_SHIFT,
        (pos[1].floor() as i32) >> REGION_SHIFT,
        (pos[2].floor() as i32) >> REGION_SHIFT,
    )
}

/// Why a tick counted as progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gain {
    /// The run's first tick.
    Start,
    Stage,
    NewKind,
    GoalItem,
    NewPlace,
    /// A feet biome not stood in before, or a warped forest known (issue 17b).
    Biome,
    /// An enderman killed by the bot or a pearl picked up (issue 18b): a hunt that kills is
    /// going on even when a pearl lost and found again is no more than the most held.
    Hunt,
    /// Issue 19b: an eye thrown or picked up, an end portal frame seen, or a new dimension: the
    /// search for the stronghold goes on while the eyes held stay at most what was held.
    Eyes,
}

/// The run's progress so far.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// Items the goal rows name, with the most any row asks for.
    goal_items: BTreeMap<String, i32>,
    /// The most the run has held of each item.
    most: BTreeMap<String, i32>,
    regions: BTreeSet<(i32, i32, i32)>,
    /// Feet biomes stood in.
    biomes: BTreeSet<String>,
    /// The server tick of the last progress.
    last: Option<u64>,
}

impl Progress {
    pub fn new(goal_items: BTreeMap<String, i32>) -> Self {
        Self {
            goal_items,
            ..Self::default()
        }
    }

    /// One tick: its facts and where the body is. Returns why it was progress, if it was.
    pub fn see(
        &mut self,
        tick: u64,
        facts: &[observe::Stamped],
        place: Option<[f64; 3]>,
    ) -> Option<Gain> {
        let mut gain = self.last.is_none().then_some(Gain::Start);
        for f in facts {
            match &f.fact {
                observe::Fact::Stage { .. } => gain = Some(Gain::Stage),
                // A feet biome not stood in before, or a forest known from blocks seen, is
                // progress (issue 17b): a long heading crosses biomes.
                observe::Fact::Biome { name } => {
                    if self.biomes.insert(name.clone()) {
                        gain = gain.or(Some(Gain::Biome));
                    }
                }
                observe::Fact::ForestSeen { .. } | observe::Fact::SoulValleySeen { .. } => {
                    gain = gain.or(Some(Gain::Biome))
                }
                observe::Fact::EndermanKilled { by_bot: true, .. }
                | observe::Fact::PearlPicked { .. } => gain = gain.or(Some(Gain::Hunt)),
                // Another's kill, a blow, anger, a drop or a taken block is not progress by
                // itself: a hunt that never kills is stuck.
                observe::Fact::EndermanKilled { by_bot: false, .. }
                | observe::Fact::EndermanAngry { .. }
                | observe::Fact::EndermanHit { .. }
                | observe::Fact::PearlDropped { .. }
                | observe::Fact::BlockTaken { .. } => {}
                // An eye thrown or picked up, or a frame seen, is the stronghold search going on
                // (the eyes held fall with each throw, so the item rule never counts them); a
                // flight's end, a shatter or a silverfish is not by itself.
                observe::Fact::EyeThrown { .. }
                | observe::Fact::EyePicked { .. }
                | observe::Fact::FrameSeen { .. }
                | observe::Fact::Dimension { .. } => gain = gain.or(Some(Gain::Eyes)),
                observe::Fact::EyeFlight { .. }
                | observe::Fact::EyeEnded { .. }
                | observe::Fact::SilverfishWoken { .. } => {}
                observe::Fact::Item { item, to, .. } => {
                    let most = self.most.get(item).copied();
                    if most.is_none() && *to > 0 {
                        gain = gain.or(Some(Gain::NewKind));
                    }
                    // More of a goal item than ever held, and not yet all any row asks for.
                    if let (Some(most), Some(asked)) = (most, self.goal_items.get(item))
                        && *to > most
                        && most < *asked
                    {
                        gain = gain.or(Some(Gain::GoalItem));
                    }
                    if *to > most.unwrap_or(0) {
                        self.most.insert(item.clone(), *to);
                    }
                }
                observe::Fact::Health { .. }
                | observe::Fact::Food { .. }
                | observe::Fact::Hit { .. }
                | observe::Fact::Alight { .. }
                | observe::Fact::ToolWorn { .. }
                | observe::Fact::ToolBroke { .. }
                | observe::Fact::ToolGone { .. }
                | observe::Fact::HealthDrop { .. }
                | observe::Fact::Cursor { .. }
                | observe::Fact::Effect { .. }
                | observe::Fact::Sound { .. }
                | observe::Fact::LevelEvent { .. }
                | observe::Fact::Chat { .. }
                | observe::Fact::Appear { .. }
                | observe::Fact::Gone { .. }
                | observe::Fact::Shot { .. }
                | observe::Fact::Landed { .. }
                | observe::Fact::Sign { .. }
                | observe::Fact::Night { .. }
                | observe::Fact::Day { .. }
                | observe::Fact::Enderman { .. }
                | observe::Fact::EndermanStill { .. }
                | observe::Fact::Death
                | observe::Fact::Won => {}
            }
        }
        if place.is_some_and(|p| self.regions.insert(region(p))) {
            gain = gain.or(Some(Gain::NewPlace));
        }
        if gain.is_some() {
            self.last = Some(tick);
        }
        gain
    }

    /// Server ticks since the last progress.
    pub fn idle(&self, tick: u64) -> u64 {
        self.last.map_or(0, |s| tick.saturating_sub(s))
    }
}

/// The items `config/goals.toml`'s rows name (needs, provides, holds, wants), each with the most
/// any row asks for: what `Progress` counts more of as progress.
pub fn goal_items(goals: &plan::make::Goals) -> BTreeMap<String, i32> {
    let mut out: BTreeMap<String, i32> = BTreeMap::new();
    for row in &goals.goal {
        let asked = row
            .needs
            .iter()
            .chain(&row.provides)
            .chain(&row.holds)
            .chain(&row.wants);
        for (key, n) in asked {
            let Some(need) = plan::make::Need::parse(key) else {
                continue;
            };
            for name in need.items.iter().filter_map(|k| k.name()) {
                let most = out.entry(name.to_owned()).or_default();
                *most = (*most).max(*n);
            }
        }
    }
    out
}

/// `[progress] items` of `config/race.toml`: items that count as the goal table's do, for a
/// stage the table has no row for yet.
pub fn race_items(race_toml: &str) -> BTreeMap<String, i32> {
    #[derive(serde::Deserialize)]
    struct File {
        #[serde(default)]
        progress: Section,
    }
    #[derive(serde::Deserialize, Default)]
    struct Section {
        #[serde(default)]
        items: Vec<(String, i32)>,
    }
    toml::from_str::<File>(race_toml)
        .map(|f| f.progress.items.into_iter().collect())
        .unwrap_or_default()
}

/// The goal items of the repository's own files (`config/goals.toml`, `config/race.toml`), read
/// from the working directory as the goal table is; a file that is not there adds nothing.
pub fn items_from_repo() -> BTreeMap<String, i32> {
    let mut out = std::fs::read_to_string("config/goals.toml")
        .ok()
        .and_then(|t| toml::from_str::<plan::make::Goals>(&t).ok())
        .map(|g| goal_items(&g))
        .unwrap_or_default();
    if let Ok(t) = std::fs::read_to_string("config/race.toml") {
        for (name, n) in race_items(&t) {
            let most = out.entry(name).or_default();
            *most = (*most).max(n);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(at: u64, name: &str, from: i32, to: i32) -> observe::Stamped {
        observe::Stamped {
            at,
            fact: observe::Fact::Item {
                item: name.to_owned(),
                from,
                to,
            },
        }
    }

    fn goals() -> BTreeMap<String, i32> {
        BTreeMap::from([("blaze_rod".to_owned(), 7), ("cobblestone".to_owned(), 3)])
    }

    const HERE: Option<[f64; 3]> = Some([22.5, 34.0, -19.5]);

    /// Races 45164371770 to 45204423376: bots dug stairs and tunnels in one cube for hours,
    /// every dig a netherrack more, and the old rule (any count up) never ended them.
    #[test]
    fn digging_in_place_is_not_progress() {
        let mut p = Progress::new(goals());
        assert_eq!(p.see(100, &[], HERE), Some(Gain::Start));
        assert_eq!(
            p.see(101, &[item(101, "netherrack", 0, 1)], HERE),
            Some(Gain::NewKind)
        );
        for n in 1..2000 {
            let tick = 101 + 12 * n as u64;
            assert_eq!(
                p.see(tick, &[item(tick, "netherrack", n, n + 1)], HERE),
                None
            );
        }
        assert_eq!(p.idle(24_101), 24_000);
    }

    #[test]
    fn a_goal_item_over_its_most_is_progress_up_to_what_is_asked() {
        let mut p = Progress::new(goals());
        p.see(0, &[item(0, "blaze_rod", 0, 1)], HERE);
        assert_eq!(
            p.see(10, &[item(10, "blaze_rod", 1, 2)], HERE),
            Some(Gain::GoalItem)
        );
        // Lost and found again: not more than the run has held.
        assert_eq!(p.see(20, &[item(20, "blaze_rod", 2, 1)], HERE), None);
        assert_eq!(p.see(30, &[item(30, "blaze_rod", 1, 2)], HERE), None);
        assert_eq!(
            p.see(40, &[item(40, "blaze_rod", 2, 3)], HERE),
            Some(Gain::GoalItem)
        );
        assert_eq!(p.idle(100), 60);
        // Cobblestone is asked for 3 at most: the fourth and later are a dig in place.
        p.see(200, &[item(200, "cobblestone", 0, 3)], HERE);
        assert_eq!(p.see(210, &[item(210, "cobblestone", 3, 4)], HERE), None);
        assert_eq!(p.see(220, &[item(220, "cobblestone", 4, 60)], HERE), None);
    }

    #[test]
    fn a_stage_and_a_new_cube_are_progress() {
        let mut p = Progress::new(goals());
        p.see(0, &[], HERE);
        assert_eq!(p.see(1, &[], HERE), None);
        let stage = observe::Stamped {
            at: 2,
            fact: observe::Fact::Stage {
                reached: "fortress".to_owned(),
            },
        };
        assert_eq!(p.see(2, &[stage], HERE), Some(Gain::Stage));
        // 15 blocks along the same cube is nothing; the next cube is a new place, once.
        assert_eq!(p.see(3, &[], Some([31.9, 34.0, -19.5])), None);
        assert_eq!(
            p.see(4, &[], Some([32.1, 34.0, -19.5])),
            Some(Gain::NewPlace)
        );
        assert_eq!(p.see(5, &[], HERE), None);
        assert_eq!(p.idle(104), 100);
    }

    #[test]
    fn the_goal_table_names_its_items_with_the_most_asked() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/goals.toml"
        ))
        .unwrap();
        let goals: plan::make::Goals = toml::from_str(&text).unwrap();
        let items = goal_items(&goals);
        assert!(items.get("stick").is_some_and(|n| *n >= 4), "{items:?}");
        assert!(!items.contains_key("netherrack"), "{items:?}");
    }

    /// Issue 16's done-condition is 7 blaze rods and the goal table has no row for them yet.
    #[test]
    fn the_race_config_adds_the_rods() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/race.toml"
        ))
        .unwrap();
        assert_eq!(race_items(&text).get("blaze_rod"), Some(&7));
        assert_eq!(race_items(&text).get("ender_pearl"), Some(&12));
        assert_eq!(race_items(&text).get("ender_eye"), Some(&12));
        assert!(race_items("[race]\ncap_ticks = 1").is_empty());
    }

    /// Issue 19b: an eye thrown or picked up, a frame seen and a dimension entered are progress;
    /// a flight's end, a shatter and a silverfish are not.
    #[test]
    fn an_eye_thrown_or_a_frame_seen_is_progress() {
        let fact = |at, fact| observe::Stamped { at, fact };
        let mut p = Progress::new(BTreeMap::new());
        p.see(100, &[], HERE);
        let quiet = [
            observe::Fact::EyeEnded {
                id: 1,
                dropped: false,
                at: [0.0; 3],
            },
            observe::Fact::EyeFlight {
                id: 1,
                first: [0.0; 3],
                last: [9.0, 0.0, 0.0],
                heading: Some(-90.0),
                samples: 40,
                ticks: 40,
            },
            observe::Fact::SilverfishWoken { id: 2, d: 3.0 },
        ];
        for (i, f) in quiet.into_iter().enumerate() {
            assert_eq!(p.see(101 + i as u64, &[fact(101, f)], HERE), None);
        }
        for (i, f) in [
            observe::Fact::EyeThrown {
                id: 3,
                from: [0.0; 3],
                yaw: None,
            },
            observe::Fact::EyePicked { id: 4, amount: 1 },
            observe::Fact::FrameSeen { cell: [1, 30, 1] },
            observe::Fact::Dimension {
                to: "minecraft:overworld".to_owned(),
                from: Some("minecraft:the_nether".to_owned()),
            },
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                p.see(110 + i as u64, &[fact(110, f)], HERE),
                Some(Gain::Eyes)
            );
        }
    }

    /// Issue 18b: the bot's kill and a pearl picked up are progress; another's kill, anger and a
    /// blow are not; the twelfth pearl counts as a goal item (config/race.toml).
    #[test]
    fn a_kill_or_a_pearl_picked_is_progress() {
        let fact = |at, fact| observe::Stamped { at, fact };
        let mut p = Progress::new(BTreeMap::from([("ender_pearl".to_owned(), 12)]));
        p.see(100, &[], HERE);
        let quiet = [
            observe::Fact::EndermanAngry {
                id: 3,
                d: 12.0,
                ticking: true,
            },
            observe::Fact::EndermanHit {
                id: 3,
                before: 40.0,
                after: 34.0,
            },
            observe::Fact::EndermanKilled {
                id: 4,
                by_bot: false,
            },
        ];
        for (i, f) in quiet.into_iter().enumerate() {
            assert_eq!(p.see(101 + i as u64, &[fact(101, f)], HERE), None);
        }
        let kill = observe::Fact::EndermanKilled {
            id: 3,
            by_bot: true,
        };
        assert_eq!(p.see(110, &[fact(110, kill)], HERE), Some(Gain::Hunt));
        let picked = observe::Fact::PearlPicked { id: 9, amount: 1 };
        assert_eq!(p.see(120, &[fact(120, picked)], HERE), Some(Gain::Hunt));
        p.see(130, &[item(130, "ender_pearl", 0, 1)], HERE);
        assert_eq!(
            p.see(140, &[item(140, "ender_pearl", 1, 2)], HERE),
            Some(Gain::GoalItem)
        );
    }

    /// Issue 17b: a feet biome not stood in before is progress, the same one again is not, and
    /// an enderman seen is not.
    #[test]
    fn a_new_feet_biome_is_progress() {
        let fact = |at, fact| observe::Stamped { at, fact };
        let biome = |name: &str| observe::Fact::Biome {
            name: name.to_owned(),
        };
        let mut p = Progress::new(goals());
        p.see(100, &[], HERE);
        assert_eq!(
            p.see(101, &[fact(101, biome("soul_sand_valley"))], HERE),
            Some(Gain::Biome)
        );
        assert_eq!(
            p.see(102, &[fact(102, biome("nether_wastes"))], HERE),
            Some(Gain::Biome)
        );
        assert_eq!(
            p.see(103, &[fact(103, biome("soul_sand_valley"))], HERE),
            None
        );
        let seen = observe::Fact::Enderman {
            id: 5,
            d: 40.0,
            ticking: false,
        };
        assert_eq!(p.see(104, &[fact(104, seen)], HERE), None);
        let forest = observe::Fact::ForestSeen { column: (30, -8) };
        assert_eq!(p.see(105, &[fact(105, forest)], HERE), Some(Gain::Biome));
    }
}
