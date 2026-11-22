//! results: read batch and case files.
//!
//! ```text
//! cargo run -p results -- summary <dir or batch.rsb>...   # k/n per stage, coverage, endings (markdown)
//! cargo run -p results -- diagnose <dir>...               # by place, note kinds, cast failures (markdown)
//! cargo run -p results -- case <file.case.rsc>            # one case as text
//! cargo run -p results -- offline-uuid <name>              # the player file's UUID (snapshots)
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Every file under the given paths whose name `keep` accepts, sorted.
fn files_under(paths: &[String], keep: fn(&str) -> bool) -> Vec<PathBuf> {
    fn walk(p: &Path, keep: fn(&str) -> bool, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    walk(&e.path(), keep, out);
                }
            }
        } else if p.file_name().and_then(|n| n.to_str()).is_some_and(keep) {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        walk(Path::new(p), keep, &mut out);
    }
    out.sort();
    out
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("summary") => {
            let files = files_under(args.get(1..).unwrap_or(&[]), |n| n == "batch.rsb");
            if files.is_empty() {
                return Err("no batch.rsb found".to_owned());
            }
            let batches = files
                .iter()
                .map(|f| results::read_batch(f))
                .collect::<Result<Vec<_>, _>>()?;
            print!(
                "{}",
                results::render_summary(
                    &results::summarize(&batches),
                    &batches,
                    results::observe::Observe::from_repo().as_ref(),
                )
            );
            Ok(())
        }
        Some("diagnose") => {
            let files = files_under(args.get(1..).unwrap_or(&[]), |n| n.ends_with(".case.rsc"));
            if files.is_empty() {
                return Err("no .case.rsc found".to_owned());
            }
            let cases = files
                .iter()
                .map(|f| results::read_case(f))
                .collect::<Result<Vec<_>, _>>()?;
            print!("{}", results::diagnose::diagnose(&cases));
            Ok(())
        }
        Some("offline-uuid") => {
            let name = args.get(1).ok_or("offline-uuid needs a name")?;
            println!("{}", results::snapshot::offline_uuid(name));
            Ok(())
        }
        Some("case") => {
            let path = args.get(1).ok_or("case needs a file")?;
            print!("{}", results::read_case(Path::new(path))?.render());
            Ok(())
        }
        _ => Err(
            "usage: results summary <paths>... | results diagnose <paths>... | results case <file> | results offline-uuid <name>"
                .to_owned(),
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("results: {e}");
            ExitCode::FAILURE
        }
    }
}
