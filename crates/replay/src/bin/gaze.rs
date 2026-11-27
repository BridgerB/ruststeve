//! gaze: every moment a run's look came inside an enderman's stare cone (issue 20, E7,
//! `docs/problems/end-gaze.md`), with the act that sent it. The race's `GazeNearMiss` fact counts
//! a stare from the look sent last at the standing eye with no line of sight; this re-reads the
//! recording tick by tick and prints, for each such moment, what the server's own test would say
//! (the crouching eye when sneaking, the line past seen full blocks), what the planner's
//! `look::stares` said when it chose the controls (the eye before the tick's move), the body's
//! move, the last act and the last planner note: the act whose look made the stare.
//!
//! ```text
//! cargo run -p replay --bin gaze -- <file.rsr> [--force] [--from <ticks>] [--to <ticks>]
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::process::ExitCode;

use data::EntityKind;
use protocol::metadata::MetaValue;
use record::{COMMIT, EventRecord, Folder, Record, Recording, check_commit};
use world::World;

/// The enderman's stared-at flag (metadata index 18, `plan::endermen::STARED_INDEX`).
const STARED_INDEX: u8 = 18;
/// A standing player's eye (`physics::EYE_HEIGHT`), the observer's.
const EYE: f64 = 1.62;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gaze: {e}");
            ExitCode::FAILURE
        }
    }
}

fn arg(args: &[String], name: &str) -> Option<u64> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

/// The look's dot with the way to an enderman's eye, in stare tolerances past the cone's edge
/// (above 0 is inside), and the range.
fn margin(eye: [f64; 3], yaw: f32, pitch: f32, e: [f64; 3]) -> Option<(f64, f64)> {
    let t = [e[0], e[1] + plan::look::ENDERMAN_EYE, e[2]];
    let d = [t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len <= f64::EPSILON || len > plan::look::STARE_RANGE {
        return None;
    }
    let v = plan::look::view(yaw, pitch);
    let dot = (v[0] * d[0] + v[1] * d[1] + v[2] * d[2]) / len;
    // dot > 1 - k * tol / len, so k = (1 - dot) * len / tol: inside the cone when k < 1.
    Some(((1.0 - dot) * len / plan::look::STARE_TOLERANCE, len))
}

fn stared(w: &World, id: i32) -> bool {
    w.entities
        .get(&id)
        .is_some_and(|e| matches!(e.metadata.get(&STARED_INDEX), Some(MetaValue::Bool(true))))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let force = args.iter().any(|a| a == "--force");
    let from = arg(&args, "--from").unwrap_or(0);
    let to = arg(&args, "--to").unwrap_or(u64::MAX);
    let file = args
        .iter()
        .enumerate()
        .find(|(i, a)| {
            !a.starts_with("--")
                && args
                    .get(i.wrapping_sub(1))
                    .is_none_or(|p| p != "--from" && p != "--to")
        })
        .map(|(_, a)| a.clone())
        .ok_or("usage: gaze <file.rsr> [--force] [--from <ticks>] [--to <ticks>]")?;
    let mut r = Recording::open(Path::new(&file))?;
    let header = match r.next_record()? {
        Some(Record::Header(h)) => h,
        Some(_) | None => return Err("the recording has no header".to_owned()),
    };
    check_commit(&header, COMMIT, force)?;
    println!(
        "gaze of {} (recorded at {}, folded at {COMMIT})",
        header.bot_name, header.commit
    );
    println!(
        "columns: tick id range k(observer: <1 inside) k(server eye) los sneak | plan said | sent yaw,pitch | feet vel | enderman | last act | last note"
    );
    let mut folder = Folder::new();
    let mut controls = None;
    let mut plan_said: Option<u32> = None;
    let mut last_act = String::new();
    let mut last_note = String::new();
    let mut inside: BTreeMap<i32, bool> = BTreeMap::new();
    let mut flagged: BTreeMap<i32, bool> = BTreeMap::new();
    let mut counts = (0u32, 0u32, 0u32);
    while let Some(rec) = r.next_record()? {
        match rec {
            Record::Event(e) => {
                let tick = matches!(e, EventRecord::Tick { .. });
                match &e {
                    EventRecord::Controls(c) => {
                        controls = Some(*c);
                        // What `keep_off` saw when it let these controls go: the eye before the
                        // tick's move.
                        let w = &folder.world;
                        plan_said = w.body.and_then(|b| {
                            let eye = plan::look::eye(b.pos, c);
                            #[allow(clippy::cast_possible_truncation)]
                            plan::look::stares(
                                w,
                                eye,
                                c.yaw as f32,
                                c.pitch.unwrap_or(w.player.pitch),
                            )
                        });
                    }
                    EventRecord::Act(a) => {
                        last_act = format!("{a:?} at {}", folder.world.session.server_ticks);
                    }
                    EventRecord::Packet { .. } | EventRecord::Tick { .. } => {}
                }
                folder.step(&e)?;
                if !tick {
                    continue;
                }
                let w = &folder.world;
                let at = w.session.server_ticks;
                let (Some(b), Some((yaw, pitch))) = (w.body, w.sent_look) else {
                    continue;
                };
                let sneak = controls.is_some_and(|c| c.sneak);
                let obs_eye = [b.pos[0], b.pos[1] + EYE, b.pos[2]];
                let srv_eye = [
                    b.pos[0],
                    b.pos[1] + if sneak { plan::look::SNEAK_EYE } else { EYE },
                    b.pos[2],
                ];
                for (id, en) in w
                    .entities
                    .iter()
                    .filter(|(_, e)| !e.dead && e.kind == EntityKind::Enderman)
                {
                    let flag = stared(w, *id);
                    if flag && !flagged.get(id).copied().unwrap_or(false) && at >= from && at <= to
                    {
                        counts.2 += 1;
                        println!(
                            "{at:>8} FLAG enderman#{id} stared-at flag set | {:.1},{:.1},{:.1} | last act {last_act} | last note {last_note}",
                            en.pos[0], en.pos[1], en.pos[2]
                        );
                    }
                    flagged.insert(*id, flag);
                    let Some((k, len)) = margin(obs_eye, yaw, pitch, en.pos) else {
                        inside.insert(*id, false);
                        continue;
                    };
                    let now = k < 1.0;
                    let was = inside.get(id).copied().unwrap_or(false);
                    inside.insert(*id, now);
                    if !now || was || at < from || at > to {
                        continue;
                    }
                    counts.0 += 1;
                    let (ks, _) = margin(srv_eye, yaw, pitch, en.pos).unwrap_or((f64::NAN, 0.0));
                    let t = [en.pos[0], en.pos[1] + plan::look::ENDERMAN_EYE, en.pos[2]];
                    let los = plan::look::clear(w, srv_eye, t);
                    if ks < 1.0 && los {
                        counts.1 += 1;
                    }
                    println!(
                        "{at:>8} STARE enderman#{id} {len:.1} k {k:.2} k_srv {ks:.2} los {los} sneak {sneak} | plan said {plan_said:?} | look {yaw:.1},{pitch:.1} ctl {:?} | feet {:.2},{:.2},{:.2} vel {:.2},{:.2},{:.2} | at {:.1},{:.1},{:.1} | last act {last_act} | last note {last_note}",
                        controls.map(|c| (c.yaw, c.pitch)),
                        b.pos[0],
                        b.pos[1],
                        b.pos[2],
                        b.vel[0],
                        b.vel[1],
                        b.vel[2],
                        en.pos[0],
                        en.pos[1],
                        en.pos[2],
                    );
                }
            }
            Record::Note(n) => n.clone_into(&mut last_note),
            Record::Checkpoint {
                event_index, world, ..
            } => {
                folder.take_start(event_index, &world);
            }
            Record::Header(_) | Record::Effects(_) => {}
        }
        if folder.world.session.server_ticks > to {
            break;
        }
    }
    println!(
        "{} looks into the observer's cone, {} of them a server stare (crouch eye, line clear); {} stared-at flags",
        counts.0, counts.1, counts.2
    );
    Ok(())
}
