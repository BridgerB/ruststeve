//! ingest: the insight store (brain/ruststeve/design/insight-store.md). Folds race batch and case
//! files into an embedded SurrealDB 4 (SurrealKV, versioned) whose schema is
//! `schema/insight-v1.surql`, the whole contract with steve. Files stay the truth; the store is
//! rebuilt from them and never written back (brain/ruststeve/decisions/data-out.md).
//!
//! ```text
//! cargo run --release --manifest-path crates/ingest/Cargo.toml -- add <store dir> <batch dirs or batch.rsb>...
//! cargo run --release --manifest-path crates/ingest/Cargo.toml -- query <store dir> kn|deaths|endings|batches|runs
//! cargo run --release --manifest-path crates/ingest/Cargo.toml -- case <store dir> <race run id> <bot>
//! ```
//!
//! `add` is idempotent: a batch already in the store (same race run and lane) is skipped, so
//! each publish adds only the new race to the downloaded store, and SurrealKV's versions keep
//! what the store said before it.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use results::{Batch, Case, Source};
use serde_json::{Value, json};
use surrealdb::Surreal;
use surrealdb::engine::local::{Db, SurrealKv};
use surrealdb::types::RecordId;
use world::Stage;

/// The schema this build writes: the contract file itself, compiled in.
const SCHEMA: &str = include_str!("../../../schema/insight-v1.surql");
const SCHEMA_VERSION: u32 = 1;
const NS: &str = "ruststeve";
const DB: &str = "insight";

type Res<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn open(dir: &Path) -> Res<Surreal<Db>> {
    let db = Surreal::new::<SurrealKv>(dir.to_string_lossy().as_ref())
        .versioned()
        .await
        .map_err(|e| format!("open {}: {e}", dir.display()))?;
    db.use_ns(NS).use_db(DB).await.map_err(err)?;
    Ok(db)
}

/// Run statements and fail on the first error any of them returned.
async fn run(
    db: &Surreal<Db>,
    q: surrealdb::method::Query<'_, Db>,
) -> Res<surrealdb::IndexedResults> {
    let _ = db;
    let mut r = q.await.map_err(err)?;
    let errors = r.take_errors();
    if let Some((i, e)) = errors.into_iter().min_by_key(|(i, _)| *i) {
        return Err(format!("statement {i}: {e}"));
    }
    Ok(r)
}

fn source_name(s: Source) -> &'static str {
    match s {
        Source::Real => "real",
        Source::Sim => "sim",
    }
}

fn rank(s: Stage) -> i64 {
    Stage::ALL
        .iter()
        .position(|x| *x == s)
        .and_then(|i| i64::try_from(i).ok())
        .unwrap_or(-1)
}

/// Every `batch.rsb` under the paths, sorted.
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

async fn schema(db: &Surreal<Db>) -> Res<()> {
    run(db, db.query(SCHEMA)).await?;
    run(
        db,
        db.query("UPSERT meta:schema CONTENT { version: $v, surrealdb: '4.0.1' }")
            .bind(("v", SCHEMA_VERSION)),
    )
    .await?;
    for s in Stage::ALL {
        let _: Option<Value> = db
            .upsert(("stage", s.name()))
            .content(json!({ "name": s.name(), "rank": rank(s) }))
            .await
            .map_err(err)?;
    }
    Ok(())
}

/// Add one lane's batch and its cases; `false` when it was already in the store.
async fn add_batch(db: &Surreal<Db>, path: &Path, b: &Batch) -> Res<bool> {
    let batch_key = format!("{}/{}", b.run, b.lane);
    let existing: Option<Value> = db
        .select(("batch", batch_key.as_str()))
        .await
        .map_err(err)?;
    if existing.is_some() {
        return Ok(false);
    }
    let dir = path.parent().unwrap_or(Path::new("."));
    let short = b.commit.get(..7).unwrap_or(&b.commit).to_owned();
    let _: Option<Value> = db
        .upsert(("commit", b.commit.as_str()))
        .content(json!({ "sha": b.commit, "short": short }))
        .await
        .map_err(err)?;
    let _: Option<Value> = db
        .upsert(("start", b.start.as_str()))
        .content(json!({ "name": b.start }))
        .await
        .map_err(err)?;
    let c = &b.coverage;
    let _: Option<Value> = db
        .create(("batch", batch_key.as_str()))
        .content(json!({
            "race_run": b.run, "commit": b.commit, "lane": b.lane, "start": b.start,
            "source": source_name(b.source), "config_hash": b.config_hash, "bots": b.bots,
            "server": b.server, "server_tick_rate": b.server_tick_rate,
            "runs": c.runs, "cases": c.cases, "recordings": c.recordings,
            "coverage": c.cases_ratio(), "events_dropped": c.events_dropped,
            "unknown_packets": c.unknown_packets,
            "artifact": format!("race-{}-lane-{}", b.run, b.lane),
        }))
        .await
        .map_err(err)?;
    for row in &b.rows {
        let case: Option<Case> = match &row.case {
            Some(f) => Some(results::read_case(&dir.join(f))?),
            None => None,
        };
        let key = format!("{}/{}", b.run, row.bot);
        let signature = &row.signature;
        let case_json = case
            .as_ref()
            .map(|c| serde_json::to_value(c).map_err(err))
            .transpose()?
            .unwrap_or(Value::Object(Default::default()));
        let _: Option<Value> = db
            .upsert(("run", key.as_str()))
            .content(absent_nulls(json!({
                "race_run": b.run, "bot": row.bot, "commit": b.commit, "lane": b.lane,
                "start": b.start, "source": source_name(b.source),
                "stage_reached": row.stage_reached.map(Stage::name),
                "stage_rank": row.stage_reached.map_or(-1, rank),
                "ending": row.ending.name(), "counted": row.ending.counted(),
                "killer": signature[1], "doing": signature[2], "why": signature[3],
                "server_ticks": row.server_ticks, "wall_ms": row.wall_ms,
                "recording": row.recording,
                "replay": case.as_ref().map_or("-".to_owned(), |c| c.replay.clone()),
                "case": case_json,
            })))
            .await
            .map_err(err)?;
        let run_id = RecordId::new("run", key.as_str());
        let mut rel = String::from(
            "RELATE $commit->ran->$run; RELATE $run->played->$start; RELATE $run->in_batch->$batch;",
        );
        if row.stage_reached.is_some() {
            rel.push_str(" RELATE $run->reached->$stage;");
        }
        run(
            db,
            db.query(rel)
                .bind(("commit", RecordId::new("commit", b.commit.as_str())))
                .bind(("run", run_id))
                .bind(("start", RecordId::new("start", b.start.as_str())))
                .bind(("batch", RecordId::new("batch", batch_key.as_str())))
                .bind((
                    "stage",
                    RecordId::new("stage", row.stage_reached.map_or("none", Stage::name)),
                )),
        )
        .await?;
        if row.ending.counted() {
            for s in Stage::ALL {
                let reached = i64::from(row.stage_reached.is_some_and(|r| r >= s));
                let _: Option<Value> = db
                    .upsert(("attempt", format!("{key}/{}", s.name()).as_str()))
                    .content(json!({
                        "commit": b.commit, "start": b.start, "source": source_name(b.source),
                        "stage": s.name(), "rank": rank(s), "reached": reached,
                    }))
                    .await
                    .map_err(err)?;
            }
        }
    }
    Ok(true)
}

async fn select_all(db: &Surreal<Db>, q: &str) -> Res<Vec<Value>> {
    let mut r = run(db, db.query(q)).await?;
    r.take::<Vec<Value>>(0).map_err(err)
}

/// One query by name, as text lines.
async fn query(db: &Surreal<Db>, what: &str) -> Res<Vec<String>> {
    let rows = match what {
        "kn" => {
            select_all(
                db,
                "SELECT commit, start, source, stage, rank, k, n FROM kn ORDER BY commit, start, source, rank",
            )
            .await?
        }
        "deaths" => select_all(db, "SELECT commit, stage_reached, killer, n FROM deaths ORDER BY n DESC").await?,
        "endings" => select_all(db, "SELECT commit, start, ending, killer, doing, why, n FROM endings ORDER BY n DESC").await?,
        "batches" => {
            select_all(
                db,
                "SELECT race_run, lane, commit, start, source, bots, runs, cases, coverage, server_tick_rate FROM batch ORDER BY race_run, lane",
            )
            .await?
        }
        "runs" => {
            select_all(
                db,
                "SELECT race_run, bot, commit, start, stage_reached, ending, killer, server_ticks FROM run ORDER BY race_run, bot",
            )
            .await?
        }
        other => return Err(format!("no query {other}: kn, deaths, endings, batches, runs")),
    };
    Ok(rows.iter().map(|r| strip_id(r).to_string()).collect())
}

/// A row with its `null` fields dropped: an `option<..>` field is NONE when absent, and a
/// JSON null is not NONE to SurrealDB.
fn absent_nulls(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.into_iter().filter(|(_, v)| !v.is_null()).collect()),
        other => other,
    }
}

/// A row without its record id, for printing.
fn strip_id(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| *k != "id")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// Add every batch under the paths: (added, already in the store).
async fn add_all(db: &Surreal<Db>, paths: &[String]) -> Res<(u32, u32)> {
    let files = batches_under(paths);
    if files.is_empty() {
        return Err("no batch.rsb found".to_owned());
    }
    schema(db).await?;
    let (mut added, mut skipped) = (0, 0);
    for f in &files {
        let b = results::read_batch(f)?;
        if add_batch(db, f, &b).await? {
            added += 1;
        } else {
            skipped += 1;
        }
    }
    Ok((added, skipped))
}

async fn main_async(args: &[String]) -> Res<()> {
    match args {
        [cmd, store, rest @ ..] if cmd == "add" => {
            let db = open(Path::new(store)).await?;
            let (added, skipped) = add_all(&db, rest).await?;
            println!("ingest: {added} batches added, {skipped} already in the store");
            Ok(())
        }
        [cmd, store, what] if cmd == "query" => {
            let db = open(Path::new(store)).await?;
            for line in query(&db, what).await? {
                println!("{line}");
            }
            Ok(())
        }
        [cmd, store, race_run, bot] if cmd == "case" => {
            let db = open(Path::new(store)).await?;
            let key = format!("{race_run}/{bot}");
            let row: Option<Value> = db.select(("run", key.as_str())).await.map_err(err)?;
            let row = row.ok_or_else(|| format!("no run {key}"))?;
            let case: Case = serde_json::from_value(row.get("case").cloned().unwrap_or(Value::Null))
                .map_err(|e| format!("run {key}: the case does not read back: {e}"))?;
            print!("{}", case.render());
            Ok(())
        }
        _ => Err("usage: ingest add <store> <paths>... | query <store> <view> | case <store> <run> <bot>".to_owned()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(10 * 1024 * 1024)
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("ingest: {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(main_async(&args)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("ingest: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use results::{Batch, Case, Coverage, Doing, Ending, FORMAT_VERSION, Row, Source};
    use world::Stage;

    use super::*;

    fn case(bot: &str, stage: Option<Stage>, ending: Ending) -> Case {
        Case {
            format_version: FORMAT_VERSION,
            run: "r1".to_owned(),
            bot: bot.to_owned(),
            commit: "c0ffee0000000000000000000000000000000000".to_owned(),
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
                why: "test".to_owned(),
            },
            items_gained: BTreeMap::new(),
            items_lost: BTreeMap::new(),
            swings_sent: 0,
            swings_landed: 0,
            health: None,
            pos: None,
            dimension: None,
            server_ticks: 100,
            wall_ms: 5000,
            recording: None,
            replay: "-".to_owned(),
            stream: None,
            plan: None,
        }
    }

    /// Writes a batch of these cases into `dir`, as the race does.
    fn write_batch(dir: &Path, cases: &[Case]) {
        std::fs::create_dir_all(dir).unwrap();
        let rows = cases
            .iter()
            .map(|c| {
                let f = format!("{}.case.rsc", c.bot);
                results::write(&dir.join(&f), c).unwrap();
                Row {
                    bot: c.bot.clone(),
                    stage_reached: c.stage_reached,
                    ending: c.ending.clone(),
                    signature: c.signature(),
                    server_ticks: c.server_ticks,
                    wall_ms: c.wall_ms,
                    case: Some(f),
                    recording: None,
                }
            })
            .collect();
        let n = cases.len() as u64;
        let b = Batch {
            format_version: FORMAT_VERSION,
            run: "r1".to_owned(),
            commit: cases[0].commit.clone(),
            lane: 0,
            start: "spawn".to_owned(),
            source: Source::Real,
            config_hash: "0".to_owned(),
            bots: u32::try_from(cases.len()).unwrap(),
            server: "test".to_owned(),
            server_tick_rate: 20.0,
            rows,
            coverage: Coverage {
                runs: n,
                cases: n,
                ..Coverage::default()
            },
        };
        results::write(&dir.join("batch.rsb"), &b).unwrap();
    }

    #[test]
    fn the_store_folds_k_over_n_deaths_and_cases() {
        let root = std::env::temp_dir().join(format!("ingest-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let lane = root.join("lane");
        write_batch(
            &lane,
            &[
                case("a", Some(Stage::Table), Ending::Cap),
                case(
                    "b",
                    Some(Stage::InWorld),
                    Ending::Death {
                        killer: "death.attack.mob zombie".to_owned(),
                    },
                ),
                case("c", Some(Stage::End), Ending::Cut),
            ],
        );
        let store = root.join("store");
        let lane_s = vec![lane.to_string_lossy().into_owned()];
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let db = open(&store).await.unwrap();
            assert_eq!(add_all(&db, &lane_s).await.unwrap(), (1, 0));
            // Idempotent: the same batch again adds nothing.
            assert_eq!(add_all(&db, &lane_s).await.unwrap(), (0, 1));
            let kn = query(&db, "kn").await.unwrap();
            let get = |stage: &str| {
                kn.iter()
                    .map(|l| serde_json::from_str::<Value>(l).unwrap())
                    .find(|v| v["stage"] == stage)
                    .map(|v| (v["k"].as_i64().unwrap(), v["n"].as_i64().unwrap()))
                    .unwrap()
            };
            // The cut run is not counted; the death reached in_world.
            assert_eq!(get("in_world"), (2, 2));
            assert_eq!(get("table"), (1, 2));
            assert_eq!(get("end"), (0, 2));
            let deaths = query(&db, "deaths").await.unwrap();
            assert_eq!(deaths.len(), 1);
            assert!(deaths[0].contains("death.attack.mob zombie"), "{deaths:?}");
            let row: Option<Value> = db.select(("run", "r1/b")).await.unwrap();
            let back: Case = serde_json::from_value(row.unwrap()["case"].clone()).unwrap();
            assert_eq!(
                back,
                case(
                    "b",
                    Some(Stage::InWorld),
                    Ending::Death {
                        killer: "death.attack.mob zombie".to_owned(),
                    }
                )
            );
        });
        drop(rt);
        let _ = std::fs::remove_dir_all(&root);
    }
}
