//! The server jar from Mojang's version manifest (sha1-checked), and the reports its own data
//! generator writes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;
use sha1::{Digest, Sha1};

/// The one Minecraft version ruststeve targets (protocol 775); every table is generated from it.
pub const VERSION: &str = "26.1.2";

/// Mojang's list of every released version and where its metadata lives.
const MANIFEST: &str = "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("GET {url}"))?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut bytes)
        .with_context(|| format!("read {url}"))?;
    Ok(bytes)
}

fn get_json(url: &str) -> Result<Value> {
    serde_json::from_slice(&get_bytes(url)?).with_context(|| format!("parse {url}"))
}

fn sha1_hex(bytes: &[u8]) -> String {
    Sha1::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `<work>/server.jar`, downloaded unless a copy with the manifest's sha1 is already there.
pub fn server_jar(work: &Path) -> Result<PathBuf> {
    let manifest = get_json(MANIFEST)?;
    let entry = manifest["versions"]
        .as_array()
        .and_then(|vs| vs.iter().find(|v| v["id"] == VERSION))
        .with_context(|| format!("{VERSION} is not in the version manifest"))?;
    let meta_url = entry["url"].as_str().context("version entry has no url")?;
    let meta = get_json(meta_url)?;
    let server = &meta["downloads"]["server"];
    let url = server["url"].as_str().context("no server download")?;
    let sha1 = server["sha1"].as_str().context("no server sha1")?;

    let path = work.join("server.jar");
    if let Ok(bytes) = std::fs::read(&path)
        && sha1_hex(&bytes) == sha1
    {
        println!("server.jar {VERSION} cached, sha1 {sha1}");
        return Ok(path);
    }
    let bytes = get_bytes(url)?;
    let got = sha1_hex(&bytes);
    if got != sha1 {
        bail!("server.jar sha1 {got}, manifest says {sha1}");
    }
    std::fs::write(&path, &bytes).with_context(|| format!("write {}", path.display()))?;
    println!("server.jar {VERSION} downloaded, sha1 {sha1}");
    Ok(path)
}

/// `<work>/reports/reports`: the server's own data generator run with `--reports` (Java 25 on the
/// runner). Skipped when the reports are already there for this version.
pub fn reports(work: &Path) -> Result<PathBuf> {
    let dir = work.join("reports").join("reports");
    let stamp = work.join("reports").join("version");
    if dir.join("packets.json").exists()
        && std::fs::read_to_string(&stamp).is_ok_and(|v| v == VERSION)
    {
        println!("reports for {VERSION} cached");
        return Ok(dir);
    }
    let status = Command::new("java")
        .current_dir(work)
        .args([
            "-DbundlerMainClass=net.minecraft.data.Main",
            "-jar",
            "server.jar",
            "--reports",
            "--output",
            "reports",
        ])
        .status()
        .context("run java (Java 25 must be on PATH)")?;
    if !status.success() {
        bail!("the server's data generator failed: {status}");
    }
    if !dir.join("packets.json").exists() {
        bail!(
            "the data generator wrote no {}",
            dir.join("packets.json").display()
        );
    }
    std::fs::write(&stamp, VERSION).with_context(|| format!("write {}", stamp.display()))?;
    Ok(dir)
}
