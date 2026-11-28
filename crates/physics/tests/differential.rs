//! Issue 06's first done-condition: ruststeve's physics step agrees with clojurecraft's, tick by
//! tick, on shared scenarios. `fixtures/expected.txt` was written once, outside this repository,
//! by running `clojurecraft.physics/step` (at be85a92) over `fixtures/scenarios.txt`. Where the
//! two read a tick's inertia differently (decision inertia-before-the-move), the horizontal
//! velocity is held to clojurecraft's times the ratio of the two inertias.

use std::collections::BTreeMap;

use physics::{Body, Controls, Fluid, FluidKind, step};

struct Scenario {
    name: String,
    solids: Vec<([i32; 6], f64)>,
    fluids: Vec<([i32; 6], Fluid)>,
    start: [f64; 3],
    ground: bool,
    ticks: Vec<Controls>,
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn scenarios() -> Vec<Scenario> {
    let mut out = Vec::new();
    let mut cur: Option<Scenario> = None;
    for line in fixture("scenarios.txt").lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let w: Vec<&str> = line.split_whitespace().collect();
        let range = |w: &[&str]| -> [i32; 6] {
            std::array::from_fn(|i| w[i].parse::<f64>().unwrap() as i32)
        };
        match w[0] {
            "scenario" => {
                cur = Some(Scenario {
                    name: w[1].to_owned(),
                    solids: Vec::new(),
                    fluids: Vec::new(),
                    start: [0.0; 3],
                    ground: false,
                    ticks: Vec::new(),
                });
            }
            "solid" => cur
                .as_mut()
                .unwrap()
                .solids
                .push((range(&w[1..7]), w[7].parse().unwrap())),
            "fluid" => {
                let kind = if w[7] == "water" {
                    FluidKind::Water
                } else {
                    FluidKind::Lava
                };
                let level = w[8].parse().unwrap();
                cur.as_mut()
                    .unwrap()
                    .fluids
                    .push((range(&w[1..7]), Fluid { kind, level }));
            }
            "start" => {
                let s = cur.as_mut().unwrap();
                s.start = [
                    w[1].parse().unwrap(),
                    w[2].parse().unwrap(),
                    w[3].parse().unwrap(),
                ];
                s.ground = w[4] == "ground";
            }
            "ticks" => {
                let n: usize = w[1].parse().unwrap();
                let c = Controls {
                    forward: w[2] == "1",
                    jump: w[3] == "1",
                    sneak: w[4] == "1",
                    yaw: w[5].parse().unwrap(),
                    pitch: None,
                    sprint: false,
                };
                cur.as_mut()
                    .unwrap()
                    .ticks
                    .extend(std::iter::repeat_n(c, n));
            }
            "end" => out.push(cur.take().unwrap()),
            other => panic!("unknown line {other}"),
        }
    }
    out
}

fn inside(r: &[i32; 6], x: i32, y: i32, z: i32) -> bool {
    (r[0]..=r[3]).contains(&x) && (r[1]..=r[4]).contains(&y) && (r[2]..=r[5]).contains(&z)
}

/// `(scenario, tick) -> the expected fields as written`.
fn expected() -> BTreeMap<(String, usize), Vec<String>> {
    fixture("expected.txt")
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let w: Vec<String> = l.split_whitespace().map(str::to_owned).collect();
            ((w[0].clone(), w[1].parse().unwrap()), w[2..].to_vec())
        })
        .collect()
}

fn b(v: bool) -> &'static str {
    if v { "1" } else { "0" }
}

/// The scenarios whose horizontal velocity differs from clojurecraft's by design
/// (brain/ruststeve/decisions/inertia-before-the-move.md): a non-liquid tick where the body stood
/// on the ground before the move and not after, or the other way, with a horizontal velocity to
/// scale. clojurecraft takes that tick's inertia from where the body ended (`landed`), ruststeve
/// from where it stood (`LivingEntity.travelInAir`'s `onGround()` before the move). A walk from a
/// fixture's standing start diverges on its first tick: the start has no downward velocity, so
/// tick 0 does not land.
const DIVERGING: [&str; 7] = [
    "flat-walk",
    "wall",
    "jump-onto-block",
    "carpet",
    "off-a-ledge",
    "bank-impulse",
    "turning-walk",
];

const GROUND_INERTIA: f64 = physics::GROUND_INERTIA;
const AIR_INERTIA: f64 = physics::AIR_INERTIA;

/// A body from one expected line (the state after that tick), as the next tick reads it.
fn body_of(w: &[String]) -> Body {
    let num = |k: usize| -> f64 { w[k].parse().unwrap() };
    Body {
        pos: [num(0), num(1), num(2)],
        vel: [num(3), num(4), num(5)],
        on_ground: w[6] == "1",
        horizontal_collision: w[7] == "1",
        jump_ticks: w[8].parse().unwrap(),
        fall_distance: num(9),
        in_water: w[10] == "1",
        in_lava: w[11] == "1",
        eye_in_water: w[12] == "1",
        ..Body::default()
    }
}

/// Where the two inertia rules part on this tick: the factor that turns clojurecraft's horizontal
/// velocity into ruststeve's (`None` where they agree). `before` stood on the ground before the
/// move; `w` is clojurecraft's state after it.
fn inertia_ratio(before: bool, w: &[String]) -> Option<f64> {
    let after = w[6] == "1";
    let liquid = w[10] == "1" || w[11] == "1";
    let inertia = |ground: bool| if ground { GROUND_INERTIA } else { AIR_INERTIA };
    (before != after && !liquid).then(|| inertia(before) / inertia(after))
}

/// The fields of `body` that differ from the expected line `w`. `ratio` scales the expected
/// horizontal velocity, compared then within a float's rounding instead of bit for bit.
fn diffs(body: &Body, w: &[String], ratio: Option<f64>) -> Vec<String> {
    let num = |k: usize| -> f64 { w[k].parse().unwrap() };
    let mut out = Vec::new();
    let floats = [
        body.pos[0],
        body.pos[1],
        body.pos[2],
        body.vel[0],
        body.vel[1],
        body.vel[2],
    ];
    for (k, got) in floats.iter().enumerate() {
        let horizontal = k == 3 || k == 5;
        match ratio {
            Some(r) if horizontal => {
                let want = num(k) * r;
                if (got - want).abs() > 1e-12 * want.abs().max(1e-3) {
                    out.push(format!("field {k}: {got:?} vs {want:?} (scaled {r})"));
                }
            }
            _ => {
                if *got != num(k) {
                    out.push(format!("field {k}: {got:?} vs {:?}", num(k)));
                }
            }
        }
    }
    let flags = [
        b(body.on_ground),
        b(body.horizontal_collision),
        &body.jump_ticks.to_string(),
    ]
    .map(str::to_owned);
    for (k, got) in flags.iter().enumerate() {
        if *got != w[6 + k] {
            out.push(format!("flag {k}: {got} vs {}", w[6 + k]));
        }
    }
    if body.fall_distance != num(9) {
        out.push(format!("fall: {:?} vs {:?}", body.fall_distance, num(9)));
    }
    for (k, got) in [b(body.in_water), b(body.in_lava), b(body.eye_in_water)]
        .iter()
        .enumerate()
    {
        if *got != w[10 + k] {
            out.push(format!("fluid flag {k}: {got} vs {}", w[10 + k]));
        }
    }
    out
}

/// Does this tick part the rules visibly: a ratio, and a horizontal velocity for it to scale?
fn parts(before: bool, w: &[String]) -> bool {
    let moving = w[3].parse::<f64>().unwrap() != 0.0 || w[5].parse::<f64>().unwrap() != 0.0;
    inertia_ratio(before, w).is_some() && moving
}

fn report(wrong: &[String], ticks: usize) -> String {
    format!(
        "{} of {ticks} ticks differ:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Run as a chain, ruststeve's own state feeding each tick: every tick of every scenario not in
/// `DIVERGING`, and of a diverging one every tick before its first parting, equal bit for bit.
/// Every listed scenario must part somewhere, and no other may, so the list cannot rot.
#[test]
fn every_tick_equals_clojurecraft() {
    let want = expected();
    let mut ticks = 0;
    let mut wrong = Vec::new();
    for s in scenarios() {
        let solid = |x: i32, y: i32, z: i32| {
            s.solids
                .iter()
                .find(|(r, _)| inside(r, x, y, z))
                .map(|(_, top)| *top)
        };
        let fluid = |x: i32, y: i32, z: i32| {
            s.fluids
                .iter()
                .find(|(r, _)| inside(r, x, y, z))
                .map(|(_, f)| *f)
        };
        let listed = DIVERGING.contains(&s.name.as_str());
        let mut before = s.ground;
        let parting = (0..s.ticks.len()).find(|&i| {
            let w = &want[&(s.name.clone(), i)];
            let p = parts(before, w);
            before = w[6] == "1";
            p
        });
        assert_eq!(
            listed,
            parting.is_some(),
            "{}: listed as diverging {listed}, parts at {parting:?}",
            s.name
        );
        let upto = parting.unwrap_or(s.ticks.len());
        let mut body = Body {
            pos: s.start,
            on_ground: s.ground,
            ..Body::default()
        };
        for (i, c) in s.ticks.iter().enumerate().take(upto) {
            body = step(&solid, &fluid, &body, c);
            ticks += 1;
            let d = diffs(&body, &want[&(s.name.clone(), i)], None);
            if !d.is_empty() {
                wrong.push(format!("{} tick {i}: {}", s.name, d.join("; ")));
            }
        }
    }
    assert!(ticks > 0);
    assert!(wrong.is_empty(), "{}", report(&wrong, ticks));
}

/// Every tick of every scenario as one step from clojurecraft's state before it: equal bit for
/// bit, except where the inertia rules part, where the horizontal velocity is clojurecraft's
/// times the ratio of the two inertias and nothing else differs.
#[test]
fn every_step_from_clojurecraft_state_agrees() {
    let want = expected();
    let mut ticks = 0;
    let mut parted = 0;
    let mut wrong = Vec::new();
    for s in scenarios() {
        let solid = |x: i32, y: i32, z: i32| {
            s.solids
                .iter()
                .find(|(r, _)| inside(r, x, y, z))
                .map(|(_, top)| *top)
        };
        let fluid = |x: i32, y: i32, z: i32| {
            s.fluids
                .iter()
                .find(|(r, _)| inside(r, x, y, z))
                .map(|(_, f)| *f)
        };
        let mut prev = Body {
            pos: s.start,
            on_ground: s.ground,
            ..Body::default()
        };
        for (i, c) in s.ticks.iter().enumerate() {
            let w = &want[&(s.name.clone(), i)];
            let ratio = inertia_ratio(prev.on_ground, w);
            parted += usize::from(ratio.is_some());
            let body = step(&solid, &fluid, &prev, c);
            ticks += 1;
            let d = diffs(&body, w, ratio);
            if !d.is_empty() {
                wrong.push(format!("{} tick {i}: {}", s.name, d.join("; ")));
            }
            prev = body_of(w);
        }
    }
    assert_eq!(ticks, want.len(), "every expected tick was run");
    assert!(
        parted > 0,
        "the inertia rules never part: the test tests nothing"
    );
    assert!(wrong.is_empty(), "{}", report(&wrong, ticks));
}
