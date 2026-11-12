//! Issue 07a: case and batch files round-trip, the starts file is valid, the summary folds k/n.

use std::collections::BTreeMap;

use results::config::{RaceConfig, Starts, start_commands};
use results::{Batch, Case, Coverage, Doing, Ending, FORMAT_VERSION, Row, Source, summarize};
use world::Stage;

fn root(p: &str) -> String {
    format!("{}/../../{p}", env!("CARGO_MANIFEST_DIR"))
}

fn case(bot: &str, stage: Option<Stage>, ending: Ending) -> Case {
    Case {
        format_version: FORMAT_VERSION,
        run: "local-1".to_owned(),
        bot: bot.to_owned(),
        commit: "abc1234".to_owned(),
        lane: 0,
        start: "spawn".to_owned(),
        source: Source::Real,
        stage_reached: stage,
        ending,
        death_text: None,
        first_hit: None,
        hits: Vec::new(),
        doing: Doing {
            task: "idle".to_owned(),
            why: "no planner yet".to_owned(),
        },
        items_gained: BTreeMap::new(),
        items_lost: BTreeMap::new(),
        swings_sent: 0,
        swings_landed: 0,
        health: Some(20.0),
        pos: Some([0.5, 64.0, 0.5]),
        dimension: Some("minecraft:overworld".to_owned()),
        server_ticks: 6000,
        wall_ms: 300_000,
        recording: Some(format!("{bot}.rsr")),
        replay: format!("cargo run -p replay -- verify {bot}.rsr"),
    }
}

fn batch(rows: Vec<Case>) -> Batch {
    Batch {
        format_version: FORMAT_VERSION,
        run: "local-1".to_owned(),
        commit: "abc1234".to_owned(),
        lane: 0,
        start: "spawn".to_owned(),
        source: Source::Real,
        config_hash: "0".to_owned(),
        bots: u32::try_from(rows.len()).unwrap(),
        server: "127.0.0.1:25565".to_owned(),
        server_tick_rate: 20.0,
        coverage: Coverage {
            runs: rows.len() as u64,
            cases: rows.len() as u64,
            recordings: rows.len() as u64,
            ..Coverage::default()
        },
        rows: rows
            .iter()
            .map(|c| Row {
                bot: c.bot.clone(),
                stage_reached: c.stage_reached,
                ending: c.ending.clone(),
                signature: c.signature(),
                server_ticks: c.server_ticks,
                wall_ms: c.wall_ms,
                case: Some(format!("{}.case.rsc", c.bot)),
                recording: c.recording.clone(),
            })
            .collect(),
    }
}

#[test]
fn case_and_batch_files_round_trip() {
    let dir = std::env::temp_dir().join(format!("results-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let c = case(
        "rs-001",
        Some(Stage::InWorld),
        Ending::Death {
            killer: "death.attack.mob rs-001 entity.minecraft.zombie".to_owned(),
        },
    );
    results::write(&dir.join("c.case.rsc"), &c).unwrap();
    assert_eq!(results::read_case(&dir.join("c.case.rsc")).unwrap(), c);
    let b = batch(vec![c]);
    results::write(&dir.join("batch.rsb"), &b).unwrap();
    assert_eq!(results::read_batch(&dir.join("batch.rsb")).unwrap(), b);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_summary_counts_reached_and_skips_cut() {
    let b = batch(vec![
        case("a", Some(Stage::Table), Ending::Cap),
        case("b", Some(Stage::InWorld), Ending::Cap),
        case(
            "c",
            None,
            Ending::Lost {
                why: "eof".to_owned(),
            },
        ),
        case("d", Some(Stage::End), Ending::Cut),
    ]);
    let s = summarize(&[b]);
    let g = &s[&("abc1234".to_owned(), "spawn".to_owned(), Source::Real)];
    assert_eq!((g[&Stage::InWorld].k, g[&Stage::InWorld].n), (2, 3));
    assert_eq!((g[&Stage::Log].k, g[&Stage::Log].n), (1, 3));
    assert_eq!((g[&Stage::Table].k, g[&Stage::Table].n), (1, 3));
    assert_eq!(g[&Stage::StonePickaxe].k, 0);
}

#[test]
fn the_committed_config_and_starts_are_valid() {
    let (config, hash) = RaceConfig::read(&root("config/race.toml")).unwrap();
    assert_eq!(config.server.simulation_distance, 3);
    assert_eq!(hash.len(), 16);
    let starts = Starts::read(&root("config/starts.toml")).unwrap();
    let table = starts.find("table").unwrap();
    assert_eq!(
        start_commands(table, "rs-001"),
        [
            "give rs-001 minecraft:crafting_table 1",
            "give rs-001 minecraft:oak_log 4"
        ]
    );
    assert!(starts.find("spawn").is_some());
}

#[test]
fn a_typo_in_a_start_is_refused() {
    let bad = "[[start]]\nname = \"x\"\nstage = \"tabel\"\nplace = \"spawn\"\n";
    assert!(Starts::parse(bad).is_err());
    let bad = "[[start]]\nname = \"x\"\nstage = \"table\"\nplace = \"spawn\"\nkit = [[\"oak_logg\", 1]]\n";
    assert!(Starts::parse(bad).is_err());
}
