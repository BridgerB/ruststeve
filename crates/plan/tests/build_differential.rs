//! Issue 14c: `plan::build`'s portal plans against clojurecraft's own (`build/plan-for` at
//! be85a92, a one-off oracle run in its flake shell outside this repository): every op of the
//! cast-one, portal and nether plans for every facing, lava heights 0, 1, 3 and 6, and three
//! origins, in the oracle's text (brain/ruststeve/design/portal-cast.md).

use plan::build::{Facing, Frame, cast_one_plan, frame_plan, op_text};

#[test]
fn portal_plans_equal_clojurecraft() {
    // zstd text: 4.9 MB plain, over the repository's large-file check.
    let packed = include_bytes!("fixtures/build-expected-portal.txt.zst");
    let expected = String::from_utf8(zstd::decode_all(&packed[..]).unwrap()).unwrap();
    let mut lines = Vec::new();
    for origin in [(0, 64, 0), (13, -50, -7), (-200, 12, 77)] {
        for facing in [Facing::South, Facing::West, Facing::North, Facing::East] {
            for h in [0, 1, 3, 6] {
                for build in ["cast-one", "portal", "nether"] {
                    let f = Frame { origin, facing };
                    let plan = match build {
                        "cast-one" => cast_one_plan(f, h),
                        "portal" => frame_plan(f, false, h),
                        _ => frame_plan(f, true, h),
                    };
                    lines.push(format!(
                        "# :{build} [{} {} {}] :{} {h}",
                        origin.0,
                        origin.1,
                        origin.2,
                        facing.name()
                    ));
                    lines.extend(plan.iter().map(op_text));
                }
            }
        }
    }
    let want: Vec<&str> = expected.lines().collect();
    let first_bad = lines.iter().zip(&want).position(|(a, b)| a != b);
    if let Some(i) = first_bad {
        panic!(
            "line {}:\n  rust:   {}\n  oracle: {}",
            i + 1,
            lines[i],
            want[i]
        );
    }
    assert_eq!(lines.len(), want.len(), "line counts differ");
}
