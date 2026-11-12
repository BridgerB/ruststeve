//! Scripted controls: a test harness until the planner (issue 09). A script starts once the
//! bot's start has settled (its kit or place applied, 20 server ticks after), so a real run
//! and a model run of the same moment press the same keys from the same position.

use physics::Controls;

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
            Script::Still => Controls::default(),
        }
    }

    /// The name a case and a flag use: `square`, `straight:90`, `jump:180`, `still`.
    pub fn name(self) -> String {
        match self {
            Script::WalkSquare => "square".to_owned(),
            Script::Straight { yaw } => format!("straight:{yaw}"),
            Script::JumpForward { yaw } => format!("jump:{yaw}"),
            Script::Still => "still".to_owned(),
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
        ] {
            assert_eq!(Script::parse(&s.name()), Some(s));
        }
        assert_eq!(Script::parse("straight"), None);
    }
}
