//! Persistent world memory + event log, backed by SQLite (like steve's
//! `data/steve.db`). Two tables:
//!
//! - `pois` — the bot's map of points of interest (ores, logs, water, placed
//!   tables/furnaces, descent anchors), keyed by position. The bot queries this
//!   first when it wants to know where something is (e.g. iron ore) instead of
//!   re-wandering, and only explores if the query comes back empty.
//! - `events` — a running diagnostic log (steps, mining, wood, errors) for
//!   replay, the way steve logs everything to SQLite instead of stdout.
//!
//! Entries are annotated, never silently forgotten: an unreachable tree or a
//! needs-a-better-tool ore stays on the map with a `status` and "unlocks" once
//! the bot is ready for it.

use std::path::Path;

use rusqlite::{params, params_from_iter, types::Value, Connection};

pub type Pos = (i32, i32, i32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoiKind {
    Log,
    CoalOre,
    IronOre,
    GoldOre,
    DiamondOre,
    RedstoneOre,
    LapisOre,
    CopperOre,
    Gravel,
    Water,
    Lava,
    CraftingTable,
    Furnace,
    Chest,
    DescentPoint,
    SurfaceBase,
}

impl PoiKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PoiKind::Log => "log",
            PoiKind::CoalOre => "coal_ore",
            PoiKind::IronOre => "iron_ore",
            PoiKind::GoldOre => "gold_ore",
            PoiKind::DiamondOre => "diamond_ore",
            PoiKind::RedstoneOre => "redstone_ore",
            PoiKind::LapisOre => "lapis_ore",
            PoiKind::CopperOre => "copper_ore",
            PoiKind::Gravel => "gravel",
            PoiKind::Water => "water",
            PoiKind::Lava => "lava",
            PoiKind::CraftingTable => "crafting_table",
            PoiKind::Furnace => "furnace",
            PoiKind::Chest => "chest",
            PoiKind::DescentPoint => "descent_point",
            PoiKind::SurfaceBase => "surface_base",
        }
    }

    pub fn from_str(s: &str) -> Option<PoiKind> {
        Some(match s {
            "log" => PoiKind::Log,
            "coal_ore" => PoiKind::CoalOre,
            "iron_ore" => PoiKind::IronOre,
            "gold_ore" => PoiKind::GoldOre,
            "diamond_ore" => PoiKind::DiamondOre,
            "redstone_ore" => PoiKind::RedstoneOre,
            "lapis_ore" => PoiKind::LapisOre,
            "copper_ore" => PoiKind::CopperOre,
            "gravel" => PoiKind::Gravel,
            "water" => PoiKind::Water,
            "lava" => PoiKind::Lava,
            "crafting_table" => PoiKind::CraftingTable,
            "furnace" => PoiKind::Furnace,
            "chest" => PoiKind::Chest,
            "descent_point" => PoiKind::DescentPoint,
            "surface_base" => PoiKind::SurfaceBase,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoiStatus {
    /// Usable right now.
    Available,
    /// Seen, but needs a pickaxe of at least this tier (1 wood … 4 diamond).
    /// "Unlocks" to usable once the bot's tool reaches the tier.
    NeedsTool(i32),
    /// The pathfinder couldn't reach it. Kept on the map, not deleted.
    Unreachable,
    /// Consumed / no longer there.
    Gone,
}

impl PoiStatus {
    fn parts(self) -> (&'static str, i32) {
        match self {
            PoiStatus::Available => ("available", 0),
            PoiStatus::NeedsTool(t) => ("needs_tool", t),
            PoiStatus::Unreachable => ("unreachable", 0),
            PoiStatus::Gone => ("gone", 0),
        }
    }
    fn from_parts(s: &str, tier: i32) -> PoiStatus {
        match s {
            "needs_tool" => PoiStatus::NeedsTool(tier),
            "unreachable" => PoiStatus::Unreachable,
            "gone" => PoiStatus::Gone,
            _ => PoiStatus::Available,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Poi {
    pub kind: PoiKind,
    pub pos: Pos,
    pub status: PoiStatus,
}

pub struct WorldMemory {
    conn: Connection,
    tick: i64,
}

impl WorldMemory {
    /// Open (or create) the memory database at `path`.
    pub fn open(path: &Path) -> Self {
        let conn = Connection::open(path).expect("open memory db");
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS pois(
                 x INTEGER, y INTEGER, z INTEGER,
                 kind TEXT NOT NULL, status TEXT NOT NULL, tier INTEGER NOT NULL DEFAULT 0,
                 seen INTEGER NOT NULL DEFAULT 0,
                 PRIMARY KEY(x, y, z)
             );
             CREATE INDEX IF NOT EXISTS pois_kind ON pois(kind, status);
             CREATE TABLE IF NOT EXISTS events(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 t INTEGER, category TEXT, event TEXT, detail TEXT
             );",
        )
        .expect("init memory schema");
        let tick = conn
            .query_row("SELECT COALESCE(MAX(t), 0) FROM events", [], |r| r.get(0))
            .unwrap_or(0);
        WorldMemory { conn, tick }
    }

    /// Storage key. Large connected volumes (water/lava) snap to a coarse grid so
    /// a whole body is a single entry instead of thousands of blocks.
    fn key(kind: PoiKind, pos: Pos) -> Pos {
        match kind {
            PoiKind::Water | PoiKind::Lava => {
                const Q: i32 = 8;
                (pos.0.div_euclid(Q) * Q, pos.1.div_euclid(Q) * Q, pos.2.div_euclid(Q) * Q)
            }
            _ => pos,
        }
    }

    /// Record (or overwrite) a POI with an explicit status — for things we know
    /// for sure: tables we placed, descent anchors, water bodies, a tree we just
    /// found unreachable.
    pub fn record(&mut self, kind: PoiKind, pos: Pos, status: PoiStatus) {
        let k = Self::key(kind, pos);
        let (s, t) = status.parts();
        self.tick += 1;
        let _ = self.conn.execute(
            "INSERT INTO pois(x,y,z,kind,status,tier,seen) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(x,y,z) DO UPDATE SET kind=excluded.kind, status=excluded.status,
                 tier=excluded.tier, seen=excluded.seen",
            params![k.0, k.1, k.2, kind.as_str(), s, t, self.tick],
        );
    }

    /// Passive sighting (e.g. ore seen through stone while mining). Like `record`,
    /// but does NOT clobber an existing `unreachable`/`gone` status — once we've
    /// learned a spot is unreachable or mined out, re-seeing it doesn't reset that.
    pub fn observe(&mut self, kind: PoiKind, pos: Pos, status: PoiStatus) {
        let k = Self::key(kind, pos);
        let (s, t) = status.parts();
        self.tick += 1;
        let _ = self.conn.execute(
            "INSERT INTO pois(x,y,z,kind,status,tier,seen) VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(x,y,z) DO UPDATE SET
                 kind=excluded.kind, seen=excluded.seen,
                 status=CASE WHEN pois.status IN ('unreachable','gone') THEN pois.status ELSE excluded.status END,
                 tier =CASE WHEN pois.status IN ('unreachable','gone') THEN pois.tier   ELSE excluded.tier   END",
            params![k.0, k.1, k.2, kind.as_str(), s, t, self.tick],
        );
    }

    /// Update the status of an exactly-keyed POI (e.g. mark a mined ore `Gone`).
    pub fn mark(&mut self, pos: Pos, status: PoiStatus) {
        let (s, t) = status.parts();
        let _ = self.conn.execute(
            "UPDATE pois SET status=?1, tier=?2 WHERE x=?3 AND y=?4 AND z=?5",
            params![s, t, pos.0, pos.1, pos.2],
        );
    }

    pub fn is_unreachable(&self, pos: Pos) -> bool {
        self.conn
            .query_row(
                "SELECT status FROM pois WHERE x=?1 AND y=?2 AND z=?3",
                params![pos.0, pos.1, pos.2],
                |r| r.get::<_, String>(0),
            )
            .map(|s| s == "unreachable")
            .unwrap_or(false)
    }

    /// Nearest remembered POI of one of `kinds` that is usable now: `Available`,
    /// or `NeedsTool(t)` with `t <= tier`. Skips `Unreachable`/`Gone`. This is the
    /// "where is the iron ore?" query — pass the bot's current pickaxe tier.
    pub fn nearest(&self, kinds: &[PoiKind], from: Pos, tier: i32) -> Option<Poi> {
        if kinds.is_empty() {
            return None;
        }
        let placeholders = kinds.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT x,y,z,kind,status,tier FROM pois
             WHERE kind IN ({placeholders})
               AND (status='available' OR (status='needs_tool' AND tier<=?))
             ORDER BY ((x-?)*(x-?)+(y-?)*(y-?)+(z-?)*(z-?)) ASC LIMIT 1"
        );
        let mut vals: Vec<Value> = kinds.iter().map(|k| Value::Text(k.as_str().to_string())).collect();
        vals.push(Value::Integer(tier as i64));
        for v in [from.0, from.0, from.1, from.1, from.2, from.2] {
            vals.push(Value::Integer(v as i64));
        }
        self.conn
            .query_row(&sql, params_from_iter(vals), |r| {
                let x: i32 = r.get(0)?;
                let y: i32 = r.get(1)?;
                let z: i32 = r.get(2)?;
                let kind_s: String = r.get(3)?;
                let status_s: String = r.get(4)?;
                let t: i32 = r.get(5)?;
                Ok(Poi {
                    kind: PoiKind::from_str(&kind_s).unwrap_or(PoiKind::Log),
                    pos: (x, y, z),
                    status: PoiStatus::from_parts(&status_s, t),
                })
            })
            .ok()
    }

    /// Append a diagnostic event (category/event/detail), the way steve logs to
    /// SQLite. `t` is a monotonic counter.
    pub fn log(&mut self, category: &str, event: &str, detail: &str) {
        self.tick += 1;
        let _ = self.conn.execute(
            "INSERT INTO events(t,category,event,detail) VALUES(?1,?2,?3,?4)",
            params![self.tick, category, event, detail],
        );
    }

    pub fn count(&self, kind: PoiKind) -> i64 {
        self.conn
            .query_row("SELECT COUNT(*) FROM pois WHERE kind=?1", params![kind.as_str()], |r| r.get(0))
            .unwrap_or(0)
    }

    pub fn len(&self) -> i64 {
        self.conn.query_row("SELECT COUNT(*) FROM pois", [], |r| r.get(0)).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
