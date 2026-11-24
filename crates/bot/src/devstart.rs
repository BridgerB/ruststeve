//! The guard on development starts (issue 16, docs/problems/race-tooling.md): a made-up kit at a
//! made-up place is for development only. A race from one marks every snapshot it takes
//! (`dev = true` in `snap.toml`, a `dev-` prefix on the release asset), a race resumed from a
//! marked snapshot marks its own, and the frontier (`config/frontier.toml`, race.yml's
//! `snapshot=frontier`) refuses a marked one: only a bot that came from spawn moves the frontier
//! (brain/ruststeve/decisions/one-bot-first.md).

/// The starts a frontier may come from: the world's own spawn, and a resume of a real snapshot.
/// Every other row of `config/starts.toml` gives a kit or a place.
pub const REAL_STARTS: [&str; 3] = ["snapshot", "fresh", "spawn"];

/// The asset prefix of a development snapshot.
pub const DEV_PREFIX: &str = "dev-";

/// Whether a start is a development start.
pub fn dev_start(start: &str) -> bool {
    !REAL_STARTS.contains(&start)
}

/// Whether a `snap.toml` is marked as a development snapshot.
pub fn snap_is_dev(snap_toml: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Mark {
        #[serde(default)]
        dev: bool,
    }
    toml::from_str::<Mark>(snap_toml).is_ok_and(|m| m.dev)
}

/// A `snap.toml` marked: the key goes first, before any table.
pub fn mark_dev(snap_toml: &str) -> String {
    if snap_is_dev(snap_toml) {
        return snap_toml.to_owned();
    }
    format!("dev = true\n{snap_toml}")
}

/// A `snap.toml` marked as a snapshot on request (`kind = "at"`) or of a stuck spot
/// (`kind = "stuck"`): a place to resume a look from, never a stage's snapshot.
pub fn mark_kind(snap_toml: &str, kind: &str) -> String {
    format!("kind = \"{kind}\"\n{snap_toml}")
}

/// The `kind` a `snap.toml` is marked with, if any.
pub fn snap_kind(snap_toml: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Mark {
        kind: Option<String>,
    }
    toml::from_str::<Mark>(snap_toml).ok().and_then(|m| m.kind)
}

/// The directory (and so the asset's name) of a snapshot that is not a stage's: `at-<ticks>` or
/// `stuck-<bot>`. No stage is named so (a test holds it).
pub fn request_dir(kind: &str, ticks: u64, bot: &str) -> String {
    match kind {
        "at" => format!("at-{ticks}"),
        _ => format!("{kind}-{bot}"),
    }
}

/// Whether a race's snapshots are development ones: its start is, or the snapshot it resumed is.
pub fn race_is_dev(start: &str, resumed_snap_toml: Option<&str>) -> bool {
    dev_start(start) || resumed_snap_toml.is_some_and(snap_is_dev)
}

/// Why `config/frontier.toml` may not stand, if it may not: its asset is a development snapshot.
pub fn frontier_refusal(frontier_toml: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Frontier {
        asset: String,
    }
    let f: Frontier = match toml::from_str(frontier_toml) {
        Ok(f) => f,
        Err(e) => return Some(format!("config/frontier.toml: {e}")),
    };
    if f.asset.starts_with("snapshot-at-") || f.asset.starts_with("snapshot-stuck-") {
        return Some(format!(
            "the frontier's asset {} is a snapshot on request or of a stuck spot, not a stage's",
            f.asset
        ));
    }
    f.asset.starts_with(DEV_PREFIX).then(|| {
        format!(
            "the frontier's asset {} is a development snapshot: only a bot from a real start \
             ({}) moves the frontier",
            f.asset,
            REAL_STARTS.join(", ")
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SNAP: &str =
        "stage = \"fortress\"\nbot = \"rs-l00-b00\"\nlane = 0\n\n[items]\nbread = 3\n";

    #[test]
    fn only_spawn_fresh_and_snapshot_are_real_starts() {
        for s in ["spawn", "fresh", "snapshot"] {
            assert!(!dev_start(s), "{s}");
        }
        for s in [
            "nether", "portal", "kit", "lava", "kitlava", "table", "stone", "anything",
        ] {
            assert!(dev_start(s), "{s}");
        }
    }

    /// Every start in the repository's file but the three real ones gives a kit or a place.
    #[test]
    fn the_real_starts_of_the_file_give_nothing() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/starts.toml"
        ))
        .unwrap();
        let starts: results::config::Starts = toml::from_str(&text).unwrap();
        for name in REAL_STARTS {
            let s = starts
                .find(name)
                .unwrap_or_else(|| panic!("no start {name}"));
            assert!(s.kit.is_empty(), "{name} gives a kit");
            assert!(
                matches!(&s.place, results::config::Place::Named(p) if p == "spawn"),
                "{name} gives a place"
            );
        }
    }

    #[test]
    fn a_marked_snap_reads_as_dev_and_still_parses() {
        assert!(!snap_is_dev(SNAP));
        let marked = mark_dev(SNAP);
        assert!(marked.starts_with("dev = true\n"));
        assert!(snap_is_dev(&marked));
        // The mark is a top-level key, not one of the items.
        let v: toml::Table = toml::from_str(&marked).unwrap();
        assert_eq!(v.get("dev").and_then(toml::Value::as_bool), Some(true));
        assert_eq!(v["items"].as_table().unwrap().len(), 1);
        assert_eq!(mark_dev(&marked), marked);
    }

    #[test]
    fn a_race_resumed_from_a_dev_snapshot_is_dev() {
        assert!(race_is_dev("nether", None));
        assert!(!race_is_dev("snapshot", Some(SNAP)));
        assert!(race_is_dev("snapshot", Some(&mark_dev(SNAP))));
        assert!(!race_is_dev("spawn", None));
    }

    #[test]
    fn the_frontier_refuses_a_dev_asset() {
        let dev = "stage = \"spawner\"\ntag = \"race-1\"\nasset = \"dev-snapshot-spawner-1-race-1-lane-0.tar.zst\"\n";
        assert!(frontier_refusal(dev).is_some());
        let real = "stage = \"fortress\"\ntag = \"race-1\"\nasset = \"snapshot-fortress-1-race-1-lane-0.tar.zst\"\n";
        assert_eq!(frontier_refusal(real), None);
    }

    #[test]
    fn a_snapshot_on_request_is_marked_and_never_a_stage_or_the_frontier() {
        let at = mark_kind(SNAP, "at");
        assert_eq!(snap_kind(&at).as_deref(), Some("at"));
        assert_eq!(snap_kind(SNAP), None);
        // Both marks together still parse, each a top-level key.
        let both = mark_dev(&mark_kind(SNAP, "stuck"));
        assert!(snap_is_dev(&both));
        assert_eq!(snap_kind(&both).as_deref(), Some("stuck"));
        assert_eq!(request_dir("at", 36_000, "rs-l00-b00"), "at-36000");
        assert_eq!(request_dir("stuck", 0, "rs-l00-b00"), "stuck-rs-l00-b00");
        for dir in ["at-36000", "stuck-rs-l00-b00"] {
            assert!(world::stage::Stage::from_name(dir).is_none());
            let frontier = format!(
                "stage = \"nether\"\ntag = \"race-1\"\nasset = \"snapshot-{dir}-1-race-1-lane-0.tar.zst\"\n"
            );
            assert!(frontier_refusal(&frontier).is_some(), "{dir}");
        }
    }

    /// The repository's own frontier: CI fails the commit that points it at a dev snapshot.
    #[test]
    fn the_repository_frontier_is_not_a_dev_snapshot() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config/frontier.toml"
        ))
        .unwrap();
        assert_eq!(frontier_refusal(&text), None);
    }
}
