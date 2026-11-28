//! The keep rule's count per run (issue 77, brain/ruststeve/design/keep-rule.md): the drops,
//! the wants, and `dropped_then_wanted`, the wants of a kind the run had dropped earlier. The
//! issue's bar is that last count at 0. It is read from the ledger's notes: the drop lines
//! (`drop <Kind> from slot <n>`, `drop <Kind> from the cursor`) and the want lines
//! (`want <Kind>|<Kind> <n> for <task>`) `plan::keep` and `Plan::equip` write. The summary row
//! comes after issue 56 (lead-77).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::Ledger;

/// A run's keep counts.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepCounts {
    /// Drop lines.
    pub drops: u64,
    /// Want lines.
    pub wants: u64,
    /// Want lines naming a kind dropped at an earlier tick of the run (to be 0).
    pub dropped_then_wanted: u64,
    /// Those wants, as `<tick> <what>`, in order.
    pub cases: Vec<String>,
}

/// The kind a drop line names: "drop Cobblestone from slot 21" is `Cobblestone`, and so are
/// the forest base's "base: drop Cobblestone from slot 21" and "base: a main slot freed: drop
/// Cobblestone from slot 21".
pub fn drop_kind(what: &str) -> Option<&str> {
    let what = what.strip_prefix("base: ").unwrap_or(what);
    let what = what.strip_prefix("a main slot freed: ").unwrap_or(what);
    let mut words = what.strip_prefix("drop ")?.split_whitespace();
    let kind = words.next()?;
    (words.next() == Some("from")).then_some(kind)
}

/// The kinds a want line names: "want Cobblestone|Blackstone 3 for craft stone_pickaxe" is
/// both. "fuel" names no item and matches no drop.
pub fn want_kinds(what: &str) -> Option<Vec<&str>> {
    let mut words = what.strip_prefix("want ")?.split_whitespace();
    let kinds = words.next()?;
    words.next()?.parse::<i64>().ok()?;
    Some(kinds.split('|').collect())
}

/// The keep counts of one run's ledger.
pub fn keep_counts(l: &Ledger) -> KeepCounts {
    let mut first_drop: BTreeMap<String, u64> = BTreeMap::new();
    let mut c = KeepCounts::default();
    for t in &l.tasks {
        let plan::TaskRecord::Note { at, what, .. } = t else {
            continue;
        };
        if let Some(k) = drop_kind(what) {
            c.drops += 1;
            first_drop.entry(k.to_owned()).or_insert(*at);
        } else if let Some(kinds) = want_kinds(what) {
            c.wants += 1;
            if kinds
                .iter()
                .any(|k| first_drop.get(*k).is_some_and(|d| d < at))
            {
                c.dropped_then_wanted += 1;
                c.cases.push(format!("{at} {what}"));
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(at: u64, what: &str) -> plan::TaskRecord {
        plan::TaskRecord::Note {
            at,
            what: what.to_owned(),
            why: String::new(),
        }
    }

    fn ledger(tasks: Vec<plan::TaskRecord>) -> Ledger {
        Ledger {
            format_version: 0,
            run: "r".to_owned(),
            bot: "b".to_owned(),
            commit: "c".to_owned(),
            tasks,
            dropped: 0,
            facts: Vec::new(),
        }
    }

    #[test]
    fn drop_and_want_lines_are_read() {
        assert_eq!(
            drop_kind("drop Cobblestone from slot 21"),
            Some("Cobblestone")
        );
        assert_eq!(drop_kind("drop String from the cursor"), Some("String"));
        assert_eq!(drop_kind("drop the plan"), None);
        assert_eq!(drop_kind("base: drop Dirt from slot 22"), Some("Dirt"));
        assert_eq!(
            drop_kind("base: a main slot freed: drop Netherrack from slot 9"),
            Some("Netherrack")
        );
        assert_eq!(
            want_kinds("want OakPlanks|SprucePlanks 4 for goal kit"),
            Some(vec!["OakPlanks", "SprucePlanks"])
        );
        assert_eq!(
            want_kinds("want Cobblestone|Blackstone 3 for craft stone_pickaxe"),
            Some(vec!["Cobblestone", "Blackstone"])
        );
        assert_eq!(
            want_kinds("want fuel 1 for smelt IronIngot"),
            Some(vec!["fuel"])
        );
        assert_eq!(want_kinds("want to go home"), None);
    }

    /// A want of a kind dropped earlier counts; one of a kind never dropped, or dropped only
    /// later, does not.
    #[test]
    fn a_want_after_its_kinds_drop_counts() {
        let l = ledger(vec![
            note(10, "want String 1 for craft bow"),
            note(20, "drop String from slot 12"),
            note(30, "drop Cobblestone from the cursor"),
            note(40, "want Cobblestone|Blackstone 3 for craft stone_pickaxe"),
            note(50, "want Stick 2 for craft stone_pickaxe"),
            note(60, "want fuel 1 for smelt IronIngot"),
        ]);
        let c = keep_counts(&l);
        assert_eq!((c.drops, c.wants, c.dropped_then_wanted), (2, 4, 1));
        assert_eq!(
            c.cases,
            vec!["40 want Cobblestone|Blackstone 3 for craft stone_pickaxe".to_owned()]
        );
    }
}
