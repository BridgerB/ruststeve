//! Captured worlds (brain/ruststeve/design/parity-table.md): a real server's chunk columns
//! around a landing, kept byte for byte as the server sent them, one file per landing. The
//! server model plays the bot in them; the bot decodes them as it decodes any server's.

use std::path::Path;

use serde::{Deserialize, Serialize};
use sim::ModelWorld;

/// The format's version; a reader refuses one it does not know.
pub const FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    /// The `level_chunk_with_light` body as received.
    #[serde(with = "serde_bytes")]
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CapturedWorld {
    pub format_version: u32,
    /// The commit that captured it, the server's seed label and the landing.
    pub commit: String,
    pub seed: String,
    pub landing: results::parity::Landing,
    /// Columns captured each way around the landing's column.
    pub radius: i32,
    pub columns: Vec<Column>,
}

impl CapturedWorld {
    pub fn write(&self, path: &Path) -> Result<(), String> {
        results::write(path, self)
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let w: CapturedWorld = results::read(path)?;
        if w.format_version != FORMAT_VERSION {
            return Err(format!(
                "{}: captured format {}",
                path.display(),
                w.format_version
            ));
        }
        Ok(w)
    }

    /// The world for the model, the player spawning at `spawn`.
    pub fn model(&self, spawn: [f64; 3]) -> Result<ModelWorld, String> {
        ModelWorld::captured(
            self.columns.iter().map(|c| ((c.x, c.z), c.body.clone())),
            spawn,
        )
        .map_err(|e| e.to_string())
    }

    /// The file for landing `i` in a directory.
    pub fn path(dir: &Path, i: usize) -> std::path::PathBuf {
        dir.join(format!("landing-{i:02}.rsw"))
    }
}
