//! datagen: the game's tables, generated from the vanilla server itself and committed as Rust in
//! `crates/data/src/generated/`. Never hand-typed (brain/ruststeve/design/datagen.md).
//!
//! ```text
//! cargo run -p datagen -- generate [--work target/datagen] [--out crates/data/src/generated]
//!                                  [--oracle <clojurecraft>/resources/clojurecraft]
//! ```
//!
//! Runs on the runners only (the Mac never runs code). It downloads the 26.1.2 server jar named by
//! Mojang's version manifest and checks its sha1, runs the server's own data generator
//! (`--reports`) with Java 25, reads the data files inside the inner server jar, and writes one
//! rustfmt'd Rust file per table. CI regenerates and fails on any diff against the committed files.

mod emit;
mod emit_probe;
mod fetch;
mod jar;
mod model;
mod oracle;
mod probe;

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("datagen: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let task = args.next();
    let mut work = PathBuf::from("target/datagen");
    let mut out = PathBuf::from("crates/data/src/generated");
    let mut oracle = None;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .with_context(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--work" => work = PathBuf::from(value),
            "--out" => out = PathBuf::from(value),
            "--oracle" => oracle = Some(PathBuf::from(value)),
            other => bail!("unknown flag {other}"),
        }
    }
    match task.as_deref() {
        Some("generate") => generate(&work, &out, oracle.as_deref()),
        other => bail!("unknown task {other:?}; tasks: generate"),
    }
}

fn generate(
    work: &std::path::Path,
    out: &std::path::Path,
    oracle: Option<&std::path::Path>,
) -> Result<()> {
    std::fs::create_dir_all(work).with_context(|| format!("create {}", work.display()))?;
    let jar = fetch::server_jar(work)?;
    let reports = fetch::reports(work)?;
    let entries = jar::inner_entries(&jar)?;
    let game = model::Game::read(&reports, &entries)?;
    game.print_counts();
    let probe = probe::run(work)?;
    if let Some(dir) = oracle {
        oracle::compare(dir, &probe)?;
    }
    emit::write_all(&game, &probe, out)?;
    println!("wrote {}", out.display());
    Ok(())
}
