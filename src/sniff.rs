//! Packet sniffer — a queryable, SQLite-backed capture of the raw protocol
//! traffic in BOTH directions, hooked at the one choke point every packet passes
//! through ([`crate::protocol::client::Client`]'s `write` + `next_packet`).
//!
//! Motivation: the two remaining speedrun walls (`craft`/`click_window` desync and
//! the `water`/`lava` bucket scoop) are *interaction-reliability* failures that only
//! surface under server lag — the bot sends a click / use-item and the authoritative
//! reply is late, reordered, or absent, and a print-only `PKT_DEBUG` scrolls past too
//! fast to reason about. Capturing every packet with a millisecond timestamp into
//! SQLite lets a debug session ask precise questions after the fact:
//!
//! ```sql
//! -- what did the server actually reply after the click, and how late?
//! SELECT ts_ms, dir, name, size, summary FROM packets
//! WHERE ts_ms BETWEEN <click_ts>-500 AND <click_ts>+3000 ORDER BY ts_ms;
//! ```
//!
//! Off by default (zero overhead). Enable with the `SNIFF` env var:
//!   `SNIFF=1`                     capture ALL packets to `data/sniff-<user>.db`
//!   `SNIFF=container,click,use`   capture only packets whose name contains one of
//!                                 these substrings (keeps the craft/scoop signal,
//!                                 drops the movement/chunk noise, stays fast)
//! Modelled on typecraft's global packet sniffer → SQLite capture.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::protocol::PValue;

/// A running capture keyed to one connection. Cheap `record` on the hot path:
/// filter check first (a few `contains`), then one prepared INSERT.
pub struct Sniffer {
    conn: Connection,
    /// Empty = capture everything; else capture only names containing one of these.
    filters: Vec<String>,
}

impl Sniffer {
    /// Build a sniffer from the `SNIFF` env var, or `None` when it is unset/empty.
    /// `username` names the db so parallel bots don't clobber each other's capture.
    pub fn from_env(username: &str) -> Option<Sniffer> {
        let spec = std::env::var("SNIFF").ok().filter(|s| !s.is_empty())?;
        let filters = if spec == "1" || spec.eq_ignore_ascii_case("all") {
            Vec::new()
        } else {
            spec.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
        };
        let path = format!("data/sniff-{username}.db");
        Some(Sniffer::open(Path::new(&path), filters))
    }

    fn open(path: &Path, filters: Vec<String>) -> Sniffer {
        let conn = Connection::open(path).expect("open sniff db");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS packets(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 ts_ms INTEGER NOT NULL,
                 dir TEXT NOT NULL,      -- 'in' (server→bot) or 'out' (bot→server)
                 name TEXT NOT NULL,
                 size INTEGER NOT NULL,
                 summary TEXT
             );
             CREATE INDEX IF NOT EXISTS packets_ts ON packets(ts_ms);
             CREATE INDEX IF NOT EXISTS packets_name ON packets(name);",
        )
        .expect("init sniff schema");
        eprintln!("SNIFF: capturing to {} (filters: {filters:?})", path.display());
        Sniffer { conn, filters }
    }

    /// True if this packet name passes the capture filter.
    fn wanted(&self, name: &str) -> bool {
        self.filters.is_empty() || {
            let lname = name.to_lowercase();
            self.filters.iter().any(|f| lname.contains(f.as_str()))
        }
    }

    /// Record one packet. `dir` is "in" or "out". Captures a compact param summary
    /// only for the interaction packets that matter (windows, items, blocks, acks) —
    /// enough to reconstruct a craft/scoop exchange without storing whole chunks.
    pub fn record(&self, dir: &str, name: &str, size: usize, params: &PValue) {
        if !self.wanted(name) {
            return;
        }
        let ts_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        let summary = summarize(name, params);
        let _ = self.conn.execute(
            "INSERT INTO packets(ts_ms, dir, name, size, summary) VALUES(?1,?2,?3,?4,?5)",
            params![ts_ms, dir, name, size as i64, summary],
        );
    }
}

/// Names worth a full param dump (the craft/scoop/inventory interaction surface).
/// Everything else stores just name+size — plenty to see ordering and timing.
fn is_interaction(name: &str) -> bool {
    const KEYS: [&str; 21] = [
        "container", "set_slot", "set_content", "carried", "click", "use_item",
        "block_change", "block_update", "acknowledge", "ping", "pong", "transaction",
        // combat debugging: the attack packet, arm swing, our position updates, and the server's
        // damage/hurt/knockback responses (so we can see if a hit registers) + entity add/remove
        // (to check the attacked id is the real, live blaze — not a stale/ghost or wrong entity).
        "interact", "swing", "move_player", "player_position", "hurt", "damage", "entity_event",
        "add_entity", "remove_entities",
    ];
    let lname = name.to_lowercase();
    KEYS.iter().any(|k| lname.contains(k))
}

/// Compact, truncated debug of the params for interaction packets; empty otherwise.
fn summarize(name: &str, params: &PValue) -> String {
    if !is_interaction(name) {
        return String::new();
    }
    let mut s = format!("{params:?}");
    const MAX: usize = 600;
    if s.len() > MAX {
        s.truncate(MAX);
        s.push('…');
    }
    s
}
