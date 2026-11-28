//! The lane seed (generality item A, brain/ruststeve/design/determinism-rules.md): lanes resumed
//! from one snapshot played the same game to tick 18,700 (docs/problems/lava-portal.md wall 3 on
//! issue-83-lava-portal), because nothing in the planner read the tick's seed. The planner reads
//! it here, and only to break ties among choices it holds equal: never for a threshold, a count
//! or a safety rule. Every use is a [`Use`], drawn through [`pick`], so each is named and tested.
//!
//! The seed arrives on the tick (`world.seed`): the shell's xorshift, started from `Opts.seed`
//! (`derive` of the race's run, lane and bot) or, with none, from the clock. The planner holds the
//! first non-zero one it sees for the whole run ([`Draw::from_tick`]), so a tie broken once stays
//! broken and no choice flips from tick to tick.

/// One use of the seed: a closed set, so every tie the seed breaks is listed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Use {
    /// The explore heading's first quarter. Every bot began at 0 (+z), so every lane of a
    /// snapshot explored, tunnelled and cut stairs the same way; no quarter is known better.
    StartQuarter,
    /// The side the heading turns on a failure: a quarter one way or the other. Both reach all
    /// four quarters in four turns; neither is known better.
    TurnSide,
}

impl Use {
    /// The use's salt: fixed per use, so two uses of one seed draw apart. Grow by accretion:
    /// never renumber one, or a recorded run's tie-breaks change on replay.
    const fn salt(self) -> u64 {
        match self {
            Self::StartQuarter => 1,
            Self::TurnSide => 2,
        }
    }
}

/// splitmix64's finaliser (Steele, Lea and Flood 2014; the constants are the published ones):
/// every input bit reaches every output bit, so nearby seeds, lanes and salts draw apart.
pub const fn mix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A bot's seed from its race run id, lane and index in the lane: the same three give the same
/// seed on any machine (a rerun of a race replays its tie-breaks), any one changed gives another.
/// Never 0, which the planner reads as "no seed". The run id is folded with FNV-1a (64-bit; its
/// published offset basis and prime).
pub fn derive(run: &str, lane: u32, bot: u32) -> u64 {
    let h = run.bytes().fold(0xCBF2_9CE4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01B3)
    });
    let s = mix(mix(mix(h) ^ u64::from(lane)) ^ u64::from(bot));
    s | 1
}

/// One draw in `0..n` for `use_` from `seed`, with `key` naming which tie of that use it breaks
/// (0 for a use made once a run). Pure: the same four give the same answer. `n` 0 gives 0.
pub fn pick(seed: u64, use_: Use, key: u64, n: usize) -> usize {
    let Ok(n64) = u64::try_from(n) else {
        return 0;
    };
    if n64 == 0 {
        return 0;
    }
    let r = mix(mix(seed ^ use_.salt()) ^ key);
    usize::try_from(r % n64).unwrap_or(0)
}

/// The run's held seed and the ties it broke once a run: what the planner keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Draw {
    /// The first non-zero tick seed seen; `None` until one comes (a hand-built world has 0, and
    /// the planner then keeps every default).
    pub seed: Option<u64>,
}

/// The start the seed gives: the explore heading's first quarter and the turn's side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Start {
    /// Degrees, one of 0, 90, 180, 270 (the planner's heading convention: 0 is +z).
    pub heading: f64,
    /// Degrees added on each turn: 90 (the old default) or 270 (a quarter the other way).
    pub turn: f64,
}

/// The defaults every bot had before the seed: heading 0, turns of +90.
pub const DEFAULT_START: Start = Start {
    heading: 0.0,
    turn: 90.0,
};

/// The quarters a heading starts on and the two sides of a turn, in the planner's degrees.
const QUARTERS: [f64; 4] = [0.0, 90.0, 180.0, 270.0];
const TURNS: [f64; 2] = [90.0, 270.0];

/// The start for `seed`.
pub fn start(seed: u64) -> Start {
    Start {
        heading: QUARTERS
            .get(pick(seed, Use::StartQuarter, 0, QUARTERS.len()))
            .copied()
            .unwrap_or(0.0),
        turn: TURNS
            .get(pick(seed, Use::TurnSide, 0, TURNS.len()))
            .copied()
            .unwrap_or(90.0),
    }
}

impl Draw {
    /// The tick's seed taken, the first time a non-zero one comes: `Some(start)` that once, else
    /// `None`.
    pub fn from_tick(&mut self, tick_seed: u64) -> Option<Start> {
        if self.seed.is_some() || tick_seed == 0 {
            return None;
        }
        self.seed = Some(tick_seed);
        Some(start(tick_seed))
    }

    /// The quarter turn for this run: +90 with no seed (the old default), else the seed's side.
    pub fn turn(&self) -> f64 {
        self.seed.map_or(DEFAULT_START.turn, |s| start(s).turn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_fixed_and_apart() {
        assert_eq!(derive("46127598383", 0, 0), derive("46127598383", 0, 0));
        assert_ne!(derive("46127598383", 0, 0), derive("46127598383", 1, 0));
        assert_ne!(derive("46127598383", 0, 0), derive("46127598383", 0, 1));
        assert_ne!(derive("46127598383", 0, 0), derive("46127598384", 0, 0));
        assert_ne!(derive("", 0, 0), 0);
    }

    #[test]
    fn pick_stays_in_range() {
        for s in 1..200 {
            assert!(pick(s, Use::StartQuarter, 0, 4) < 4);
            assert_eq!(pick(s, Use::TurnSide, 7, 0), 0);
        }
    }
}
