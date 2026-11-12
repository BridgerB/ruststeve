//! Scripted controls: a test harness until the planner (issue 09). A script starts once the
//! bot's start has settled (its kit or place applied, 20 server ticks after), so a real run
//! and a model run of the same moment press the same keys from the same position. Since issue
//! 10b a script also acts (digs, clicks), for the parity table's dig and craft rows.

use physics::Controls;
use world::{Act, DigStep, World};

/// What a craft script makes in window 0, from what its moment gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Craft {
    /// One log (given into hotbar slot 36) into the grid; shift-click the planks out.
    Planks,
    /// Two planks, one each into grid slots 1 and 3 (a column); shift-click the sticks out.
    Sticks,
    /// Four planks, one into each grid slot; shift-click the table out.
    Table,
}

impl Craft {
    /// The clicks, in order: (slot, button, mode). Each changes the window, so each is
    /// answered (a click that changes nothing draws no reply).
    fn clicks(self) -> &'static [(i16, i8, i32)] {
        match self {
            Craft::Planks => &[(36, 0, 0), (1, 0, 0), (0, 0, 1)],
            Craft::Sticks => &[(36, 0, 0), (1, 1, 0), (3, 1, 0), (0, 0, 1)],
            Craft::Table => &[
                (36, 0, 0),
                (1, 1, 0),
                (2, 1, 0),
                (3, 1, 0),
                (4, 1, 0),
                (0, 0, 1),
            ],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Craft::Planks => "planks",
            Craft::Sticks => "sticks",
            Craft::Table => "table",
        }
    }
}

/// What a scripted bot presses.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Script {
    /// Issue 06's staircase: 40 ticks south then 40 west, over and over, jumping for the first
    /// 6 ticks of each west leg.
    WalkSquare,
    /// Forward along one yaw, never jumping.
    Straight { yaw: f64 },
    /// Forward along one yaw, holding jump: ledges and steps are climbed, water swum.
    JumpForward { yaw: f64 },
    /// Nothing pressed.
    Still,
    /// Standing still, dig the block under the feet: START at tick 0, FINISH at `finish`.
    DigDown { finish: u64 },
    /// Standing still, craft in window 0: one click per round trip (the next only once the
    /// last moved the window's state id).
    Craft(Craft),
}

impl Script {
    pub fn controls(self, tick: u64) -> Controls {
        match self {
            Script::WalkSquare => {
                let side = (tick / 40) % 4;
                let into_side = tick % 40;
                Controls {
                    forward: true,
                    jump: side % 2 == 1 && into_side < 6,
                    sneak: false,
                    yaw: [0.0, 90.0, 0.0, 90.0][side as usize],
                }
            }
            Script::Straight { yaw } => Controls {
                forward: true,
                jump: false,
                sneak: false,
                yaw,
            },
            Script::JumpForward { yaw } => Controls {
                forward: true,
                jump: true,
                sneak: false,
                yaw,
            },
            Script::Still | Script::DigDown { .. } | Script::Craft(_) => Controls::default(),
        }
    }

    /// What the script does to the world this tick, read from what the world holds.
    pub fn acts(self, tick: u64, w: &World) -> Vec<Act> {
        match self {
            Script::DigDown { finish } => {
                let Some(body) = w.body else {
                    return Vec::new();
                };
                // The block under the feet (y a hair under the top of the block stood on).
                let pos = world::BlockPos::new(
                    body.pos[0].floor() as i32,
                    (body.pos[1] - 0.01).floor() as i32,
                    body.pos[2].floor() as i32,
                );
                let step = match tick {
                    0 => DigStep::Start,
                    t if t == finish => DigStep::Finish,
                    _ => return Vec::new(),
                };
                vec![Act::Dig { step, pos, face: 1 }]
            }
            Script::Craft(c) => {
                let sent = usize::try_from(w.clicks_sent).unwrap_or(usize::MAX);
                match (w.click.is_none(), c.clicks().get(sent)) {
                    (true, Some(&(slot, button, mode))) => vec![Act::Click { slot, button, mode }],
                    (_, _) => Vec::new(),
                }
            }
            Script::WalkSquare
            | Script::Straight { .. }
            | Script::JumpForward { .. }
            | Script::Still => Vec::new(),
        }
    }

    /// The name a case and a flag use: `square`, `straight:90`, `jump:180`, `still`.
    pub fn name(self) -> String {
        match self {
            Script::WalkSquare => "square".to_owned(),
            Script::Straight { yaw } => format!("straight:{yaw}"),
            Script::JumpForward { yaw } => format!("jump:{yaw}"),
            Script::Still => "still".to_owned(),
            Script::DigDown { finish } => format!("dig:{finish}"),
            Script::Craft(c) => format!("craft:{}", c.name()),
        }
    }

    pub fn parse(s: &str) -> Option<Script> {
        let (head, yaw) = match s.split_once(':') {
            Some((h, y)) => (h, y.parse::<f64>().ok()),
            None => (s, None),
        };
        match (head, yaw) {
            ("square", None) => Some(Script::WalkSquare),
            ("still", None) => Some(Script::Still),
            ("straight", Some(yaw)) => Some(Script::Straight { yaw }),
            ("jump", Some(yaw)) => Some(Script::JumpForward { yaw }),
            ("dig", Some(f)) if f >= 0.0 && f.fract() == 0.0 => {
                Some(Script::DigDown { finish: f as u64 })
            }
            _ => Self::parse_craft(s),
        }
    }

    fn parse_craft(s: &str) -> Option<Script> {
        match s {
            "craft:planks" => Some(Script::Craft(Craft::Planks)),
            "craft:sticks" => Some(Script::Craft(Craft::Sticks)),
            "craft:table" => Some(Script::Craft(Craft::Table)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_back() {
        for s in [
            Script::WalkSquare,
            Script::Still,
            Script::Straight { yaw: 90.0 },
            Script::JumpForward { yaw: -45.0 },
            Script::DigDown { finish: 30 },
            Script::Craft(Craft::Planks),
            Script::Craft(Craft::Sticks),
            Script::Craft(Craft::Table),
        ] {
            assert_eq!(Script::parse(&s.name()), Some(s));
        }
        assert_eq!(Script::parse("straight"), None);
    }
}
