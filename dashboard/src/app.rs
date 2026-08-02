use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use serde::{Deserialize, Serialize};

// The full speedrun roadmap (mirrors gen_dashboard.sh MS_NAMES). Milestone index →
// name; the race goal is ENTER NETHER (index 17).
pub const MS_NAMES: [&str; 29] = [
    "Logs", "Planks", "Table", "Sticks", "Wood Pick", "Cobble", "Stone Pick", "Furnace", "Coal",
    "Iron Ore", "Smelt", "IRON PICK", "Buckets", "Water Buckets", "Gather Food", "Flint & Steel",
    "Build Portal", "ENTER NETHER", "Nether Fortress", "Kill Blazes", "Hunt Endermen",
    "Return Overworld", "Eyes of Ender", "Find Stronghold", "Activate End Portal", "Bow & Arrows",
    "Enter The End", "End Crystals", "KILL DRAGON",
];
pub const GOAL_MS: i32 = 17; // ENTER NETHER

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BotView {
    pub name: String,
    pub current_step: String,
    pub done: u32,
    pub total: u32,
    pub milestones: Vec<(usize, u32)>, // (ms index, reach time secs)
    pub furthest: i32,
    pub phase: String,
    pub phase_color: String,
    pub logs: i32,
    pub planks: i32,
    pub sticks: i32,
    pub cobble: i32,
    pub pick: String,
    pub y: i32,
    pub deaths: u32,
    pub recent: Vec<String>,
    pub alive: bool,
    pub view: String, // top-down terrain slice (rows of category chars)
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct RaceView {
    pub elapsed_min: i64,
    pub running: bool,
    pub bots: Vec<BotView>,
}

fn phase_for(furthest: i32) -> (&'static str, &'static str) {
    match furthest {
        f if f >= 17 => ("NETHER", "#ff7043"),
        f if f >= 16 => ("PORTAL", "#ce93d8"),
        f if f >= 9 => ("IRON", "#90caf9"),
        f if f >= 6 => ("STONE", "#cfd8dc"),
        f if f >= 0 => ("WOOD", "#a5d6a7"),
        _ => ("START", "#9e9e9e"),
    }
}

// ── server: read the ruststeve race data ────────────────────────────────────
#[server(GetRace, "/api")]
pub async fn get_race() -> Result<RaceView, ServerFnError> {
    const DIR: &str = "/Users/bridger/Developer/mc/upstream/ruststeve";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let start = std::fs::read_to_string("/tmp/race-start")
        .ok()
        .and_then(|s| s.trim().parse::<i64>().ok())
        .unwrap_or(now);
    let elapsed_min = (now - start).max(0) / 60;

    // Milestones per bot from /tmp/race-ms-times.tsv: "botIdx msIdx timeSec".
    let mut ms_by_bot: Vec<Vec<(usize, u32)>> = vec![Vec::new(); 5];
    if let Ok(tsv) = std::fs::read_to_string("/tmp/race-ms-times.tsv") {
        for line in tsv.lines() {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() >= 3 {
                if let (Ok(b), Ok(m), Ok(t)) =
                    (p[0].parse::<usize>(), p[1].parse::<usize>(), p[2].parse::<u32>())
                {
                    if b < 5 && !ms_by_bot[b].iter().any(|&(mm, _)| mm == m) {
                        ms_by_bot[b].push((m, t));
                    }
                }
            }
        }
    }

    let running = std::process::Command::new("pgrep")
        .args(["-f", "bash race.sh"])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false);

    let mut bots = Vec::new();
    for i in 0..5usize {
        let path = format!("{DIR}/race-{i}.log");
        let log = std::fs::read_to_string(&path).unwrap_or_default();
        if log.is_empty() {
            continue;
        }
        let lines: Vec<&str> = log.lines().collect();
        let step_line = lines.iter().rev().find(|l| l.contains("→ ")).copied().unwrap_or("");
        let current_step = step_line
            .split("→ ")
            .nth(1)
            .and_then(|s| s.split(" (").next())
            .unwrap_or("")
            .trim()
            .to_string();
        let (done, total) = step_line
            .split('(')
            .nth(1)
            .and_then(|s| s.split(')').next())
            .and_then(|s| s.split_once('/'))
            .map(|(d, t)| (d.trim().parse().unwrap_or(0), t.trim().parse().unwrap_or(19)))
            .unwrap_or((0, 19));
        // The race-log stdout only prints logs/planks/sticks/pick — cobble= and y= live
        // in the SQLite events detail (e.g. "start 8/19 logs=11 ... cobble=173 pick=Stone
        // y=55"). Pull the newest detail row that carries them so the full inventory +
        // position show, and search it alongside the log lines.
        let db_detail = std::process::Command::new("sqlite3")
            .arg(format!("{DIR}/.memory-rust-race-{:03}.db", i + 1))
            .arg("select detail from events where detail like '%cobble=%' order by id desc limit 1")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        let kv = |key: &str| -> Option<String> {
            std::iter::once(db_detail.as_str())
                .chain(lines.iter().rev().copied())
                .find_map(|l| {
                    l.split_whitespace().find_map(|tok| {
                        tok.strip_prefix(key).map(|v| v.trim_end_matches(',').to_string())
                    })
                })
        };
        let numi = |key: &str| kv(key).and_then(|v| v.parse::<i32>().ok()).unwrap_or(-1);
        let deaths = log.matches("died").count() as u32;
        let alive = !step_line.is_empty();
        let recent: Vec<String> = {
            let mut r: Vec<String> = lines
                .iter()
                .rev()
                .filter(|l| !l.contains("keep") && !l.trim().is_empty())
                .take(8)
                .map(|l| l.trim().to_string())
                .collect();
            r.reverse();
            r
        };
        let mut milestones = ms_by_bot[i].clone();
        milestones.sort_by_key(|&(m, _)| m);
        let furthest = milestones.iter().map(|&(m, _)| m as i32).max().unwrap_or(-1);
        let (phase, phase_color) = phase_for(furthest);
        let name = format!("rust-race-{:03}", i + 1);
        let view = std::fs::read_to_string(format!("/tmp/rust-view-{name}.txt")).unwrap_or_default();
        bots.push(BotView {
            name: name.clone(),
            current_step,
            done,
            total,
            milestones,
            furthest,
            phase: phase.to_string(),
            phase_color: phase_color.to_string(),
            logs: numi("logs="),
            planks: numi("planks="),
            sticks: numi("sticks="),
            cobble: numi("cobble="),
            pick: kv("pick=").unwrap_or_else(|| "?".into()),
            y: numi("y="),
            deaths,
            recent,
            alive,
            view,
        });
    }
    Ok(RaceView { elapsed_min, running, bots })
}

// ── UI ──────────────────────────────────────────────────────────────────────
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
                // Mounts steve's Babylon viewer.js onto each bot card's <canvas>, pointing
                // its SSE at that bot's per-bot stream port. Kept out of the hydrated tree so
                // the WebGL contexts persist across the client-side stat polling.
                <script type="module" src="/mount.js"></script>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/dashboard.css"/>
        <Title text="ruststeve — race dashboard"/>
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    // Stable per-bot signals so each card's 3D <canvas> (mounted once by /mount.js)
    // survives stat updates. A client-side poll refreshes the signals every 3s instead
    // of a full-page refresh, which would tear down the WebGL viewers each cycle.
    let bots: RwSignal<Vec<RwSignal<BotView>>> = RwSignal::new(Vec::new());
    let meta: RwSignal<(i64, bool)> = RwSignal::new((0, false));

    Effect::new(move |_| {
        let poll = move || {
            leptos::task::spawn_local(async move {
                let Ok(r) = get_race().await else { return };
                meta.set((r.elapsed_min, r.running));
                let same_len = bots.with_untracked(|b| b.len() == r.bots.len());
                if same_len {
                    bots.with_untracked(|sigs| {
                        for (sig, b) in sigs.iter().zip(r.bots.into_iter()) {
                            sig.set(b);
                        }
                    });
                } else {
                    bots.set(r.bots.into_iter().map(RwSignal::new).collect());
                }
            });
        };
        poll();
        set_interval(poll, std::time::Duration::from_secs(3));
    });

    view! {
        <div class="wrap">
            <h1>"🏁 ruststeve — race to the Nether"</h1>
            <p class="muted">{move || {
                let (elapsed, running) = meta.get();
                let n = bots.with(|b| b.len());
                format!("elapsed {}:{:02}  •  {n} bots  •  {}",
                    elapsed / 60, elapsed % 60, if running { "running" } else { "stopped" })
            }}</p>
            <div class="grid">
                <For
                    each=move || bots.get().into_iter().enumerate()
                    key=|(i, _)| *i
                    children=move |(i, sig)| view! { <BotCard idx=i bot=sig/> }
                />
            </div>
        </div>
    }
}

fn ms_cells(bot: &BotView) -> impl IntoView {
    let reached: std::collections::HashMap<usize, u32> = bot.milestones.iter().copied().collect();
    (0..MS_NAMES.len())
        .map(|m| {
            let hit = reached.get(&m).copied();
            let is_goal = m as i32 == GOAL_MS;
            let cls = if hit.is_some() { "ms hit" } else if is_goal { "ms goal" } else { "ms" };
            let time = hit.map(|t| format!("{}:{:02}", t / 60, t % 60)).unwrap_or_else(|| "·".into());
            view! {
                <div class=cls title=MS_NAMES[m]>
                    <span class="ms-name">{MS_NAMES[m]}</span>
                    <span class="ms-time">{time}</span>
                </div>
            }
        })
        .collect_view()
}

#[component]
fn BotCard(idx: usize, bot: RwSignal<BotView>) -> impl IntoView {
    // rust-race-001 → 4601, etc. — mirrors viewer::port_for on the bot side.
    let port = 4601 + idx as u16;
    view! {
        <div class="card">
            <div class="card-head">
                <span class="bot-name">{move || bot.get().name}</span>
                <span class="phase" style=move || format!("background:{}", bot.get().phase_color)>
                    {move || bot.get().phase}
                </span>
            </div>
            <div class="step">
                {move || bot.get().current_step} " "
                <span class="frac">{move || format!("{}/{}", bot.get().done, bot.get().total)}</span>
            </div>
            // Live first-person 3D world — static markup so Leptos never re-creates the
            // canvas; /mount.js binds it to http://localhost:{port} once it appears.
            <canvas class="v3d" data-vport=port.to_string()></canvas>
            <div class="stats">
                <span>"pick " {move || bot.get().pick}</span>
                <span>"y " {move || bot.get().y}</span>
                <span>"log " {move || bot.get().logs}</span>
                <span>"plk " {move || bot.get().planks}</span>
                <span>"stk " {move || bot.get().sticks}</span>
                <span>"cob " {move || bot.get().cobble}</span>
                <span class="deaths">"☠ " {move || bot.get().deaths}</span>
            </div>
            <div class="ms-grid">{move || ms_cells(&bot.get())}</div>
            <details class="log">
                <summary>"recent"</summary>
                <pre>{move || bot.get().recent.join("\n")}</pre>
            </details>
        </div>
    }
}
