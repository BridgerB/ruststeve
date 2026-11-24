//! `config/observe.toml`: every clientbound packet the bot does not decode, as wanted (a blind
//! spot the summary marks) or skipped on purpose with its reason (issue 16e).

use std::path::Path;

use serde::Deserialize;

/// Where the classification lives, relative to the repository root.
pub const PATH: &str = "config/observe.toml";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Observe {
    #[serde(default)]
    pub wanted: Vec<Entry>,
    #[serde(default)]
    pub skipped: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Entry {
    /// `state/name`, as `protocol::packet_name` spells it.
    pub packet: String,
    pub why: String,
}

/// How a packet the bot threw away is classed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Class {
    /// Sent, thrown away, and needed: a blind spot.
    Wanted,
    /// Thrown away on purpose.
    Skipped,
    /// In neither list: a blind spot nobody has looked at.
    Unclassed,
}

impl Class {
    pub fn blind(self) -> bool {
        match self {
            Self::Wanted | Self::Unclassed => true,
            Self::Skipped => false,
        }
    }
}

impl Observe {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The classification from the repository root, or none when it is not there (the summary
    /// then calls every thrown-away packet unclassed).
    pub fn from_repo() -> Option<Self> {
        Self::load(Path::new(PATH)).ok()
    }

    pub fn class(&self, packet: &str) -> Class {
        if self.wanted.iter().any(|e| e.packet == packet) {
            Class::Wanted
        } else if self.skipped.iter().any(|e| e.packet == packet) {
            Class::Skipped
        } else {
            Class::Unclassed
        }
    }

    /// Every listed packet and how many lists name it (a sound classification names each once).
    #[cfg(test)]
    fn listed(&self) -> std::collections::BTreeMap<&str, u32> {
        let mut m: std::collections::BTreeMap<&str, u32> = std::collections::BTreeMap::new();
        for e in self.wanted.iter().chain(&self.skipped) {
            *m.entry(e.packet.as_str()).or_default() += 1;
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_file() -> Observe {
        let p = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(PATH);
        Observe::load(&p).unwrap()
    }

    /// Every clientbound packet is exactly one of decoded, wanted or skipped: a new packet in a
    /// new game version, or one decoded and still listed, fails here.
    #[test]
    fn every_clientbound_packet_is_classified() {
        let o = repo_file();
        let listed = o.listed();
        let twice: Vec<_> = listed.iter().filter(|(_, n)| **n > 1).collect();
        assert!(twice.is_empty(), "listed more than once: {twice:?}");
        let mut problems = Vec::new();
        let mut names = Vec::new();
        for (state, id) in protocol::clientbound() {
            let name = protocol::packet_name(state, id);
            let decoded = protocol::decodes(state, id);
            let listed = listed.contains_key(name.as_str());
            if decoded && listed {
                problems.push(format!("{name}: decoded and still listed"));
            }
            if !decoded && !listed {
                problems.push(format!("{name}: thrown away and in no list"));
            }
            names.push(name);
        }
        for p in listed.keys() {
            if !names.iter().any(|n| n == p) {
                problems.push(format!("{p}: no such clientbound packet"));
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn every_entry_says_why() {
        let o = repo_file();
        let bare: Vec<_> = o
            .wanted
            .iter()
            .chain(&o.skipped)
            .filter(|e| e.why.trim().is_empty())
            .map(|e| e.packet.clone())
            .collect();
        assert!(bare.is_empty(), "no reason given: {bare:?}");
    }

    #[test]
    fn a_packet_in_no_list_is_unclassed_and_blind() {
        let o = Observe::default();
        assert_eq!(o.class("play/sound"), Class::Unclassed);
        assert!(o.class("play/sound").blind());
    }
}
