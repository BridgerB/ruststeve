//! Repository automation, run in CI and by the git hooks (flake.nix).
//!
//! ```text
//! cargo run -q -p xtask -- deps-boundary       the core crates depend on nothing that does I/O
//! cargo run -q -p xtask -- commit-msg <file>   a commit message keeps the repository's rules
//! cargo run -q -p xtask -- server [--dir target/server] [--port 25565] [--jar target/datagen/server.jar]
//!                                            a local vanilla 26.1.2 server (offline, simulation distance 3)
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
const FORBIDDEN: [&str; 10] = [
    "tokio",
    "mio",
    "reqwest",
    "surrealdb",
    "net",
    "rcon",
    "record",
    "ingest",
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
        other => {
            eprintln!("unknown task {other:?}; tasks: deps-boundary, commit-msg, server");
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

/// The server settings every bot's server uses: offline mode (no Mojang account), and the race
/// servers' simulation distance 3, which freezes mobs past about 48 blocks
/// (brain/game/mc-mob-facts.md). The seed is clojurecraft's, so its worlds stay comparable.
const SERVER_PROPERTIES: &str = "online-mode=false
simulation-distance=3
view-distance=8
level-seed=typecraft
spawn-protection=0
enable-rcon=false
";

/// Run a vanilla 26.1.2 server in the foreground: copy datagen's sha1-checked jar into `dir`,
/// accept the EULA, write the properties, `java -jar server.jar nogui` (Java 25: `nix develop`).
/// The world in `dir` is kept between runs; nothing here deletes one.
fn server(args: &[String]) -> ExitCode {
    let mut dir = "target/server".to_owned();
    let mut port = "25565".to_owned();
    let mut jar = "target/datagen/server.jar".to_owned();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let Some(value) = it.next() else {
            eprintln!("{flag} needs a value");
            return ExitCode::FAILURE;
        };
        match flag.as_str() {
            "--dir" => dir.clone_from(value),
            "--port" => port.clone_from(value),
            "--jar" => jar.clone_from(value),
            other => {
                eprintln!("unknown flag {other}");
                return ExitCode::FAILURE;
            }
        }
    }
    let dir = Path::new(&dir);
    let setup = || -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        std::fs::copy(&jar, dir.join("server.jar"))?;
        std::fs::write(dir.join("eula.txt"), "eula=true\n")?;
        std::fs::write(
            dir.join("server.properties"),
            format!("{SERVER_PROPERTIES}server-port={port}\n"),
        )
    };
    if let Err(e) = setup() {
        eprintln!(
            "server: set up {} from {jar}: {e} (run datagen once to download the jar)",
            dir.display()
        );
        return ExitCode::FAILURE;
    }
    println!("server: {} on port {port}", dir.display());
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

#[cfg(test)]
mod tests {
    use super::message_problems;

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
