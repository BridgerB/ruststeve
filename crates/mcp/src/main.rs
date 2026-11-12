//! `ruststeve-mcp`: an MCP server (the official Rust SDK, `rmcp`) that gives coding agents
//! ruststeve's own tools over stdio. Registered for Claude Code in `.mcp.json`.
//!
//! Code never runs on the Mac; these tools only dispatch work to the GitHub runners and read
//! results back (`gh`), or read the brain. The bot itself never links this crate: it is shell-side
//! tooling, outside the deterministic core, so tokio is fine here.
//!
//! Tools: `probe_run`, `probe_output`, `run_status`, `run_log`, `ci_runs`, `brain_check`,
//! `brain_note`, `brain_search`.

use std::path::{Path, PathBuf};
use std::process::Command;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars::JsonSchema;
use serde::Deserialize;

/// The GitHub repository every `gh` call targets.
const REPO: &str = "BridgerB/ruststeve";
/// The most text a tool returns; longer output keeps its tail, where errors and results are.
const MAX_TEXT: usize = 120_000;

fn repo_root() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    root.canonicalize().unwrap_or(root)
}

/// Run a program at the repository root; stdout and stderr, and a note when it failed.
fn run(program: &str, args: &[&str]) -> String {
    match Command::new(program)
        .args(args)
        .current_dir(repo_root())
        .output()
    {
        Ok(out) => {
            let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
            let err = String::from_utf8_lossy(&out.stderr);
            if !err.trim().is_empty() {
                text.push_str("\n[stderr]\n");
                text.push_str(&err);
            }
            if !out.status.success() {
                text.push_str(&format!("\n[exit {}]", out.status));
            }
            tail(text)
        }
        Err(e) => format!("cannot run {program}: {e}"),
    }
}

/// Keep the last `MAX_TEXT` bytes, cut at a character boundary.
fn tail(text: String) -> String {
    if text.len() <= MAX_TEXT {
        return text;
    }
    let mut start = text.len() - MAX_TEXT;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("[... {} bytes cut ...]\n{}", start, &text[start..])
}

/// Every note under `brain/`, as (stem, path).
fn notes() -> Vec<(String, PathBuf)> {
    fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "md") {
                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                out.push((stem, path));
            }
        }
    }
    let mut out = Vec::new();
    walk(&repo_root().join("brain"), &mut out);
    out.sort();
    out
}

#[derive(Deserialize, JsonSchema)]
struct ProbeRun {
    /// Shell command to run at the repository root on a runner.
    command: String,
    /// Branch or tag whose code runs (default: main).
    git_ref: Option<String>,
    /// Minutes before the command is stopped (default 60).
    timeout_minutes: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct RunId {
    /// The GitHub Actions run id.
    run_id: u64,
}

#[derive(Deserialize, JsonSchema)]
struct CiRuns {
    /// Only runs of this branch (default: all branches).
    branch: Option<String>,
    /// How many runs (default 10).
    limit: Option<u32>,
}

#[derive(Deserialize, JsonSchema)]
struct BrainCheck {
    /// "check" (default), "drift", or "touches"; "touches" also needs `source` and `paths`.
    mode: Option<String>,
    /// For "touches": the source name from brain/sources (e.g. "ruststeve").
    source: Option<String>,
    /// For "touches": the changed paths.
    paths: Option<Vec<String>>,
}

#[derive(Deserialize, JsonSchema)]
struct NoteName {
    /// The note's file stem, e.g. "sans-io-core", or a hub path such as "ruststeve/_moc".
    name: String,
}

#[derive(Deserialize, JsonSchema)]
struct Search {
    /// Words to find in note titles, descriptions and aliases (case-insensitive).
    query: String,
}

#[derive(Clone)]
struct Tools {
    tool_router: ToolRouter<Self>,
}

#[tool_router(router = tool_router)]
impl Tools {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Run one shell command on a GitHub Actions runner (the probe workflow) and return the run id. Code never runs on the Mac; this is how to run it."
    )]
    async fn probe_run(&self, Parameters(p): Parameters<ProbeRun>) -> String {
        let git_ref = p.git_ref.unwrap_or_else(|| "main".into());
        let command_field = format!("command={}", p.command);
        let timeout_field = format!("timeout={}", p.timeout_minutes.unwrap_or(60));
        let dispatched = run(
            "gh",
            &[
                "workflow",
                "run",
                "probe.yml",
                "-R",
                REPO,
                "--ref",
                &git_ref,
                "-f",
                &command_field,
                "-f",
                &timeout_field,
            ],
        );
        // The dispatch call returns before the run is listed; give GitHub a moment.
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        let latest = run(
            "gh",
            &[
                "run",
                "list",
                "-R",
                REPO,
                "--workflow",
                "probe.yml",
                "--branch",
                &git_ref,
                "--limit",
                "1",
                "--json",
                "databaseId,status,displayTitle,createdAt,url",
            ],
        );
        format!("{dispatched}\nlatest probe run (use its databaseId with probe_output):\n{latest}")
    }

    #[tool(description = "Status of a run: overall result and each job's result and timing.")]
    async fn run_status(&self, Parameters(p): Parameters<RunId>) -> String {
        run(
            "gh",
            &[
                "run",
                "view",
                &p.run_id.to_string(),
                "-R",
                REPO,
                "--json",
                "status,conclusion,displayTitle,headBranch,createdAt,updatedAt,url,jobs",
            ],
        )
    }

    #[tool(description = "The log lines of a run's failed steps.")]
    async fn run_log(&self, Parameters(p): Parameters<RunId>) -> String {
        run(
            "gh",
            &[
                "run",
                "view",
                &p.run_id.to_string(),
                "-R",
                REPO,
                "--log-failed",
            ],
        )
    }

    #[tool(
        description = "Download a finished probe run's artifact and return probe.log, plus the list of files in probe-out/."
    )]
    async fn probe_output(&self, Parameters(p): Parameters<RunId>) -> String {
        let dir = std::env::temp_dir().join(format!("ruststeve-probe-{}", p.run_id));
        let dir_text = dir.to_string_lossy().into_owned();
        let downloaded = run(
            "gh",
            &[
                "run",
                "download",
                &p.run_id.to_string(),
                "-R",
                REPO,
                "-D",
                &dir_text,
            ],
        );
        let log = std::fs::read_dir(&dir)
            .ok()
            .and_then(|mut it| it.next())
            .and_then(Result::ok)
            .map(|artifact| artifact.path().join("probe.log"))
            .and_then(|path| std::fs::read_to_string(path).ok());
        match log {
            Some(text) => tail(format!("downloaded to {dir_text}\n{text}")),
            None => format!("no probe.log found in {dir_text}\n{downloaded}"),
        }
    }

    #[tool(description = "Recent CI and probe runs, optionally for one branch.")]
    async fn ci_runs(&self, Parameters(p): Parameters<CiRuns>) -> String {
        let limit = p.limit.unwrap_or(10).to_string();
        let mut args = vec![
            "run",
            "list",
            "-R",
            REPO,
            "--limit",
            &limit,
            "--json",
            "databaseId,workflowName,headBranch,event,status,conclusion,createdAt,displayTitle",
        ];
        let branch = p.branch.unwrap_or_default();
        if !branch.is_empty() {
            args.extend(["--branch", branch.as_str()]);
        }
        run("gh", &args)
    }

    #[tool(
        description = "Run the brain checker (a seconds-long static check, allowed locally): the four invariants, drift since pins, or which notes cite given paths."
    )]
    async fn brain_check(&self, Parameters(p): Parameters<BrainCheck>) -> String {
        let mode = p.mode.unwrap_or_else(|| "check".into());
        let mut args: Vec<String> =
            vec!["run".into(), "-q".into(), "-p".into(), "brain-check".into()];
        match mode.as_str() {
            "drift" => args.extend(["--".into(), "drift".into()]),
            "touches" => {
                args.extend(["--".into(), "touches".into(), p.source.unwrap_or_default()]);
                args.extend(p.paths.unwrap_or_default());
            }
            _ => {}
        }
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run("cargo", &refs)
    }

    #[tool(
        description = "Read one brain note by its stem (e.g. 'sans-io-core') or hub path (e.g. 'ruststeve/_moc', '_index')."
    )]
    async fn brain_note(&self, Parameters(p): Parameters<NoteName>) -> String {
        let wanted = p.name.trim_end_matches(".md");
        let brain = repo_root().join("brain");
        let by_path = brain.join(format!("{wanted}.md"));
        let path = if by_path.exists() {
            Some(by_path)
        } else {
            notes()
                .into_iter()
                .find(|(stem, _)| stem == wanted)
                .map(|(_, path)| path)
        };
        match path.and_then(|p| std::fs::read_to_string(p).ok()) {
            Some(text) => text,
            None => format!("no note named {wanted}; try brain_search"),
        }
    }

    #[tool(
        description = "Find brain notes whose title, description or aliases contain all the query's words; returns each note's stem and description."
    )]
    async fn brain_search(&self, Parameters(p): Parameters<Search>) -> String {
        let words: Vec<String> = p.query.split_whitespace().map(str::to_lowercase).collect();
        let mut hits = Vec::new();
        for (stem, path) in notes() {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let front: String = text
                .lines()
                .take_while(|l| !l.starts_with("sourceRefs:"))
                .filter(|l| {
                    l.starts_with("title:")
                        || l.starts_with("description:")
                        || l.starts_with("aliases:")
                })
                .collect::<Vec<_>>()
                .join(" ");
            let hay = format!("{stem} {front}").to_lowercase();
            if words.iter().all(|w| hay.contains(w)) {
                let description = text
                    .lines()
                    .find_map(|l| l.strip_prefix("description: "))
                    .unwrap_or("");
                hits.push(format!("{stem}: {description}"));
            }
        }
        if hits.is_empty() {
            "no matching notes".into()
        } else {
            hits.join("\n")
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for Tools {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let service = Tools::new().serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
