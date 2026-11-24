//! The day clock (issue 16, `docs/problems/day-clock.md`): pure helpers over the day time, the
//! overworld clock's ticks taken mod `DAY_TICKS`. A player plans around dusk; the planner asks
//! these with `World::day_time`.
//!
//! Every number is the 26.1.2 jar's own, from `data/minecraft/timeline/day.json` (the overworld
//! clock's timeline): `period_ticks` 24000, and the `gameplay/monsters_burn` track, which turns
//! false at 12542 and true again at 23460. Between those ticks the sky is too dark for undead to
//! burn and the sky light falls (`gameplay/sky_light_level`: 1.0 to 11867, 0.27 from 13670 to
//! 22330), so surface monsters spawn; that span is the night here. The time markers in the same
//! file name 13000 `night` and 1000 `day`.

/// Ticks in one day (`period_ticks`).
pub const DAY_TICKS: i64 = 24_000;

/// The first tick of the night: undead stop burning (`monsters_burn` false at 12542).
pub const DUSK: i64 = 12_542;

/// The first tick of the day again: undead burn (`monsters_burn` true at 23460). Creepers,
/// spiders and witches do not burn and stay after it.
pub const DAWN: i64 = 23_460;

/// `t` folded into one day, 0 to 23,999 (a negative clock folds the same way).
pub fn of_day(t: i64) -> i64 {
    t.rem_euclid(DAY_TICKS)
}

/// Night: from `DUSK` up to `DAWN`, when undead do not burn and surface monsters spawn.
pub fn is_night(day_time: i64) -> bool {
    (DUSK..DAWN).contains(&of_day(day_time))
}

/// Ticks until the next dusk; 0 at night.
pub fn ticks_to_dusk(day_time: i64) -> i64 {
    if is_night(day_time) {
        return 0;
    }
    (DUSK - of_day(day_time)).rem_euclid(DAY_TICKS)
}

/// Ticks until the night ends; 0 by day.
pub fn ticks_to_dawn(day_time: i64) -> i64 {
    if !is_night(day_time) {
        return 0;
    }
    DAWN - of_day(day_time)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_spans_dusk_to_dawn() {
        assert!(!is_night(0));
        assert!(!is_night(6_000));
        assert!(!is_night(DUSK - 1));
        assert!(is_night(DUSK));
        assert!(is_night(18_000));
        assert!(is_night(DAWN - 1));
        assert!(!is_night(DAWN));
        assert!(!is_night(23_999));
        // The clock counts days on: day 3's midnight is night.
        assert!(is_night(3 * DAY_TICKS + 18_000));
        assert!(!is_night(-1));
    }

    #[test]
    fn ticks_to_dusk_counts_to_the_next_night() {
        assert_eq!(ticks_to_dusk(0), DUSK);
        assert_eq!(ticks_to_dusk(12_000), 542);
        assert_eq!(ticks_to_dusk(DUSK - 1), 1);
        assert_eq!(ticks_to_dusk(DUSK), 0);
        assert_eq!(ticks_to_dusk(20_000), 0);
        // After dawn the next dusk is tomorrow's.
        assert_eq!(ticks_to_dusk(DAWN), DAY_TICKS - DAWN + DUSK);
        assert_eq!(ticks_to_dusk(DAY_TICKS + 1_000), DUSK - 1_000);
    }

    #[test]
    fn ticks_to_dawn_counts_to_the_end_of_night() {
        assert_eq!(ticks_to_dawn(6_000), 0);
        assert_eq!(ticks_to_dawn(DUSK), DAWN - DUSK);
        assert_eq!(ticks_to_dawn(18_000), 5_460);
        assert_eq!(ticks_to_dawn(DAWN - 1), 1);
        assert_eq!(ticks_to_dawn(DAWN), 0);
    }
}
