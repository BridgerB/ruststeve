//! A race's own diagnosis from its case files (brain/ruststeve/design/race-and-results.md): the
//! aim met and the deaths by place, the planner's note kinds ranked (how often each path fired,
//! in how many runs), and the cast's failures by op with the refill's last step. Until
//! 2026-11-17 these were tallied by hand from exported recordings after every race.

use std::collections::BTreeMap;

use crate::{Case, Ending};

/// Note kinds shown, most runs first (the rest are counted in a last line).
const KINDS_SHOWN: usize = 60;

/// The bot's number in its lane, from its name (`rs-l03-b07` is 7): its place in a start that
/// lists places.
pub fn place_of(bot: &str) -> Option<u32> {
    bot.rsplit_once("-b")?.1.parse().ok()
}

/// A word with no number or bracket in it: part of a note's kind, not of its particulars.
fn plain(word: &str) -> bool {
    !word
        .chars()
        .any(|c| c.is_ascii_digit() || c == '(' || c == '[')
}

/// A note line's text after its tick.
fn after_tick(note: &str) -> &str {
    note.split_once(' ').map_or(note, |(_, rest)| rest)
}

/// A failed cast, read from its note.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CastFailure {
    /// The failure's name (`NoRefill`, `Op`, `Stalled`, `Exposed`).
    pub failure: String,
    /// The op it failed at, kind and step (`:pour :lava`).
    pub op: String,
    /// The refill's last step with its particulars left out (`scoop failed NoSight`), or
    /// `no refill`.
    pub refill: String,
}

/// The failed cast a note tells of, if it tells of one.
pub fn cast_failure(note: &str) -> Option<CastFailure> {
    let rest = note.split_once("cast failed: ")?.1;
    let failure = rest.split_whitespace().next()?.to_owned();
    let op = rest
        .split_once(", at ")
        .map_or_else(String::new, |(_, at)| {
            at.split_whitespace().take(2).collect::<Vec<_>>().join(" ")
        });
    let refill = rest.split_once("refill: ").map_or_else(
        || "no refill".to_owned(),
        |(_, trail)| {
            // The note's why follows in the line's last bracket.
            let trail = trail.rsplit_once(" (").map_or(trail, |(t, _)| t);
            let last = trail.rsplit(" / ").next().unwrap_or(trail);
            last.split_whitespace()
                .filter(|w| plain(w))
                .collect::<Vec<_>>()
                .join(" ")
        },
    );
    Some(CastFailure {
        failure,
        op,
        refill,
    })
}

/// A note's kind: its leading words up to the first with a number or a bracket, so notes that
/// differ only in a place or a count are one kind. A failed cast is its failure and its op.
pub fn note_kind(note: &str) -> String {
    if let Some(f) = cast_failure(note) {
        return format!("cast failed: {} at {}", f.failure, f.op);
    }
    let text = after_tick(note);
    let kind: Vec<&str> = text.split_whitespace().take_while(|w| plain(w)).collect();
    if kind.is_empty() {
        text.split_whitespace().next().unwrap_or("").to_owned()
    } else {
        kind.join(" ")
    }
}

#[derive(Default)]
struct Place {
    n: u64,
    met: u64,
    deaths: BTreeMap<String, u64>,
}

/// The diagnosis as markdown, for a job summary and a release.
pub fn diagnose(cases: &[Case]) -> String {
    let counted: Vec<&Case> = cases.iter().filter(|c| c.ending.counted()).collect();
    let mut places: BTreeMap<u32, Place> = BTreeMap::new();
    // kind -> (runs with it, times in all)
    let mut kinds: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut failures: BTreeMap<CastFailure, u64> = BTreeMap::new();
    let mut with_notes = 0u64;
    // Runs whose case counts every note of the ledger (`story::note_kinds`); the others' kinds
    // come from the case's kept notes, which leave repeats and the middle out.
    let mut whole = 0u64;
    for c in &counted {
        if let Some(p) = place_of(&c.bot) {
            let place = places.entry(p).or_default();
            place.n += 1;
            place.met += u64::from(c.plan.as_ref().is_some_and(|p| p.reached));
            if let Ending::Death { killer } = &c.ending {
                *place.deaths.entry(killer.clone()).or_default() += 1;
            }
        }
        let mut mine: BTreeMap<String, u64> = BTreeMap::new();
        let counted_whole = c.story.as_ref().filter(|s| !s.note_kinds.is_empty());
        if let Some(s) = counted_whole {
            whole += 1;
            for k in &s.note_kinds {
                *mine.entry(k.kind.clone()).or_default() += k.n;
            }
        }
        if let Some(plan) = &c.plan {
            with_notes += u64::from(!plan.notes.is_empty());
            for note in &plan.notes {
                if counted_whole.is_none() {
                    *mine.entry(note_kind(note)).or_default() += 1;
                }
                if let Some(f) = cast_failure(note) {
                    *failures.entry(f).or_default() += 1;
                }
            }
        }
        for (kind, times) in mine {
            let e = kinds.entry(kind).or_default();
            e.0 += 1;
            e.1 += times;
        }
    }

    let mut lines = vec![
        String::new(),
        format!(
            "### diagnosis ({} counted runs, {} with notes)",
            counted.len(),
            with_notes
        ),
        String::new(),
        "| place | aim met | deaths | killers |".to_owned(),
        "|---|---|---|---|".to_owned(),
    ];
    for (p, place) in &places {
        let dead: u64 = place.deaths.values().sum();
        let killers = place
            .deaths
            .iter()
            .map(|(k, n)| format!("{k} {n}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "| {p} | {}/{} | {dead} | {killers} |",
            place.met, place.n
        ));
    }

    let mut ranked: Vec<_> = failures.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    lines.push(String::new());
    lines.push("| casts failed | failure | at op | the refill's last step |".to_owned());
    lines.push("|---|---|---|---|".to_owned());
    for (f, n) in ranked {
        lines.push(format!("| {n} | {} | {} | {} |", f.failure, f.op, f.refill));
    }

    let mut ranked: Vec<_> = kinds.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    lines.push(String::new());
    lines.push(format!(
        "### notes by kind (race totals; every note of the ledger for {whole} of {} runs: a fix's firings are counted here)",
        counted.len()
    ));
    lines.push(String::new());
    lines.push("| runs | times | note kind |".to_owned());
    lines.push("|---|---|---|".to_owned());
    let more = ranked.len().saturating_sub(KINDS_SHOWN);
    for (kind, (runs, times)) in ranked.into_iter().take(KINDS_SHOWN) {
        lines.push(format!("| {runs} | {times} | {kind} |"));
    }
    if more > 0 {
        lines.push(format!(
            "({more} more kinds, each in fewer runs: see the case files)"
        ));
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::{CastFailure, cast_failure, note_kind, place_of};

    /// A note of race 44041169156 (rs-l00-b00), as a case file holds it.
    const NO_SIGHT: &str = "3920 cast failed: NoRefill after 76 ops, at :pour :lava - [-77 -54 124] :lava_bucket [-76.500000 -53.700000 121.980000] [-77 -54 122] [2 0] - [:or [:block [-77 -54 122] :obsidian] [:lava-source [-77 -54 122]]]; no regressions; refill: 3911 scoop (-68, -55, 129) routed / 3916 scoop (-68, -55, 129) failed NoSight / 3917 19 sources, no station / 3919 scoop (-68, -55, 129) routed / 3920 scoop (-68, -55, 129) failed NoSight (site given up; none within 16 of it)";

    #[test]
    fn a_place_is_the_bots_number_in_its_lane() {
        assert_eq!(place_of("rs-l03-b07"), Some(7));
        assert_eq!(place_of("rs-l00-b13"), Some(13));
        assert_eq!(place_of("steve"), None);
    }

    #[test]
    fn a_failed_cast_is_read_from_its_note() {
        assert_eq!(
            cast_failure(NO_SIGHT),
            Some(CastFailure {
                failure: "NoRefill".to_owned(),
                op: ":pour :lava".to_owned(),
                refill: "scoop failed NoSight".to_owned(),
            })
        );
        let no_refill = "824 cast failed: Op after 26 ops, at :dig :room [95 62 79] [95 62 79] - - - - - [:or [:block [95 62 79] :obsidian] [:not-solid [95 62 79]]]; no regressions; no refill (site given up; none within 16 of it)";
        assert_eq!(
            cast_failure(no_refill),
            Some(CastFailure {
                failure: "Op".to_owned(),
                op: ":dig :room".to_owned(),
                refill: "no refill".to_owned(),
            })
        );
        assert_eq!(
            cast_failure("57 cast at (-1, 64, -8) facing south (a site)"),
            None
        );
    }

    #[test]
    fn notes_that_differ_in_a_place_or_a_count_are_one_kind() {
        assert_eq!(
            note_kind(
                "57 cast at (-1, 64, -8) facing south (lava (1, 63, 3)) (a site beside lava)"
            ),
            "cast at"
        );
        assert_eq!(note_kind("2880 portal lit (118 ops done)"), "portal lit");
        assert_eq!(
            note_kind("436 cut 6 stairs (look again for what to mine)"),
            "cut"
        );
        assert_eq!(
            note_kind("84 dig toward the landing (101, 62, 69) (site at (99, 62, 67), no walk)"),
            "dig toward the landing"
        );
        assert_eq!(
            note_kind("3941 relocate (1 of 6) (12 tasks failed)"),
            "relocate"
        );
        assert_eq!(note_kind(NO_SIGHT), "cast failed: NoRefill at :pour :lava");
    }
}
