//! Repository automation, run in CI and by the git hooks (flake.nix).
//!
//! ```text
//! cargo run -q -p xtask -- deps-boundary       the core crates depend on nothing that does I/O
//! cargo run -q -p xtask -- commit-msg <file>   a commit message keeps the repository's rules
//! cargo run -q -p xtask -- server [--dir target/server] [--port 25565] [--jar target/datagen/server.jar] [--difficulty peaceful] [--config config/race.toml] [--rcon-port 25575] [--view-distance N] [--seed VALUE]
//!                                            a local vanilla 26.1.2 server (offline, simulation distance 3)
//! cargo run -q -p xtask -- seeds [--file config/seeds.toml]
//!                                            the seeds' names, one a line, in file order (the relay's matrix)
//! cargo run -q -p xtask -- seed <name> [--file config/seeds.toml]
//!                                            that seed's level-seed value; exit 1 when the name is unknown
//! ```
//!
//! Standard library only, like `brain-check`: it must build in seconds.

use std::path::Path;
use std::process::{Command, ExitCode};

/// The crates that hold the deterministic core: no I/O, no clock, no async runtime
/// (brain/ruststeve/design/workspace-crates.md).
const CORE_CRATES: [&str; 7] = [
    "data", "protocol", "world", "physics", "memory", "plan", "sim",
];

/// Crates a core crate must never depend on, directly or through another crate: async runtimes and
/// I/O libraries, and our own crates that touch sockets, files or the insight store.
const FORBIDDEN: [&str; 14] = [
    "tokio",
    "mio",
    "reqwest",
    "surrealdb",
    "net",
    "rcon",
    "record",
    "ingest",
    "results",
    "stream",
    "bot",
    "tungstenite",
    "async-std",
    "rmcp",
];

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("deps-boundary") => deps_boundary(),
        Some("commit-msg") => match std::env::args().nth(2) {
            Some(file) => commit_msg(Path::new(&file)),
            None => {
                eprintln!("usage: xtask commit-msg <message file>");
                ExitCode::FAILURE
            }
        },
        Some("server") => server(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("seeds") => seeds(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("seed") => seed(&std::env::args().skip(2).collect::<Vec<_>>()),
        other => {
            eprintln!(
                "unknown task {other:?}; tasks: deps-boundary, commit-msg, server, seeds, seed"
            );
            ExitCode::FAILURE
        }
    }
}

/// The crate name a `cargo tree --prefix none` line names: its first word.
fn crate_name(line: &str) -> &str {
    line.split_whitespace().next().unwrap_or("")
}

fn deps_boundary() -> ExitCode {
    let crates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let present: Vec<&str> = CORE_CRATES
        .iter()
        .copied()
        .filter(|c| crates_dir.join(c).join("Cargo.toml").exists())
        .collect();
    let mut violations = Vec::new();
    for krate in &present {
        let output = Command::new("cargo")
            .args([
                "tree", "--locked", "-p", krate, "-e", "normal", "--prefix", "none",
            ])
            .output();
        let output = match output {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                eprintln!(
                    "cargo tree -p {krate} failed: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("cannot run cargo tree: {e}");
                return ExitCode::FAILURE;
            }
        };
        let tree = String::from_utf8_lossy(&output.stdout);
        for dep in tree
            .lines()
            .map(crate_name)
            .filter(|d| FORBIDDEN.contains(d))
        {
            violations.push(format!("{krate} depends on {dep}"));
        }
    }
    println!(
        "core crates checked: {} ({})",
        present.len(),
        present.join(", ")
    );
    if violations.is_empty() {
        println!("no forbidden dependencies");
        ExitCode::SUCCESS
    } else {
        violations.sort();
        violations.dedup();
        for v in &violations {
            println!("  {v}");
        }
        ExitCode::FAILURE
    }
}

/// Text that marks AI attribution, matched case-insensitively. Bridger's absolute rule: none in
/// any commit, whoever or whatever writes it (AGENTS.md, section 7).
const ATTRIBUTION: [&str; 7] = [
    "co-authored-by: claude",
    "noreply@anthropic.com",
    "claude-session:",
    "claude.ai/code",
    "claude.com/claude-code",
    "generated with claude",
    "generated with [claude",
];

/// The longest subject line the history uses.
const SUBJECT_MAX: usize = 72;

/// The problems with a commit message: AI attribution, a subject over 72 characters, no blank
/// line between the subject and a body. Comment lines are ignored, as git strips them.
fn message_problems(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    let mut problems = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let lower = line.to_lowercase();
        if ATTRIBUTION.iter().any(|a| lower.contains(a)) {
            problems.push(format!(
                "line {}: AI attribution is not allowed: {line}",
                i + 1
            ));
        }
    }
    if let Some(subject) = lines.first() {
        let n = subject.chars().count();
        if n > SUBJECT_MAX {
            problems.push(format!(
                "the subject is {n} characters; keep it to {SUBJECT_MAX}: {subject}"
            ));
        }
    }
    if lines.get(1).is_some_and(|l| !l.trim().is_empty()) {
        problems.push("leave a blank line between the subject and the body".to_owned());
    }
    problems
}

fn commit_msg(file: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("commit-msg: cannot read {}: {e}", file.display());
            return ExitCode::FAILURE;
        }
    };
    let problems = message_problems(&text);
    for p in &problems {
        eprintln!("commit-msg: {p}");
    }
    if problems.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The properties every bot's server uses, from the race's shared config
/// (`config/race.toml`: simulation distance, view distance, seed, difficulty): offline mode
/// (no Mojang account), no spawn protection. RCON only when a password is in
/// `RUSTSTEVE_RCON_PASSWORD` (the race's own server; generated per run, never committed).
fn server_properties(
    c: &results::config::ServerConfig,
    port: &str,
    difficulty: &str,
    rcon: Option<(&str, &str)>,
) -> String {
    let rcon = match rcon {
        Some((rcon_port, password)) => {
            format!("enable-rcon=true\nrcon.port={rcon_port}\nrcon.password={password}\n")
        }
        None => "enable-rcon=false\n".to_owned(),
    };
    format!(
        "online-mode=false\nsimulation-distance={}\nview-distance={}\nlevel-seed={}\nspawn-protection=0\nserver-port={port}\ndifficulty={difficulty}\n{rcon}",
        c.simulation_distance, c.view_distance, c.seed
    )
}

/// Run a vanilla 26.1.2 server in the foreground: copy datagen's sha1-checked jar into `dir`,
/// accept the EULA, write the properties, `java -jar server.jar nogui` (Java 25: `nix develop`).
/// The world in `dir` is kept between runs; nothing here deletes one.
fn server(args: &[String]) -> ExitCode {
    let a = match ServerArgs::parse(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("server: {e}");
            return ExitCode::FAILURE;
        }
    };
    let ServerArgs {
        dir,
        port,
        jar,
        difficulty,
        config,
        rcon_port,
        view_distance,
        seed,
    } = a;
    let mut race = match results::config::RaceConfig::read(&config) {
        Ok((c, _)) => c,
        Err(e) => {
            eprintln!("server: {e}");
            return ExitCode::FAILURE;
        }
    };
    // Issue 69: a relay lane runs its own seed's world (`xtask seed <name>`); without the flag
    // the server keeps the race seed, so every older workflow and local server is unchanged.
    if let Some(s) = seed {
        race.server.seed = s;
    }
    // A capture server sends only what it captures: a smaller view distance generates fewer
    // chunks per landing.
    if let Some(v) = view_distance {
        race.server.view_distance = v;
    }
    let difficulty = difficulty.unwrap_or_else(|| race.server.difficulty.clone());
    let password = std::env::var("RUSTSTEVE_RCON_PASSWORD")
        .ok()
        .filter(|p| !p.is_empty());
    let properties = server_properties(
        &race.server,
        &port,
        &difficulty,
        password.as_deref().map(|p| (rcon_port.as_str(), p)),
    );
    let dir = Path::new(&dir);
    let setup = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::copy(&jar, dir.join("server.jar"))?;
        std::fs::write(dir.join("eula.txt"), "eula=true\n")?;
        std::fs::write(dir.join("server.properties"), &properties)
    };
    if let Err(e) = setup() {
        eprintln!(
            "server: set up {} from {jar}: {e} (run datagen once to download the jar)",
            dir.display()
        );
        return ExitCode::FAILURE;
    }
    println!(
        "server: {} on port {port}, seed {}, rcon {}",
        dir.display(),
        race.server.seed,
        if password.is_some() {
            rcon_port.as_str()
        } else {
            "off"
        }
    );
    match Command::new("java")
        .args(["-Xmx2G", "-jar", "server.jar", "nogui"])
        .current_dir(dir)
        .status()
    {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => {
            eprintln!("server: exited {s}");
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("server: run java (Java 25 must be on PATH: nix develop): {e}");
            ExitCode::FAILURE
        }
    }
}

/// `xtask server`'s flags, each `--flag value`; a flag left out keeps its default.
#[derive(Debug, PartialEq, Eq)]
struct ServerArgs {
    dir: String,
    port: String,
    jar: String,
    difficulty: Option<String>,
    config: String,
    rcon_port: String,
    view_distance: Option<u32>,
    /// `level-seed` in place of the race config's `seed` (issue 69).
    seed: Option<String>,
}

impl ServerArgs {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut a = Self {
            dir: "target/server".to_owned(),
            port: "25565".to_owned(),
            jar: "target/datagen/server.jar".to_owned(),
            difficulty: None,
            config: "config/race.toml".to_owned(),
            rcon_port: "25575".to_owned(),
            view_distance: None,
            seed: None,
        };
        let mut it = args.iter();
        while let Some(flag) = it.next() {
            let value = it.next().ok_or_else(|| format!("{flag} needs a value"))?;
            match flag.as_str() {
                "--dir" => a.dir.clone_from(value),
                "--port" => a.port.clone_from(value),
                "--jar" => a.jar.clone_from(value),
                "--difficulty" => a.difficulty = Some(value.clone()),
                "--config" => a.config.clone_from(value),
                "--rcon-port" => a.rcon_port.clone_from(value),
                "--view-distance" => {
                    a.view_distance =
                        Some(value.parse().map_err(|e| format!("--view-distance: {e}"))?);
                }
                // An empty value would make vanilla pick a random world: never a fixed seed.
                "--seed" if value.trim().is_empty() => {
                    return Err("--seed: an empty value".to_owned());
                }
                "--seed" => a.seed = Some(value.clone()),
                other => return Err(format!("unknown flag {other}")),
            }
        }
        Ok(a)
    }
}

/// The seeds file `seeds` and `seed` read: `--file <path>` anywhere, else `config/seeds.toml`;
/// the other words, in order.
fn seeds_args(args: &[String]) -> Result<(String, Vec<String>), String> {
    let mut file = "config/seeds.toml".to_owned();
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--file" {
            file.clone_from(it.next().ok_or("--file needs a value")?);
        } else {
            rest.push(a.clone());
        }
    }
    Ok((file, rest))
}

/// `xtask seeds`: every seed's name, one a line, in file order.
fn seeds(args: &[String]) -> ExitCode {
    let listed = seeds_args(args).and_then(|(file, rest)| match rest.as_slice() {
        [] => results::config::Seeds::read(&file),
        _ => Err(format!(
            "unexpected {rest:?}; usage: xtask seeds [--file <path>]"
        )),
    });
    match listed {
        Ok(s) => {
            for seed in &s.seed {
                println!("{}", seed.name);
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("seeds: {e}");
            ExitCode::FAILURE
        }
    }
}

/// The value of the seed named `name`, or why there is none.
fn seed_value(seeds: &results::config::Seeds, name: &str) -> Result<String, String> {
    seeds
        .find(name)
        .map(|s| s.value.to_string())
        .ok_or_else(|| {
            let names: Vec<&str> = seeds.seed.iter().map(|s| s.name.as_str()).collect();
            format!("no seed {name:?}; seeds: {}", names.join(", "))
        })
}

/// `xtask seed <name>`: that seed's `level-seed` value, for `xtask server --seed`.
fn seed(args: &[String]) -> ExitCode {
    let value = seeds_args(args).and_then(|(file, rest)| match rest.as_slice() {
        [name] => seed_value(&results::config::Seeds::read(&file)?, name),
        _ => Err("usage: xtask seed <name> [--file <path>]".to_owned()),
    });
    match value {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("seed: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ServerArgs, message_problems, seed_value, seeds_args};

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn no_seed_flag_keeps_the_race_seed() {
        let a = ServerArgs::parse(&words("--port 25566 --view-distance 4")).expect("args");
        assert_eq!(a.seed, None);
        assert_eq!(a.port, "25566");
        assert_eq!(a.view_distance, Some(4));
        assert_eq!(a.config, "config/race.toml");
    }

    #[test]
    fn the_seed_flag_overrides() {
        let a = ServerArgs::parse(&words("--seed -4172144997902289642 --dir d")).expect("args");
        assert_eq!(a.seed.as_deref(), Some("-4172144997902289642"));
        assert_eq!(a.dir, "d");
    }

    #[test]
    fn bad_server_flags_are_refused() {
        assert!(ServerArgs::parse(&words("--seed")).is_err());
        assert!(ServerArgs::parse(&["--seed".to_owned(), " ".to_owned()]).is_err());
        assert!(ServerArgs::parse(&words("--sed 1")).is_err());
        assert!(ServerArgs::parse(&words("--view-distance x")).is_err());
    }

    #[test]
    fn seeds_args_take_a_file_anywhere() {
        assert_eq!(
            seeds_args(&words("typecraft --file f.toml")).expect("args"),
            ("f.toml".to_owned(), words("typecraft"))
        );
        assert_eq!(
            seeds_args(&[]).expect("args"),
            ("config/seeds.toml".to_owned(), Vec::new())
        );
        assert!(seeds_args(&words("--file")).is_err());
    }

    #[test]
    fn a_seed_is_looked_up_by_name() {
        let seeds = results::config::Seeds::read("../../config/seeds.toml").expect("seeds");
        assert_eq!(seed_value(&seeds, "typecraft").as_deref(), Ok("typecraft"));
        assert!(seed_value(&seeds, "no-such-seed").is_err());
    }

    #[test]
    fn a_plain_message_passes() {
        assert!(message_problems("feat: a thing\n\n- one\n- two\n").is_empty());
    }

    #[test]
    fn attribution_is_refused() {
        let msg = "fix: x\n\nbody\n\nCo-Authored-By: Claude Opus <noreply@anthropic.com>\n";
        assert_eq!(message_problems(msg).len(), 1);
        assert_eq!(
            message_problems("fix: x\n\nClaude-Session: https://x\n").len(),
            1
        );
    }

    #[test]
    fn long_subjects_and_missing_blank_lines_are_refused() {
        assert_eq!(
            message_problems(&format!("feat: {}\n", "a".repeat(80))).len(),
            1
        );
        assert_eq!(message_problems("feat: a\nbody\n").len(), 1);
    }

    #[test]
    fn comment_lines_are_ignored() {
        assert!(message_problems("feat: a\n\n# Co-Authored-By: Claude\n").is_empty());
    }
}
