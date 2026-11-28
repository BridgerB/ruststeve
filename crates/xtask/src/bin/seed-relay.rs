//! The seeds and the relay of race.yml (issue 69, `brain/ruststeve/design/relay.md`): which lanes
//! race which seed from which snapshot, and after a leg, how far each seed has come. The
//! workflow's shell only moves files; every choice is made here.
//!
//! ```text
//! seed-relay lanes  --names "<xtask seeds>" --seed <''|name[,name]|all> --lanes <per seed>
//!                   --start <start> --snapshot <''|frontier|tag/asset> [--from <dir>]
//!                   [--config <dir>] --lanes-toml <path>
//!     prints `matrix=<json>` and `total=<n>` (for $GITHUB_OUTPUT) and writes lanes.toml
//! seed-relay record --names "<xtask seeds>" --seed <...> --run <run id> --leg <n> --legs <max>
//!                   --snaps <dir of lane dirs> [--from <dir>] --out <dir> --output <path>
//!     writes relay-<seed>.toml per seed into --out, appends `next=true|false` to --output and
//!     prints the leg's table as markdown
//! ```
//!
//! `--names` is `xtask seeds`' output (config/seeds.toml's names in file order); its first name is
//! config/race.toml's seed (a test of config/seeds.toml holds it), the one that keeps
//! config/frontier.toml and the typecraft starts. `--from` is the previous leg's
//! `relay-<seed>.toml` files, downloaded from its release.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::{Deserialize, Serialize};
use world::Stage;

/// The most lanes one race may hold: the 20 runners (AGENTS.md section 2).
const MAX_LANES: usize = 20;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("lanes") => flags(&args[1..]).and_then(|f| lanes_cmd(&f)),
        Some("record") => flags(&args[1..]).and_then(|f| record_cmd(&f)),
        other => Err(format!("unknown task {other:?}; tasks: lanes, record")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("seed-relay: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `--flag value` pairs.
fn flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let Some(name) = flag.strip_prefix("--") else {
            return Err(format!("expected a --flag, got {flag:?}"));
        };
        let Some(value) = it.next() else {
            return Err(format!("--{name} needs a value"));
        };
        out.insert(name.to_owned(), value.clone());
    }
    Ok(out)
}

fn need<'a>(f: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    f.get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("--{name} is required"))
}

fn number(f: &BTreeMap<String, String>, name: &str) -> Result<usize, String> {
    let v = need(f, name)?;
    v.parse()
        .map_err(|_| format!("--{name} must be a whole number, got {v:?}"))
}

/// The names `xtask seeds` printed, one a line (or space-separated).
fn names(f: &BTreeMap<String, String>) -> Result<Vec<String>, String> {
    let names: Vec<String> = need(f, "names")?
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    if names.is_empty() {
        return Err("--names is empty (xtask seeds printed nothing)".to_owned());
    }
    Ok(names)
}

/// The seeds a `seed` input names: empty is the race seed (today's race), `all` every seed, else
/// a name or a comma-separated list of names, each in config/seeds.toml.
fn select(names: &[String], input: &str) -> Result<Vec<String>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(names.iter().take(1).cloned().collect());
    }
    if input == "all" {
        return Ok(names.to_vec());
    }
    let mut out: Vec<String> = Vec::new();
    for name in input.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        if !names.iter().any(|n| n == name) {
            return Err(format!(
                "seed {name:?} is not in config/seeds.toml ({})",
                names.join(", ")
            ));
        }
        if !out.iter().any(|n| n == name) {
            out.push(name.to_owned());
        }
    }
    if out.is_empty() {
        return Err(format!("seed input {input:?} names no seed"));
    }
    Ok(out)
}

/// A frontier file's keys (config/frontier.toml's shape).
#[derive(Debug, Deserialize)]
struct Frontier {
    tag: String,
    asset: String,
}

/// The seed's frontier file: config/frontier.toml for the race seed (the issue-20 lineage), else
/// config/frontier/<seed>.toml.
fn frontier_path(config: &Path, race_seed: &str, seed: &str) -> PathBuf {
    if seed == race_seed {
        config.join("frontier.toml")
    } else {
        config.join("frontier").join(format!("{seed}.toml"))
    }
}

/// The seed's frontier as `tag/asset`, or none when the seed has no frontier file yet (it starts
/// from spawn). The race seed always has one.
fn frontier(config: &Path, race_seed: &str, seed: &str) -> Result<Option<String>, String> {
    let path = frontier_path(config, race_seed, seed);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && seed != race_seed => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    let f: Frontier = toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(format!("{}/{}", f.tag, f.asset)))
}

/// One leg's `relay-<seed>.toml` (the brief's shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Relay {
    seed: String,
    /// The furthest stage so far (a `Stage` name), or `spawn` before any snapshot.
    stage: String,
    /// The release and asset of that stage's snapshot; empty at `spawn`.
    #[serde(default)]
    tag: String,
    #[serde(default)]
    asset: String,
    /// This leg added no stage: the seed leaves the relay, and `stage` is its stall.
    stalled: bool,
    /// `stage` is the ladder's last: the seed leaves the relay won.
    #[serde(default)]
    won: bool,
    /// Every leg's release so far, in order.
    legs: Vec<String>,
}

impl Relay {
    fn done(&self) -> bool {
        self.stalled || self.won
    }

    fn snapshot(&self) -> Option<String> {
        (!self.tag.is_empty() && !self.asset.is_empty())
            .then(|| format!("{}/{}", self.tag, self.asset))
    }
}

fn relay_file(dir: &Path, seed: &str) -> PathBuf {
    dir.join(format!("relay-{seed}.toml"))
}

/// The previous leg's file for `seed`, when there is a previous leg.
fn previous(from: Option<&Path>, seed: &str) -> Result<Option<Relay>, String> {
    let Some(dir) = from else { return Ok(None) };
    let path = relay_file(dir, seed);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "{}: {e} (relay_from names a release without this seed)",
            path.display()
        )
    })?;
    toml::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Where a lane's snapshot comes from: the restore step refuses a development or a not-a-stage
/// snapshot when it is the frontier's or the relay's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Source {
    /// No snapshot: the start's own place.
    None,
    Frontier,
    Relay,
    /// A `tag/asset` given in the dispatch.
    Given,
}

impl Source {
    fn name(self) -> &'static str {
        match self {
            Source::None => "",
            Source::Frontier => "frontier",
            Source::Relay => "relay",
            Source::Given => "given",
        }
    }
}

/// One lane of the race: a runner, a seed, a start and maybe a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize)]
struct Lane {
    lane: usize,
    seed: String,
    start: String,
    /// `tag/asset`, or empty.
    snapshot: String,
    source: Source,
}

#[derive(Serialize)]
struct LanesToml<'a> {
    lane: &'a [Lane],
}

/// What a dispatch asks for.
struct Ask<'a> {
    names: &'a [String],
    seed: &'a str,
    lanes: usize,
    start: &'a str,
    snapshot: &'a str,
}

/// The race's lanes: each seed still racing gets `lanes` lanes, numbered on through the seeds so
/// every lane (and asset) name stays unique.
fn plan(ask: &Ask, config: &Path, from: Option<&Path>) -> Result<Vec<Lane>, String> {
    let race_seed = ask.names.first().map_or("", String::as_str);
    let seeds = select(ask.names, ask.seed)?;
    let snapshot = ask.snapshot.trim();
    if snapshot.contains('/') && seeds.len() > 1 {
        return Err(format!(
            "snapshot {snapshot} is one world; it cannot start {} seeds",
            seeds.len()
        ));
    }
    let mut out = Vec::new();
    for seed in &seeds {
        let (snap, source) = match previous(from, seed)? {
            Some(r) if r.done() => {
                eprintln!(
                    "{seed}: left the relay at {} ({})",
                    r.stage,
                    if r.won { "won" } else { "stalled" }
                );
                continue;
            }
            Some(r) => match r.snapshot() {
                Some(s) => (Some(s), Source::Relay),
                None => (None, Source::None),
            },
            None if snapshot == "frontier" => match frontier(config, race_seed, seed)? {
                Some(s) => (Some(s), Source::Frontier),
                None => {
                    eprintln!("{seed}: no frontier file yet; it starts from spawn");
                    (None, Source::None)
                }
            },
            None if snapshot.is_empty() => (None, Source::None),
            None => (Some(snapshot.to_owned()), Source::Given),
        };
        // A relay leg resumes with the snapshot start; a seed with no snapshot after all starts
        // at spawn, never at the snapshot start's empty place.
        let start = match (&snap, source) {
            (Some(_), Source::Relay) => "snapshot",
            (None, _) if ask.start == "snapshot" => "spawn",
            _ => ask.start,
        };
        // Every other start is a place on the race seed's world (config/starts.toml).
        if seed != race_seed && snap.is_none() && start != "spawn" {
            return Err(format!(
                "seed {seed}: start {start} is a place on the {race_seed} world; \
                 another seed takes spawn or a snapshot"
            ));
        }
        for _ in 0..ask.lanes {
            out.push(Lane {
                lane: out.len(),
                seed: seed.clone(),
                start: start.to_owned(),
                snapshot: snap.clone().unwrap_or_default(),
                source,
            });
        }
    }
    if out.is_empty() {
        return Err("no seed is left to race".to_owned());
    }
    if out.len() > MAX_LANES {
        return Err(format!(
            "{} lanes ({} per seed) is over the {MAX_LANES} runners",
            out.len(),
            ask.lanes
        ));
    }
    Ok(out)
}

/// A JSON string (the matrix is GitHub's format, not a recording).
fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The lanes as a `strategy.matrix.include` list.
fn matrix(lanes: &[Lane]) -> String {
    let rows: Vec<String> = lanes
        .iter()
        .map(|l| {
            format!(
                "{{\"lane\":{},\"seed\":{},\"start\":{},\"snapshot\":{},\"source\":{}}}",
                l.lane,
                json_str(&l.seed),
                json_str(&l.start),
                json_str(&l.snapshot),
                json_str(l.source.name())
            )
        })
        .collect();
    format!("[{}]", rows.join(","))
}

fn lanes_cmd(f: &BTreeMap<String, String>) -> Result<(), String> {
    let names = names(f)?;
    let lanes = number(f, "lanes")?;
    if lanes == 0 {
        return Err("lanes must be at least 1".to_owned());
    }
    let ask = Ask {
        names: &names,
        seed: f.get("seed").map_or("", String::as_str),
        lanes,
        start: need(f, "start")?,
        snapshot: f.get("snapshot").map_or("", String::as_str),
    };
    let config = PathBuf::from(f.get("config").map_or("config", String::as_str));
    let from = f.get("from").filter(|d| !d.is_empty()).map(PathBuf::from);
    let plan = plan(&ask, &config, from.as_deref())?;
    let text = toml::to_string(&LanesToml { lane: &plan }).map_err(|e| e.to_string())?;
    let path = need(f, "lanes-toml")?;
    std::fs::write(path, text).map_err(|e| format!("{path}: {e}"))?;
    for l in &plan {
        let snap = if l.snapshot.is_empty() {
            "no snapshot"
        } else {
            &l.snapshot
        };
        eprintln!(
            "lane {}: seed {}, start {}, {snap} {}",
            l.lane,
            l.seed,
            l.start,
            l.source.name()
        );
    }
    println!("matrix={}", matrix(&plan));
    println!("total={}", plan.len());
    Ok(())
}

/// The keys of a `snap.toml` the relay reads. `seed` is prepended by race.yml; `dev` and `kind`
/// by `bot::devstart`.
#[derive(Debug, Deserialize)]
struct Snap {
    #[serde(default)]
    seed: Option<String>,
    #[serde(default)]
    dev: bool,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    server_ticks: u64,
}

/// A stage's snapshot a lane of this leg published.
#[derive(Clone, Debug, PartialEq)]
struct Found {
    stage: Stage,
    /// The lane's artifact name, `race-<run>-lane-<n>`: the asset names it.
    src: String,
    server_ticks: u64,
}

/// Every stage snapshot of `seed` under `<snaps>/<src>/snapshots/<stage>/snap.toml`: neither a
/// development one (never a lineage's) nor one on request or of a stuck spot (not a stage's).
fn found(snaps: &Path, seed: &str) -> Result<Vec<Found>, String> {
    let mut out = Vec::new();
    let Ok(srcs) = std::fs::read_dir(snaps) else {
        eprintln!("{}: no snapshots in this leg", snaps.display());
        return Ok(out);
    };
    let mut srcs: Vec<PathBuf> = srcs.filter_map(|e| e.ok().map(|e| e.path())).collect();
    srcs.sort();
    for src in srcs {
        let Ok(dirs) = std::fs::read_dir(src.join("snapshots")) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = dirs.filter_map(|e| e.ok().map(|e| e.path())).collect();
        dirs.sort();
        for dir in dirs {
            let path = dir.join("snap.toml");
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let snap: Snap =
                toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let src_name = src.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if snap.seed.as_deref() != Some(seed) {
                continue;
            }
            if snap.dev || snap.kind.is_some() {
                eprintln!("{seed}: {src_name}/{name} is not a lineage's stage snapshot: skipped");
                continue;
            }
            let Some(stage) = Stage::from_name(name) else {
                eprintln!("{seed}: {src_name}/{name}: not a stage name: skipped");
                continue;
            };
            out.push(Found {
                stage,
                src: src_name.to_owned(),
                server_ticks: snap.server_ticks,
            });
        }
    }
    Ok(out)
}

/// The leg's best: the furthest stage; between equals the one reached in fewer server ticks, then
/// the lowest lane, so a rerun picks the same.
fn best(found: &[Found]) -> Option<&Found> {
    found.iter().min_by(|a, b| {
        b.stage
            .cmp(&a.stage)
            .then(a.server_ticks.cmp(&b.server_ticks))
            .then(a.src.cmp(&b.src))
    })
}

/// The seed after this leg: a stage further than the last leg's moves it on; none, and it has
/// stalled. A seed already out is carried as it was.
fn advance(seed: &str, prev: Option<Relay>, found: &[Found], run: &str) -> Relay {
    let tag = format!("race-{run}");
    let prev = prev.unwrap_or_else(|| Relay {
        seed: seed.to_owned(),
        stage: "spawn".to_owned(),
        tag: String::new(),
        asset: String::new(),
        stalled: false,
        won: false,
        legs: Vec::new(),
    });
    if prev.done() {
        return prev;
    }
    let mut legs = prev.legs.clone();
    legs.push(tag.clone());
    let before = Stage::from_name(&prev.stage);
    match best(found) {
        Some(b) if before.is_none_or(|s| b.stage > s) => Relay {
            seed: seed.to_owned(),
            stage: b.stage.name().to_owned(),
            asset: format!("snapshot-{}-{run}-{}.tar.zst", b.stage.name(), b.src),
            tag,
            stalled: false,
            won: b.stage == Stage::Win,
            legs,
        },
        _ => Relay {
            stalled: true,
            legs,
            ..prev
        },
    }
}

fn record_cmd(f: &BTreeMap<String, String>) -> Result<(), String> {
    let names = names(f)?;
    let seeds = select(&names, f.get("seed").map_or("", String::as_str))?;
    let run = need(f, "run")?;
    let leg = number(f, "leg")?;
    let legs = number(f, "legs")?;
    let snaps = PathBuf::from(need(f, "snaps")?);
    let from = f.get("from").filter(|d| !d.is_empty()).map(PathBuf::from);
    let out = PathBuf::from(need(f, "out")?);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut rows = Vec::new();
    for seed in &seeds {
        let prev = previous(from.as_deref(), seed)?;
        let was_out = prev.as_ref().is_some_and(Relay::done);
        let r = advance(seed, prev, &found(&snaps, seed)?, run);
        let text = toml::to_string(&r).map_err(|e| e.to_string())?;
        let path = relay_file(&out, seed);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
        rows.push((r, was_out));
    }
    let going = rows.iter().any(|(r, _)| !r.done());
    let next = going && leg < legs;
    println!("## relay leg {leg} of {legs} (race-{run})\n");
    println!("| seed | furthest stage | snapshot | legs | state |");
    println!("|---|---|---|---|---|");
    for (r, was_out) in &rows {
        let state = match (was_out, r.won, r.stalled) {
            (true, _, _) => "out before this leg",
            (false, true, _) => "won",
            (false, false, true) => "stalled here",
            (false, false, false) => "moved on",
        };
        let snap = r.snapshot().unwrap_or_else(|| "spawn".to_owned());
        println!(
            "| {} | {} | {snap} | {} | {state} |",
            r.seed,
            r.stage,
            r.legs.len()
        );
    }
    println!();
    println!(
        "{}",
        match (going, next) {
            (_, true) => format!("Leg {} follows.", leg + 1),
            (true, false) => format!("The legs limit ({legs}) ends the relay."),
            (false, _) => "Every seed has won or stalled: the relay ends.".to_owned(),
        }
    );
    let output = need(f, "output")?;
    let line = format!("next={next}\n");
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()))
        .map_err(|e| format!("{output}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["typecraft", "ocean", "desert"].map(str::to_owned).to_vec()
    }

    fn ask<'a>(names: &'a [String], seed: &'a str, start: &'a str, snapshot: &'a str) -> Ask<'a> {
        Ask {
            names,
            seed,
            lanes: 2,
            start,
            snapshot,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("seed-relay-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("scratch dir");
        d
    }

    #[test]
    fn the_seed_input_selects() {
        let n = names();
        assert_eq!(select(&n, "").expect("empty"), ["typecraft"]);
        assert_eq!(select(&n, "all").expect("all"), n);
        assert_eq!(select(&n, "desert").expect("one"), ["desert"]);
        assert_eq!(
            select(&n, "ocean, typecraft,ocean").expect("list"),
            ["ocean", "typecraft"]
        );
        assert!(select(&n, "moon").is_err());
        assert!(select(&n, ",").is_err());
    }

    /// Today's race: no seed, no snapshot, one seed's lanes numbered from 0.
    #[test]
    fn an_empty_seed_is_todays_race() {
        let n = names();
        let p = plan(&ask(&n, "", "spawn", ""), Path::new("config"), None).expect("plan");
        assert_eq!(p.len(), 2);
        assert!(
            p.iter()
                .all(|l| l.seed == "typecraft" && l.start == "spawn")
        );
        assert_eq!(p.iter().map(|l| l.lane).collect::<Vec<_>>(), [0, 1]);
    }

    /// Lanes run on through the seeds; another seed with no frontier file starts at spawn.
    #[test]
    fn seeds_times_lanes_and_per_seed_frontiers() {
        let n = names();
        let cfg = scratch("frontier");
        std::fs::write(
            cfg.join("frontier.toml"),
            "stage = \"win\"\ntag = \"race-1\"\nasset = \"snapshot-win-1-race-1-lane-0.tar.zst\"\n",
        )
        .expect("write");
        std::fs::create_dir_all(cfg.join("frontier")).expect("dir");
        std::fs::write(
            cfg.join("frontier/ocean.toml"),
            "stage = \"log\"\ntag = \"race-2\"\nasset = \"snapshot-log-2-race-2-lane-3.tar.zst\"\n",
        )
        .expect("write");
        let p = plan(&ask(&n, "all", "snapshot", "frontier"), &cfg, None).expect("plan");
        assert_eq!(p.len(), 6);
        assert_eq!(p[5].lane, 5);
        assert_eq!(p[0].snapshot, "race-1/snapshot-win-1-race-1-lane-0.tar.zst");
        assert_eq!(p[2].snapshot, "race-2/snapshot-log-2-race-2-lane-3.tar.zst");
        assert_eq!(p[2].source, Source::Frontier);
        assert_eq!(
            (p[4].seed.as_str(), p[4].start.as_str()),
            ("desert", "spawn")
        );
        assert_eq!(p[4].source, Source::None);
    }

    #[test]
    fn a_typecraft_place_is_refused_on_another_seed() {
        let n = names();
        assert!(plan(&ask(&n, "ocean", "nether", ""), Path::new("config"), None).is_err());
        assert!(plan(&ask(&n, "", "nether", ""), Path::new("config"), None).is_ok());
        // A snapshot given by tag is one world: one seed only.
        assert!(
            plan(
                &ask(&n, "all", "snapshot", "race-1/a.tar.zst"),
                Path::new("x"),
                None
            )
            .is_err()
        );
        let mut a = ask(&n, "all", "spawn", "");
        a.lanes = 7;
        assert!(plan(&a, Path::new("config"), None).is_err());
    }

    #[test]
    fn the_committed_frontier_is_the_race_seeds() {
        let f = frontier(Path::new("../../config"), "typecraft", "typecraft").expect("read");
        assert!(f.is_some_and(|s| s.starts_with("race-")));
    }

    fn relay(seed: &str, stage: &str, stalled: bool) -> Relay {
        Relay {
            seed: seed.to_owned(),
            stage: stage.to_owned(),
            tag: "race-1".to_owned(),
            asset: format!("snapshot-{stage}-1-race-1-lane-0.tar.zst"),
            stalled,
            won: stage == "win",
            legs: vec!["race-1".to_owned()],
        }
    }

    /// A relay leg resumes each seed still going from its own snapshot and leaves out the rest.
    #[test]
    fn a_relay_leg_resumes_and_leaves_out() {
        let n = names();
        let from = scratch("from");
        for r in [
            relay("typecraft", "win", false),
            relay("ocean", "nether", false),
            relay("desert", "log", true),
        ] {
            std::fs::write(
                relay_file(&from, &r.seed),
                toml::to_string(&r).expect("toml"),
            )
            .expect("write");
        }
        let p = plan(&ask(&n, "all", "spawn", ""), Path::new("x"), Some(&from)).expect("plan");
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|l| l.seed == "ocean"
            && l.start == "snapshot"
            && l.source == Source::Relay
            && l.snapshot == "race-1/snapshot-nether-1-race-1-lane-0.tar.zst"));
        assert!(plan(&ask(&n, "desert", "spawn", ""), Path::new("x"), Some(&from)).is_err());
    }

    fn found_at(stage: Stage, lane: u32, ticks: u64) -> Found {
        Found {
            stage,
            src: format!("race-9-lane-{lane}"),
            server_ticks: ticks,
        }
    }

    #[test]
    fn the_furthest_stage_then_the_quickest() {
        let f = [
            found_at(Stage::Log, 0, 10),
            found_at(Stage::Nether, 2, 900),
            found_at(Stage::Nether, 1, 500),
            found_at(Stage::Table, 3, 50),
        ];
        assert_eq!(best(&f), Some(&f[2]));
        assert_eq!(best(&[]), None);
    }

    #[test]
    fn a_leg_moves_on_stalls_or_wins() {
        let moved = advance(
            "ocean",
            Some(relay("ocean", "log", false)),
            &[found_at(Stage::Nether, 1, 5)],
            "9",
        );
        assert_eq!(moved.stage, "nether");
        assert_eq!(moved.tag, "race-9");
        assert_eq!(moved.asset, "snapshot-nether-9-race-9-lane-1.tar.zst");
        assert_eq!(moved.legs, ["race-1", "race-9"]);
        assert!(!moved.stalled && !moved.won);

        // The same stage again is no stage added: stalled, the old snapshot kept.
        let stalled = advance(
            "ocean",
            Some(relay("ocean", "nether", false)),
            &[found_at(Stage::Nether, 1, 5)],
            "9",
        );
        assert!(stalled.stalled);
        assert_eq!(stalled.tag, "race-1");
        assert_eq!(stalled.legs.len(), 2);

        // A first leg with no snapshot stalls at spawn, with no snapshot to name.
        let none = advance("desert", None, &[], "9");
        assert_eq!((none.stage.as_str(), none.stalled), ("spawn", true));
        assert_eq!(none.snapshot(), None);

        let won = advance("typecraft", None, &[found_at(Stage::Win, 0, 1)], "9");
        assert!(won.won && won.done());

        // A seed already out is carried unchanged.
        let out = relay("desert", "log", true);
        assert_eq!(advance("desert", Some(out.clone()), &[], "9"), out);
    }

    /// Only the seed's own stage snapshots count: another seed's, a development one, one on
    /// request and a directory that is not a stage are left out.
    #[test]
    fn only_a_seeds_own_stage_snapshots_count() {
        let snaps = scratch("snaps");
        let put = |src: &str, dir: &str, text: &str| {
            let d = snaps.join(src).join("snapshots").join(dir);
            std::fs::create_dir_all(&d).expect("dir");
            std::fs::write(d.join("snap.toml"), text).expect("write");
        };
        let snap = |extra: &str, seed: &str| {
            format!(
                "{extra}seed = \"{seed}\"\nstage = \"x\"\nserver_ticks = 7\n[items]\nstick = 1\n"
            )
        };
        put("race-9-lane-0", "nether", &snap("", "ocean"));
        put("race-9-lane-1", "end", &snap("", "typecraft"));
        put("race-9-lane-0", "fortress", &snap("dev = true\n", "ocean"));
        put("race-9-lane-0", "at-100", &snap("kind = \"at\"\n", "ocean"));
        put("race-9-lane-0", "nonsense", &snap("", "ocean"));
        let f = found(&snaps, "ocean").expect("found");
        assert_eq!(f, [found_at(Stage::Nether, 0, 7)]);
        assert!(
            found(&snaps.join("missing"), "ocean")
                .expect("none")
                .is_empty()
        );
    }

    #[test]
    fn the_matrix_is_json() {
        let l = Lane {
            lane: 3,
            seed: "ocean".to_owned(),
            start: "spawn".to_owned(),
            snapshot: String::new(),
            source: Source::None,
        };
        assert_eq!(
            matrix(&[l]),
            "[{\"lane\":3,\"seed\":\"ocean\",\"start\":\"spawn\",\"snapshot\":\"\",\"source\":\"\"}]"
        );
        assert_eq!(json_str("a\"b\\"), "\"a\\\"b\\\\\"");
    }
}
