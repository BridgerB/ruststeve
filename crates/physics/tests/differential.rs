//! Issue 06's first done-condition: ruststeve's physics step agrees with clojurecraft's, tick by
//! tick, on shared scenarios. `fixtures/expected.txt` was written once, outside this repository,
//! by running `clojurecraft.physics/step` (at be85a92) over `fixtures/scenarios.txt`.

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
        let mut body = Body {
            pos: s.start,
            on_ground: s.ground,
            ..Body::default()
        };
        for (i, c) in s.ticks.iter().enumerate() {
            body = step(&solid, &fluid, &body, c);
            ticks += 1;
            let w = &want[&(s.name.clone(), i)];
            let num = |k: usize| -> f64 { w[k].parse().unwrap() };
            let floats = [
                body.pos[0],
                body.pos[1],
                body.pos[2],
                body.vel[0],
                body.vel[1],
                body.vel[2],
            ];
            let mut diffs = Vec::new();
            for (k, got) in floats.iter().enumerate() {
                if *got != num(k) {
                    diffs.push(format!("field {k}: {got:?} vs {:?}", num(k)));
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
                    diffs.push(format!("flag {k}: {got} vs {}", w[6 + k]));
                }
            }
            if body.fall_distance != num(9) {
                diffs.push(format!("fall: {:?} vs {:?}", body.fall_distance, num(9)));
            }
            for (k, got) in [b(body.in_water), b(body.in_lava), b(body.eye_in_water)]
                .iter()
                .enumerate()
            {
                if *got != w[10 + k] {
                    diffs.push(format!("fluid flag {k}: {got} vs {}", w[10 + k]));
                }
            }
            if !diffs.is_empty() {
                wrong.push(format!("{} tick {i}: {}", s.name, diffs.join("; ")));
            }
        }
    }
    assert_eq!(ticks, want.len(), "every expected tick was run");
    assert!(
        wrong.is_empty(),
        "{} of {ticks} ticks differ:\n{}",
        wrong.len(),
        wrong
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
