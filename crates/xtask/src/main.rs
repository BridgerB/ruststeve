//! Repository automation, run in CI on the runners.
//!
//! ```text
//! cargo run -q -p xtask -- deps-boundary    the core crates depend on nothing that does I/O
//! ```
//!
//! Standard library only, like `brain-check`: it must build in seconds.

use std::path::Path;
use std::process::{Command, ExitCode};

/// The crates that hold the deterministic core: no I/O, no clock, no async runtime
/// (brain/ruststeve/design/workspace-crates.md).
const CORE_CRATES: [&str; 6] = ["protocol", "world", "physics", "memory", "plan", "sim"];

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
        other => {
            eprintln!("unknown task {other:?}; tasks: deps-boundary");
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
