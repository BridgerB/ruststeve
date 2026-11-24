//! Issue 16e, phase D: server truth is for a reader only. No core crate may depend on `results`
//! (where truth lives) or `rcon` (how it is asked), so nothing the server knows can reach the
//! planner: strict no X-ray, held by the crate graph.

const CORE: [&str; 8] = [
    "data", "protocol", "world", "physics", "memory", "plan", "sim", "observe",
];

#[test]
fn no_core_crate_can_read_server_truth() {
    let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for name in CORE {
        let manifest = std::fs::read_to_string(crates.join(name).join("Cargo.toml")).unwrap();
        let deps: String = manifest
            .split("\n[")
            .filter(|s| s.starts_with("dependencies") || s.starts_with("dev-dependencies"))
            .collect();
        for banned in ["results", "rcon"] {
            assert!(
                !deps
                    .lines()
                    .any(|l| l.trim_start().starts_with(&format!("{banned} "))
                        || l.trim_start().starts_with(&format!("{banned}="))),
                "core crate {name} depends on {banned}: truth could reach the planner"
            );
        }
    }
}
