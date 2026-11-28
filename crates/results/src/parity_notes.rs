//! A parity row's path, not only its end (issue 77, brain/ruststeve/design/chests.md): a
//! moment's `notes` are ledger substrings, each with the count it must fire, checked against the
//! real bot's planner notes and the model's. A count that differs on either side is a miss, and a
//! miss fails the table as a differing field would.

use crate::Source;
use crate::parity::{Moment, MomentResult};

/// How many of `notes` contain `needle`.
fn fired(notes: &[String], needle: &str) -> usize {
    notes.iter().filter(|n| n.contains(needle)).count()
}

/// Every miss, as a line for the report: the row, the side, the note, the count asked and the
/// count fired, then each note that matched (so a substring two notes share is seen as such). A
/// row with `notes` and no result on a side is a miss for each of its notes.
pub fn check(moments: &[Moment], results: &[MomentResult]) -> Vec<String> {
    let mut misses = Vec::new();
    for m in moments.iter().filter(|m| !m.notes.is_empty()) {
        for side in [Source::Real, Source::Sim] {
            let run = results
                .iter()
                .find(|r| r.moment == m.id && r.source == side);
            for (needle, want) in &m.notes {
                let got = run.map(|r| fired(&r.notes, needle));
                let want = usize::try_from(*want).unwrap_or(usize::MAX);
                if got != Some(want) {
                    let mut line = format!(
                        "{} {side:?}: \"{needle}\" fired {}, want {want}",
                        m.id,
                        got.map_or_else(|| "no result".to_owned(), |n| n.to_string())
                    );
                    for n in run
                        .into_iter()
                        .flat_map(|r| &r.notes)
                        .filter(|n| n.contains(needle))
                    {
                        line.push_str(&format!("\n  - matched: {n}"));
                    }
                    misses.push(line);
                }
            }
        }
    }
    misses
}

/// The misses under the table, or the line saying every row's notes fired as asked.
pub fn render(misses: &[String], rows_with_notes: usize) -> String {
    if misses.is_empty() {
        return format!("\nnotes: every note of {rows_with_notes} rows fired as asked\n");
    }
    let mut out = format!("\nnotes: {} misses: FAIL\n", misses.len());
    for m in misses {
        out.push_str(&format!("- {m}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment(notes: &[(&str, u32)]) -> Moment {
        let text = format!(
            "id = \"k01-task\"\nlanding = 0\ndx = 0\ndz = 0\nscript = \"plan:chest-trip\"\nticks = 10\nnotes = [{}]\n",
            notes
                .iter()
                .map(|(s, n)| format!("[\"{s}\", {n}]"))
                .collect::<Vec<_>>()
                .join(", ")
        );
        toml::from_str(&text).unwrap()
    }

    fn result(side: Source, notes: &[&str]) -> MomentResult {
        MomentResult {
            moment: "k01-task".to_owned(),
            source: side,
            commit: String::new(),
            outcome: None,
            corrections: 0,
            ending: "cap".to_owned(),
            notes: notes.iter().map(|s| (*s).to_owned()).collect(),
            final_pos: None,
        }
    }

    const TRIP: [&str; 4] = [
        "chest trip: place the chest",
        "chest closed at (21, 80, 20): stash done, 74 of 74 cobblestone",
        "chest closed at (21, 80, 20): fetch done, 74 of 74 cobblestone",
        "chest trip met (1 stashes and 1 fetches ended)",
    ];

    fn asked() -> Moment {
        moment(&[("stash done", 1), ("fetch done", 1), ("chest trip met", 1)])
    }

    #[test]
    fn every_note_fired_once_on_both_sides_passes() {
        let rs = [result(Source::Real, &TRIP), result(Source::Sim, &TRIP)];
        assert!(check(&[asked()], &rs).is_empty());
    }

    #[test]
    fn a_missing_note_fails_the_row() {
        let short = &TRIP[..3];
        let rs = [result(Source::Real, short), result(Source::Sim, &TRIP)];
        let misses = check(&[asked()], &rs);
        assert_eq!(misses.len(), 1, "{misses:?}");
        assert!(misses[0].contains("Real") && misses[0].contains("chest trip met"));
        assert!(misses[0].contains("fired 0, want 1"));
    }

    #[test]
    fn a_note_fired_twice_where_one_is_asked_fails() {
        let mut twice = TRIP.to_vec();
        twice.push("chest closed at (21, 80, 20): stash done, 0 of 74 cobblestone");
        let rs = [result(Source::Real, &TRIP), result(Source::Sim, &twice)];
        let misses = check(&[asked()], &rs);
        assert_eq!(misses.len(), 1, "{misses:?}");
        assert!(misses[0].contains("Sim") && misses[0].contains("fired 2, want 1"));
    }

    /// Parity 46130761031: "stash done" was in two notes of one stash, the window's close and
    /// the planner's end; the miss shows both, and a substring only one carries counts one.
    #[test]
    fn a_substring_two_notes_share_counts_both_and_shows_them() {
        let notes = [
            "chest closed at (21, 80, 20): stash done, 74 of 74 cobblestone",
            "stash done: 77 items at (21, 80, 20)",
        ];
        let rs = [result(Source::Real, &notes), result(Source::Sim, &notes)];
        let shared = check(&[moment(&[("stash done", 1)])], &rs);
        assert_eq!(shared.len(), 2, "{shared:?}");
        assert!(shared[0].contains("fired 2, want 1"));
        assert!(shared[0].contains("matched: chest closed at"));
        assert!(shared[0].contains("matched: stash done: 77 items"));
        let unique = check(
            &[moment(&[("stash done:", 1), ("chest closed at", 1)])],
            &rs,
        );
        assert!(unique.is_empty(), "{unique:?}");
    }

    #[test]
    fn a_side_with_no_result_misses_every_note() {
        let rs = [result(Source::Real, &TRIP)];
        assert_eq!(check(&[asked()], &rs).len(), 3);
        // A row asking no notes is never checked.
        assert!(check(&[moment(&[])], &[]).is_empty());
    }
}
