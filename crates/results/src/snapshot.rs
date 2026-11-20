//! Snapshots (Bridger, 2026-11-19, `brain/ruststeve/decisions/one-bot-first.md`): when a bot first
//! reaches a stage, the race's server world is copied with what the bot holds in memory, so every
//! later attempt resumes from that real moment instead of from spawn or a made-up checkpoint. A
//! resumed bot takes the snapshot bot's player file (items, position, dimension, health) under its
//! own offline UUID.

use std::path::Path;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

/// What `snap.toml` says about a snapshot.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    /// The stage reached (a `Stage` name).
    pub stage: String,
    pub bot: String,
    /// The bot's offline UUID: its player file is `world/playerdata/<uuid>.dat`.
    pub uuid: String,
    pub server_ticks: u64,
    pub commit: String,
    pub run: String,
    pub lane: u32,
    pub pos: [f64; 3],
    pub dimension: String,
}

/// The UUID an offline-mode server gives `name` (vanilla's `UUIDUtil.createOfflinePlayerUUID`:
/// version 3, MD5 of `OfflinePlayer:<name>`), as the player file names it.
pub fn offline_uuid(name: &str) -> String {
    let mut b: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    b[6] = (b[6] & 0x0f) | 0x30;
    b[8] = (b[8] & 0x3f) | 0x80;
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

/// The bot's memory to a file: MessagePack with named fields, zstd (the recordings' encoding).
pub fn write_memory(path: &Path, memory: &memory::Memory) -> Result<(), String> {
    let bytes = rmp_serde::to_vec_named(memory).map_err(|e| e.to_string())?;
    let packed = zstd::encode_all(bytes.as_slice(), 3).map_err(|e| e.to_string())?;
    std::fs::write(path, packed).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn read_memory(path: &Path) -> Result<memory::Memory, String> {
    let packed = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bytes = zstd::decode_all(packed.as_slice()).map_err(|e| e.to_string())?;
    rmp_serde::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Copy a directory tree, leaving out the server's `session.lock` (held while it runs).
pub fn copy_dir(from: &Path, to: &Path) -> Result<u64, String> {
    std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut n = 0;
    for entry in std::fs::read_dir(from).map_err(|e| format!("{}: {e}", from.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        if name == "session.lock" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            n += copy_dir(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The widely published offline UUID for Notch.
    #[test]
    fn the_offline_uuid_matches_vanilla() {
        assert_eq!(
            offline_uuid("Notch"),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
    }

    #[test]
    fn a_snap_round_trips() {
        let s = Snap {
            stage: "nether".to_owned(),
            bot: "rs-l00-b03".to_owned(),
            uuid: offline_uuid("rs-l00-b03"),
            server_ticks: 41_200,
            commit: "abc1234".to_owned(),
            run: "local".to_owned(),
            lane: 0,
            pos: [1.5, 64.0, -3.5],
            dimension: "minecraft:the_nether".to_owned(),
        };
        let text = toml::to_string(&s).expect("toml");
        assert_eq!(toml::from_str::<Snap>(&text).expect("back"), s);
    }
}
