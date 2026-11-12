//! The landing measure (issue 09, brain/ruststeve/design/planner.md): the planner in each
//! captured landing, against the server model, in lockstep. A landing qualifies when its start
//! is dry ground and a log 30+ blocks away is in view once the bot has joined *and some walking
//! route to one exists* (Bridger, 2026-11-12: pre-checked as clojurecraft pre-checked its landing
//! sets; a log reachable only by swimming, digging or pillaring waits for issues 11 and 12). The
//! measure is how many qualifying landings reach a log, the longest any walk went without getting
//! closer, and the planner's time per tick against the race config's budget; the raw rate over
//! every landing with any log in view is reported beside it.

use std::path::Path;
use std::time::Instant;

use results::config::Budget;
use results::parity::Landings;

use crate::captured::CapturedWorld;

/// The pre-check's search: far past any planner budget, so "no route" means none exists.
pub const PRECHECK_EXPANSIONS: u32 = 200_000;
/// Logs the pre-check routes to, nearest first, before it calls a landing enclosed.
pub const PRECHECK_LOGS: usize = 40;

/// Is a log in view 30+ blocks out walkable from the start at all (the qualification's
/// pre-check; a harness question, outside the planner and its CPU budget)?
pub fn walkable_log(w: &world::World, start: [f64; 3], min: f64) -> bool {
    let (Some(dim), Some(logs)) = (
        w.dimension.as_ref(),
        data::generated::tags::block_tag("logs"),
    ) else {
        return false;
    };
    let flat = |p: (i32, i32, i32)| {
        ((start[0] - (f64::from(p.0) + 0.5)).powi(2) + (start[2] - (f64::from(p.2) + 0.5)).powi(2))
            .sqrt()
    };
    let mut cands: Vec<(i32, i32, i32)> = w
        .memory
        .log()
        .iter()
        .filter(|s| s.dimension == dim.name)
        .map(|s| s.pos)
        .filter(|p| {
            w.memory
                .latest(&dim.name, *p)
                .is_some_and(|l| data::block_of_state(l.state).is_some_and(|k| logs.contains(&k)))
        })
        .filter(|p| flat(*p) >= min)
        .collect();
    cands.sort_by(|a, b| flat(*a).total_cmp(&flat(*b)).then(a.cmp(b)));
    cands.dedup();
    let feet = plan::path::feet_cell(start);
    cands.iter().take(PRECHECK_LOGS).any(|c| {
        plan::path::plan(
            w,
            feet,
            &plan::path::Goal::Near {
                pos: *c,
                range: plan::walk::GOAL_RANGE,
            },
            PRECHECK_EXPANSIONS,
        )
        .status
            == plan::path::Status::Found
    })
}

/// One landing's run.
#[derive(Clone, Debug, PartialEq)]
pub struct LandingRun {
    pub index: usize,
    /// Why it does not count, when it does not: `wet` (the start is water), `no log` (none 30+
    /// away in view after joining), `not walkable` (in view, but no walking route to any),
    /// `missing` (no captured world).
    pub excluded: Option<&'static str>,
    pub reached: bool,
    pub stuck: bool,
    pub plan: Option<plan::Plan>,
    pub tick_us: Vec<u64>,
    pub corrections: u64,
}

/// Run the planner in landing `i`.
pub fn run_one(
    worlds: &Path,
    landings: &Landings,
    i: usize,
    config: sim::Config,
    aim: plan::Aim,
    ticks: u64,
    table: Option<std::sync::Arc<plan::make::Table>>,
) -> LandingRun {
    let empty = |excluded| LandingRun {
        index: i,
        excluded: Some(excluded),
        reached: false,
        stuck: false,
        plan: None,
        tick_us: Vec::new(),
        corrections: 0,
    };
    let (Some(l), Ok(captured)) = (
        landings.landing.get(i),
        CapturedWorld::read(&CapturedWorld::path(worlds, i)),
    ) else {
        return empty("missing");
    };
    let Ok(model) = captured.model([0.0; 3]) else {
        return empty("missing");
    };
    // A landing whose own spot is water is a wet start, excluded (it would otherwise borrow a
    // shore nearby and change what is measured).
    let centre_wet = model
        .ground(l.x, l.z)
        .and_then(|y| {
            model
                .columns
                .get(&(l.x >> 4, l.z >> 4))
                .and_then(|c| c.column.state_at(l.x, y.floor() as i32, l.z))
        })
        .and_then(data::block_of_state)
        .is_some_and(|k| k == data::BlockKind::Water);
    if centre_wet {
        return empty("wet");
    }
    // Placed as vanilla places a player: on the ground under leaves, with two free cells, at the
    // nearest such column within 8 blocks of the landing.
    let Some((sx, y, sz)) = model.placeable(l.x, l.z, 8) else {
        return empty("missing");
    };
    // The surface is in water when its cell holds water: a wet start.
    let wet = model
        .columns
        .get(&(sx >> 4, sz >> 4))
        .and_then(|c| c.column.state_at(sx, y.floor() as i32, sz))
        .and_then(data::block_of_state)
        .is_some_and(|k| k == data::BlockKind::Water);
    if wet {
        return empty("wet");
    }
    let start = [f64::from(sx) + 0.5, y, f64::from(sz) + 0.5];
    let Ok(world) = captured.model(start) else {
        return empty("missing");
    };
    // A goal aim (issue 10's `table`) qualifies on a walkable log in view at any distance.
    let min_distance = match aim {
        plan::Aim::ReachLog { min_distance } => min_distance,
        plan::Aim::Table => 0.0,
    };
    let mut p = match table {
        Some(t) => plan::Plan::with_table(aim, t),
        None => plan::Plan::new(aim),
    };
    let mut seen: Option<(bool, bool)> = None;
    let mut tick_us = Vec::new();
    let run = crate::lockstep::run_acting(
        sim::Sim::new(config, world).seeded(i as u64 + 1),
        &format!("rs-l{i}"),
        i as u64 + 1,
        ticks,
        crate::SETTLE_TICKS,
        |w, _| {
            if seen.is_none() {
                let in_view = plan::log_in_view(w, start, min_distance);
                seen = Some((in_view, in_view && walkable_log(w, start, min_distance)));
            }
            let t0 = Instant::now();
            let c = p.step(w);
            tick_us.push(u64::try_from(t0.elapsed().as_micros()).unwrap_or(u64::MAX));
            (c, p.take_acts())
        },
    );
    let corrections = run
        .teleports_at_script
        .map_or(0, |t| run.world.session.teleports.saturating_sub(t));
    LandingRun {
        index: i,
        excluded: match seen {
            Some((true, true)) => None,
            Some((true, false)) => Some("not walkable"),
            _ => Some("no log"),
        },
        reached: p.reached.is_some() || p.met,
        stuck: p.stuck,
        plan: Some(p),
        tick_us,
        corrections,
    }
}

/// The measure over runs: counts, stalls, the CPU, and whether each done-condition holds.
pub struct Measure {
    pub text: String,
    pub pass: bool,
}

pub fn measure(
    runs: &[LandingRun],
    what: &str,
    want: usize,
    pass_rate: f64,
    stall_limit: u64,
    budget: Option<&Budget>,
) -> Measure {
    let qualifying: Vec<&LandingRun> = runs
        .iter()
        .filter(|r| r.excluded.is_none())
        .take(want)
        .collect();
    let n = qualifying.len();
    let k = qualifying.iter().filter(|r| r.reached).count();
    let stalls_over = runs
        .iter()
        .filter_map(|r| r.plan.as_ref())
        .filter(|p| p.longest_stall > stall_limit)
        .count();
    let longest = runs
        .iter()
        .filter_map(|r| r.plan.as_ref())
        .map(|p| p.longest_stall)
        .max()
        .unwrap_or(0);
    let mut us: Vec<u64> = runs
        .iter()
        .flat_map(|r| r.tick_us.iter().copied())
        .collect();
    us.sort_unstable();
    let mean = if us.is_empty() {
        0
    } else {
        us.iter().sum::<u64>() / us.len() as u64
    };
    let p99 = us
        .get((us.len() * 99 / 100).min(us.len().saturating_sub(1)))
        .copied()
        .unwrap_or(0);
    let max = us.last().copied().unwrap_or(0);
    let excluded = |why| runs.iter().filter(|r| r.excluded == Some(why)).count();
    // The raw rate: every landing with any log in view, walkable or not.
    let any: Vec<&LandingRun> = runs
        .iter()
        .filter(|r| r.excluded.is_none() || r.excluded == Some("not walkable"))
        .collect();
    let any_k = any.iter().filter(|r| r.reached).count();
    let corrections: u64 = runs.iter().map(|r| r.corrections).sum();
    let stuck = qualifying.iter().filter(|r| r.stuck).count();
    let rate = if n == 0 { 0.0 } else { k as f64 / n as f64 };
    let reach_ok = n >= want && rate >= pass_rate;
    let stall_ok = stalls_over == 0;
    let budget_ok = budget.is_none_or(|b| mean <= b.plan_tick_mean_us && p99 <= b.plan_tick_p99_us);
    let mut failures: std::collections::BTreeMap<String, u64> = std::collections::BTreeMap::new();
    for p in qualifying.iter().filter_map(|r| r.plan.as_ref()) {
        for (f, c) in &p.failures {
            *failures.entry(f.clone()).or_default() += c;
        }
    }
    let unreached: Vec<String> = qualifying
        .iter()
        .filter(|r| !r.reached)
        .map(|r| {
            let last = r
                .plan
                .as_ref()
                .and_then(|p| p.events.last())
                .map_or(String::new(), |e| format!("{} ({})", e.what, e.why));
            format!("landing {}: {}", r.index, last)
        })
        .collect();
    let mut lines = vec![
        format!(
            "**{what}: {k}/{n} qualifying landings ({:.1}%)**, need {want} at {:.0}%: {}",
            100.0 * rate,
            100.0 * pass_rate,
            if reach_ok { "PASS" } else { "FAIL" }
        ),
        format!(
            "**walks without progress past {stall_limit} ticks: {stalls_over}** (longest stall {longest} ticks): {}",
            if stall_ok { "PASS" } else { "FAIL" }
        ),
        format!(
            "**planner CPU per tick: mean {mean} us, p99 {p99} us, max {max} us** over {} ticks (budget {}): {}",
            us.len(),
            budget.map_or("none".to_owned(), |b| format!(
                "mean {} us, p99 {} us",
                b.plan_tick_mean_us, b.plan_tick_p99_us
            )),
            if budget_ok { "PASS" } else { "FAIL" }
        ),
        String::new(),
        format!(
            "landings run {}: qualifying {} (used {n}), excluded wet {}, no log in view {}, a log in view but none walkable {}, missing {}; stuck {stuck}; server corrections {corrections}",
            runs.len(),
            runs.iter().filter(|r| r.excluded.is_none()).count(),
            excluded("wet"),
            excluded("no log"),
            excluded("not walkable"),
            excluded("missing"),
        ),
        format!(
            "raw: any log in view, walkable or not: {any_k}/{} reached ({:.1}%)",
            any.len(),
            if any.is_empty() {
                0.0
            } else {
                100.0 * any_k as f64 / any.len() as f64
            }
        ),
        format!("walk failures over the qualifying landings: {failures:?}"),
    ];
    if !unreached.is_empty() {
        lines.push(String::new());
        lines.push("not reached:".to_owned());
        lines.extend(unreached.into_iter().map(|u| format!("- {u}")));
    }
    Measure {
        text: lines.join("\n") + "\n",
        pass: reach_ok && stall_ok && budget_ok,
    }
}
