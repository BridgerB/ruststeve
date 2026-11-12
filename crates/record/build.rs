//! Bake the commit this binary is built from into it, so a recording names the code that made it
//! and replay can refuse another commit's file (the handoff's lesson 4.5).

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn main() {
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    let dirty =
        git(&["status", "--porcelain", "--untracked-files=no"]).is_some_and(|s| !s.is_empty());
    println!("cargo:rustc-env=RUSTSTEVE_COMMIT={commit}");
    println!("cargo:rustc-env=RUSTSTEVE_DIRTY={dirty}");
    println!(
        "cargo:rustc-env=RUSTSTEVE_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    // Rebuild when HEAD or the index moves (a commit, a checkout, a staged change).
    if let Some(dir) = git(&["rev-parse", "--git-dir"]) {
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/index");
        if let Some(head) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!("cargo:rerun-if-changed={dir}/{head}");
        }
    }
}
