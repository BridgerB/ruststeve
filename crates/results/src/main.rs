//! results: read batch and case files.
//!
//! ```text
//! cargo run -p results -- summary <dir or batch.rsb>...   # k/n per stage, coverage, endings (markdown)
//! cargo run -p results -- diagnose <dir>...               # by place, note kinds, cast failures (markdown)
//! cargo run -p results -- case <file.case.rsc>            # one case as text
//! cargo run -p results -- offline-uuid <name>              # the player file's UUID (snapshots)
//! cargo run -p results -- player-dat <gunzipped .dat>     # a player file as text (issue 17a)
//! cargo run -p results -- endermen <dirs>...               # seconds near endermen by place (17a)
//! cargo run -p results -- memory <memory.rsw> [kind]...    # a snapshot memory by dimension and kind (19a)
//! cargo run -p results -- region-find <region dir> <block|*>... [--box x0,y0,z0,x1,y1,z1]  # a block's cells (19a)
//! cargo run -p results -- region-strip <region dir> x0,z0 x1,z1 step   # column tops and biomes (19a)
//! cargo run -p results -- region-route <region dir> --from x,y,z --to x,y,z --box ... --cand ...   # route survey (19, Q9)
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
        Some("ledger") => {
            let path = args.get(1).ok_or("ledger needs a file")?;
            let l: results::Ledger = results::read(Path::new(path))?;
            print!("{}", l.render());
            Ok(())
        }
        Some("senses") => {
            let path = args.get(1).ok_or("senses needs a file")?;
            let every = args
                .iter()
                .position(|a| a == "--every")
                .and_then(|i| args.get(i + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(10);
            let s: results::SensesFile = results::read(Path::new(path))?;
            print!("{}", s.render(every));
            Ok(())
        }
        Some("case") => {
            let path = args.get(1).ok_or("case needs a file")?;
            print!("{}", results::read_case(Path::new(path))?.render());
            Ok(())
        }
        Some("player-dat") => {
            let path = args.get(1).ok_or("player-dat needs a gunzipped .dat")?;
            let raw = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            print!("{}", results::census::player_dat(&raw)?);
            Ok(())
        }
        Some("memory") => {
            let path = args.get(1).ok_or("memory needs a memory.rsw")?;
            let m = results::snapshot::read_memory(Path::new(path))?;
            print!(
                "{}",
                results::snapshot::memory_report(&m, args.get(2..).unwrap_or(&[]))
            );
            Ok(())
        }
        Some("region-find") => {
            let dir = args.get(1).ok_or("region-find needs a region directory")?;
            // --box x0,y0,z0,x1,y1,z1 keeps the cells inside two corners.
            let boxed = args.iter().position(|a| a == "--box");
            let within = match boxed.and_then(|i| args.get(i + 1)) {
                Some(s) => {
                    let v: Vec<i32> = s
                        .split(',')
                        .map(|p| p.trim().parse().map_err(|e| format!("--box {s}: {e}")))
                        .collect::<Result<_, String>>()?;
                    match v[..] {
                        [a, b, c, d, e, f] => Some(([a, b, c], [d, e, f])),
                        _ => return Err("--box needs six numbers".to_owned()),
                    }
                }
                None => None,
            };
            let names: Vec<String> = args
                .get(2..boxed.unwrap_or(args.len()))
                .unwrap_or(&[])
                .iter()
                .map(|n| {
                    if n.contains(':') {
                        n.clone()
                    } else {
                        format!("minecraft:{n}")
                    }
                })
                .collect();
            for (n, p, biome) in results::region::find(Path::new(dir), &names, within)? {
                println!("{n} {} {} {} {biome}", p[0], p[1], p[2]);
            }
            Ok(())
        }
        Some("region-strip") => {
            // region-strip <dir> x0,z0 x1,z1 step: the top block, its y and biome every step.
            let dir = args.get(1).ok_or("region-strip needs a region directory")?;
            let point = |i: usize| -> Result<(i32, i32), String> {
                let s = args.get(i).ok_or("region-strip needs x0,z0 x1,z1 step")?;
                let (a, b) = s.split_once(',').ok_or("a point is x,z")?;
                Ok((
                    a.trim().parse().map_err(|e| format!("{s}: {e}"))?,
                    b.trim().parse().map_err(|e| format!("{s}: {e}"))?,
                ))
            };
            let ((x0, z0), (x1, z1)) = (point(2)?, point(3)?);
            let step: f64 = args
                .get(4)
                .and_then(|s| s.parse().ok())
                .ok_or("region-strip needs a step")?;
            let chunks = results::region::all_chunks(Path::new(dir))?;
            let (dx, dz) = (f64::from(x1 - x0), f64::from(z1 - z0));
            let len = dx.hypot(dz);
            let n = (len / step).floor() as i32;
            for k in 0..=n {
                let t = f64::from(k) * step / len.max(1.0);
                let x = x0 + (dx * t).round() as i32;
                let z = z0 + (dz * t).round() as i32;
                match results::region::top(&chunks, x, z) {
                    Some((b, y, biome)) => {
                        println!("{:>5} {x} {z} {y} {b} {biome}", (f64::from(k) * step) as i32)
                    }
                    None => println!("{:>5} {x} {z} not generated", (f64::from(k) * step) as i32),
                }
            }
            Ok(())
        }
        Some("region-route") => {
            // region-route <dir> --from x,y,z --to x,y,z --box x0,y0,z0,x1,y1,z1 [--avoid x,z,r]...
            //   [--map] --cand 'name;miny;maxy;dig,bridge,lava,drop,low;radius;x,z x,z ...'...
            let dir = args.get(1).ok_or("region-route needs a region directory")?;
            let nums = |flag: &str| -> Result<Vec<Vec<i32>>, String> {
                args.windows(2)
                    .filter(|w| w[0] == flag)
                    .map(|w| {
                        w[1].split(',')
                            .map(|p| p.trim().parse().map_err(|e| format!("{flag} {}: {e}", w[1])))
                            .collect()
                    })
                    .collect()
            };
            let three = |flag: &str| -> Result<[i32; 3], String> {
                match nums(flag)?.first().map(Vec::as_slice) {
                    Some(&[a, b, c]) => Ok([a, b, c]),
                    _ => Err(format!("{flag} needs x,y,z")),
                }
            };
            let (from, to) = (three("--from")?, three("--to")?);
            let (lo, hi) = match nums("--box")?.first().map(Vec::as_slice) {
                Some(&[a, b, c, d, e, f]) => ([a, b, c], [d, e, f]),
                _ => return Err("--box needs x0,y0,z0,x1,y1,z1".to_owned()),
            };
            let avoid: Vec<([i32; 2], f64)> = nums("--avoid")?
                .iter()
                .filter_map(|v| match v[..] {
                    [x, z, r] => Some(([x, z], f64::from(r))),
                    _ => None,
                })
                .collect();
            let cands = args
                .windows(2)
                .filter(|w| w[0] == "--cand")
                .map(|w| results::route::parse_cand(&w[1]))
                .collect::<Result<Vec<_>, String>>()?;
            let g = results::route::Grid::load(Path::new(dir), lo, hi)?;
            let map = args.iter().any(|a| a == "--map");
            print!(
                "{}",
                results::route::survey(&g, from, to, &avoid, &cands, map)
            );
            Ok(())
        }
        Some("endermen") => {
            let files = files_under(args.get(1..).unwrap_or(&[]), |n| n.ends_with(".senses.rss"));
            let mut census = results::census::EndermanCensus::default();
            for f in &files {
                match results::read::<results::SensesFile>(f) {
                    Ok(s) => census.add(&s.rows),
                    Err(e) => {
                        eprintln!("skipped {e}");
                        census.unreadable += 1;
                    }
                }
            }
            print!("{}", census.render());
            Ok(())
        }
        _ => Err(
            "usage: results summary <paths>... | results diagnose <paths>... | results case <file> | results ledger <file> | results senses <file> [--every N] | results offline-uuid <name>"
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
