//! results: read batch and case files.
//!
//! ```text
//! cargo run -p results -- summary <dir or batch.rsb>...   # k/n per stage, coverage, endings (markdown)
//! cargo run -p results -- case <file.case.rsc>            # one case as text
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Every `batch.rsb` under the given paths, sorted.
fn batches_under(paths: &[String]) -> Vec<PathBuf> {
    fn walk(p: &Path, out: &mut Vec<PathBuf>) {
        if p.is_dir() {
            if let Ok(rd) = std::fs::read_dir(p) {
                for e in rd.flatten() {
                    walk(&e.path(), out);
                }
            }
        } else if p.file_name().is_some_and(|n| n == "batch.rsb") {
            out.push(p.to_path_buf());
        }
    }
    let mut out = Vec::new();
    for p in paths {
        walk(Path::new(p), &mut out);
    }
    out.sort();
    out
}

fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("summary") => {
            let files = batches_under(args.get(1..).unwrap_or(&[]));
            if files.is_empty() {
                return Err("no batch.rsb found".to_owned());
            }
            let batches = files
                .iter()
                .map(|f| results::read_batch(f))
                .collect::<Result<Vec<_>, _>>()?;
            print!(
                "{}",
                results::render_summary(&results::summarize(&batches), &batches)
            );
            Ok(())
        }
        Some("case") => {
            let path = args.get(1).ok_or("case needs a file")?;
            print!("{}", results::read_case(Path::new(path))?.render());
            Ok(())
        }
        _ => Err("usage: results summary <paths>... | results case <file>".to_owned()),
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
