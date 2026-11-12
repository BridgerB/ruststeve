//! The needs planner (brain/ruststeve/design/goal-table-and-needs.md): a port of the part of
//! clojurecraft's `make.clj` that wood and the 2x2 grid need. A goal row says what it provides;
//! when the inventory does not hold it, the planner walks back through producer rows (the goal
//! table's own, gather a log, and one generated row per 2x2 recipe) to the first row whose needs
//! are all met, and that row is the act. It is re-derived from the inventory every tick, so no
//! progress is stored: a consumed or lost item is planned for again. Needs are netted against one
//! working inventory, so two needs never count the same item twice.
//!
//! Not ported yet: block needs (a crafting table nearby, so 3x3 recipes), held tools and spares,
//! fuel and smelting: their issues bring them.

use std::collections::BTreeMap;

use data::ItemKind;
use data::types::{Recipe, Shape};

/// Item needs the planner may chain before it is stuck (clojurecraft's `max-depth`).
pub const MAX_DEPTH: usize = 12;

/// Held items by kind.
pub type Counts = BTreeMap<ItemKind, i32>;

/// A counted need's items: one item, a tag's, or a recipe ingredient's set, sorted by name (the
/// order `consume` takes them in, clojurecraft's `recipe/consume`).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub struct Need {
    /// `item:stick`, `tag:logs`, or the set's items joined by `|`.
    pub key: String,
    pub items: Vec<ItemKind>,
}

impl Need {
    fn sorted(mut items: Vec<ItemKind>) -> Vec<ItemKind> {
        items.sort_by_key(|k| k.name().unwrap_or_default());
        items.dedup();
        items
    }

    /// `item:<name>` or `tag:<item tag>`.
    pub fn parse(s: &str) -> Option<Need> {
        let (kind, name) = s.split_once(':')?;
        let items = match kind {
            "item" => vec![ItemKind::from_name(name)?],
            "tag" => data::generated::tags::item_tag(name)?.to_vec(),
            _ => return None,
        };
        Some(Need {
            key: s.to_owned(),
            items: Self::sorted(items),
        })
    }

    /// A recipe ingredient's set, named as clojurecraft's `need-key` names it: the shortest tag
    /// with exactly that set, the one item, or the set itself.
    fn of_set(set: &[ItemKind]) -> Need {
        let items = Self::sorted(set.to_vec());
        let tag = data::generated::tags::ITEM_TAGS
            .iter()
            .filter(|(_, t)| Self::sorted(t.to_vec()) == items)
            .min_by_key(|(n, _)| (n.len(), *n))
            .map(|(n, _)| format!("tag:{n}"));
        let key = match (tag, items.as_slice()) {
            (Some(t), _) => t,
            (None, [one]) => format!("item:{}", one.name().unwrap_or_default()),
            (None, many) => many
                .iter()
                .map(|k| k.name().unwrap_or_default())
                .collect::<Vec<_>>()
                .join("|"),
        };
        Need { key, items }
    }

    /// How many of these items the counts hold (`recipe/have`).
    pub fn have(&self, counts: &Counts) -> i32 {
        self.items
            .iter()
            .map(|k| counts.get(k).copied().unwrap_or(0))
            .sum()
    }

    /// The counts with `n` of these items taken, in name order (`recipe/consume`).
    fn consume(&self, counts: &Counts, mut n: i32) -> Counts {
        let mut c = counts.clone();
        for k in &self.items {
            if n == 0 {
                break;
            }
            let held = c.get(k).copied().unwrap_or(0);
            let take = n.min(held);
            if take > 0 {
                c.insert(*k, held - take);
            }
            n -= take;
        }
        c
    }
}

/// What a producer row does when its needs are met.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Act {
    /// Dig a log seen with an exposed face.
    Gather,
    /// One run of a 2x2 recipe in window 0.
    #[serde(skip)]
    Craft(&'static Recipe),
}

/// A row of the goal table, or a generated producer row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub id: String,
    pub needs: Vec<(Need, i32)>,
    pub provides: Vec<(Need, i32)>,
    pub act: Option<Act>,
}

/// One row of `config/goals.toml` as written.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GoalRow {
    pub id: String,
    pub priority: u32,
    /// The `--goal` name that puts this row in play (`table`).
    #[serde(default)]
    pub until: Option<String>,
    #[serde(default)]
    pub doc: String,
    /// `[["item:stick", 4], ["item:crafting_table", 1]]`, in the order they are planned.
    #[serde(default)]
    pub needs: Vec<(String, i32)>,
    pub provides: Vec<(String, i32)>,
    /// `gather` for a producer the table itself carries.
    #[serde(default)]
    pub act: Option<String>,
}

/// `config/goals.toml`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Goals {
    pub goal: Vec<GoalRow>,
}

fn row_of(g: &GoalRow) -> Result<Row, String> {
    let need = |(s, n): &(String, i32)| {
        Need::parse(s)
            .map(|k| (k, *n))
            .ok_or_else(|| format!("goal {}: no need {s}", g.id))
    };
    let act = match g.act.as_deref() {
        None => None,
        Some("gather") => Some(Act::Gather),
        Some(other) => return Err(format!("goal {}: no act {other}", g.id)),
    };
    Ok(Row {
        id: g.id.clone(),
        needs: g.needs.iter().map(need).collect::<Result<_, _>>()?,
        provides: g.provides.iter().map(need).collect::<Result<_, _>>()?,
        act,
    })
}

/// A recipe's ingredient sets and how many of each one craft uses (`recipe/needs`), in the order
/// they first appear.
fn recipe_needs(r: &Recipe) -> Vec<(Need, i32)> {
    let sets: Vec<&[ItemKind]> = match r.shape {
        Shape::Shaped { cells, .. } => cells.iter().flatten().copied().collect(),
        Shape::Shapeless { ingredients } => ingredients.to_vec(),
    };
    let mut out: Vec<(Need, i32)> = Vec::new();
    for s in sets {
        let n = Need::of_set(s);
        match out.iter_mut().find(|(k, _)| k.items == n.items) {
            Some((_, c)) => *c += 1,
            None => out.push((n, 1)),
        }
    }
    out
}

/// Whether a recipe fits the 2x2 grid (`recipe/fits?` with size 2).
pub fn fits_grid(r: &Recipe) -> bool {
    match r.shape {
        Shape::Shaped { width, height, .. } => width <= 2 && height <= 2,
        Shape::Shapeless { ingredients } => ingredients.len() <= 4,
    }
}

/// The goal table with its producers: the table's acting rows, then one row per 2x2 recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub goals: Vec<(u32, Option<String>, Row)>,
    pub producers: Vec<Row>,
}

impl Table {
    pub fn new(g: &Goals) -> Result<Self, String> {
        let mut goals = Vec::new();
        let mut producers = Vec::new();
        for row in &g.goal {
            let r = row_of(row)?;
            if r.act.is_some() {
                producers.push(r);
            } else {
                goals.push((row.priority, row.until.clone(), r));
            }
        }
        goals.sort_by_key(|(p, _, _)| *p);
        producers.extend(
            data::generated::recipes::RECIPES
                .iter()
                .filter(|r| fits_grid(r))
                .map(|r| Row {
                    id: format!("craft:{}", r.id),
                    needs: recipe_needs(r),
                    provides: vec![(
                        Need {
                            key: format!("item:{}", r.result.name().unwrap_or_default()),
                            items: vec![r.result],
                        },
                        i32::from(r.count),
                    )],
                    act: Some(Act::Craft(r)),
                }),
        );
        Ok(Self { goals, producers })
    }

    /// The goal row a `--goal` name puts in play.
    pub fn until(&self, name: &str) -> Option<&Row> {
        self.goals
            .iter()
            .find(|(_, u, _)| u.as_deref() == Some(name))
            .map(|(_, _, r)| r)
    }
}

/// What the planner decided for a goal.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice<'a> {
    /// The goal's provides are held.
    Done,
    /// Act on this producer row (its needs are met).
    Act(&'a Row),
    /// No producer can be done.
    Stuck,
}

/// One run's yield (the first thing it provides).
fn yield_of(row: &Row) -> i32 {
    row.provides.first().map_or(1, |(_, n)| *n)
}

/// clojurecraft's `producer-order`: the producer whose own needs the working counts cover most
/// first, then one that needs no table (all here fit the grid), then the larger yield. Ties keep
/// the producers' order (the table's rows, then the recipes in the generated order).
fn producer_order(counts: &Counts, row: &Row) -> (i32, i32) {
    let covered: i32 = row.needs.iter().map(|(k, _)| k.have(counts)).sum();
    (-covered, -yield_of(row))
}

struct Planner<'a> {
    table: &'a Table,
}

type Resolved<'a> = (Counts, Option<Result<&'a Row, ()>>);

impl<'a> Planner<'a> {
    /// `resolve-row`: try to get `times` runs of a row done: `None` when its needs are met (act
    /// on it), `Some(Ok(row))` a producer further down to act on, `Some(Err(()))` stuck.
    fn resolve_row(
        &self,
        counts: Counts,
        row: &'a Row,
        times: i32,
        path: &[Vec<ItemKind>],
    ) -> Resolved<'a> {
        let mut c = counts.clone();
        for (k, n) in &row.needs {
            let (c2, a) = self.resolve_need(c, k, n * times, path);
            match a {
                Some(Err(())) => return (counts, Some(Err(()))),
                Some(Ok(r)) => return (c2, Some(Ok(r))),
                None => c = c2,
            }
        }
        (c, Some(Ok(row)))
    }

    /// `craft-now`: a craft asked for n > 1 runs that would first send the bot elsewhere, but one
    /// run of which can be made from what is held: craft that now.
    fn craft_now(
        &self,
        counts: &Counts,
        row: &'a Row,
        n: i32,
        action: &'a Row,
        path: &[Vec<ItemKind>],
    ) -> Option<(Counts, &'a Row)> {
        if !matches!(row.act, Some(Act::Craft(_))) || n <= 1 || std::ptr::eq(action, row) {
            return None;
        }
        match self.resolve_row(counts.clone(), row, 1, path) {
            (c, Some(Ok(a))) if std::ptr::eq(a, row) => Some((c, a)),
            _ => None,
        }
    }

    /// `first-resolved`: the first of the rows that can be done its number of times.
    fn first_resolved(
        &self,
        counts: Counts,
        rows: &[&'a Row],
        missing: i32,
        path: &[Vec<ItemKind>],
    ) -> Resolved<'a> {
        for row in rows {
            let n = (missing + yield_of(row) - 1) / yield_of(row).max(1);
            if let (c, Some(Ok(a))) = self.resolve_row(counts.clone(), row, n, path) {
                if let Some((c1, a1)) = self.craft_now(&counts, row, n, a, path) {
                    return (c1, Some(Ok(a1)));
                }
                return (c, Some(Ok(a)));
            }
        }
        (counts, Some(Err(())))
    }

    /// `resolve-item-need`: met (`None`, the items consumed) when the counts hold n; else what is
    /// held is consumed and the producers of the rest tried in `producer_order`.
    fn resolve_need(
        &self,
        counts: Counts,
        k: &Need,
        n: i32,
        path: &[Vec<ItemKind>],
    ) -> Resolved<'a> {
        let h = k.have(&counts);
        if h >= n {
            return (k.consume(&counts, n), None);
        }
        let cycle = path
            .iter()
            .any(|above| above.iter().any(|i| k.items.contains(i)));
        if path.len() > MAX_DEPTH || cycle {
            return (counts, Some(Err(())));
        }
        let counts = k.consume(&counts, h);
        let mut rows: Vec<&'a Row> = Vec::new();
        for item in &k.items {
            for r in &self.table.producers {
                let provides = r.provides.iter().any(|(p, _)| p.items.contains(item));
                if provides && !rows.iter().any(|x| std::ptr::eq(*x, r)) {
                    rows.push(r);
                }
            }
        }
        rows.sort_by_key(|r| producer_order(&counts, r));
        let mut deeper = path.to_vec();
        deeper.push(k.items.clone());
        self.first_resolved(counts, &rows, n - h, &deeper)
    }
}

/// The act for a goal from what is held: re-derived each tick, nothing stored.
pub fn make<'a>(table: &'a Table, counts: &Counts, goal: &'a Row) -> Choice<'a> {
    let p = Planner { table };
    let mut c = counts.clone();
    for (k, n) in goal.provides.iter().chain(&goal.needs) {
        match p.resolve_need(c, k, *n, &[]) {
            (_, Some(Ok(row))) => return Choice::Act(row),
            (_, Some(Err(()))) => return Choice::Stuck,
            (c2, None) => c = c2,
        }
    }
    Choice::Done
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Table {
        let g = Goals {
            goal: vec![
                GoalRow {
                    id: "kit".to_owned(),
                    priority: 2,
                    until: Some("table".to_owned()),
                    doc: String::new(),
                    needs: Vec::new(),
                    provides: vec![
                        ("item:stick".to_owned(), 4),
                        ("item:crafting_table".to_owned(), 1),
                    ],
                    act: None,
                },
                GoalRow {
                    id: "gather".to_owned(),
                    priority: 10,
                    until: None,
                    doc: String::new(),
                    needs: Vec::new(),
                    provides: vec![("tag:logs".to_owned(), 1)],
                    act: Some("gather".to_owned()),
                },
            ],
        };
        Table::new(&g).unwrap()
    }

    fn choice(held: &[(ItemKind, i32)]) -> String {
        let t = table();
        let goal = t.until("table").unwrap();
        let counts: Counts = held.iter().copied().collect();
        match make(&t, &counts, goal) {
            Choice::Done => "done".to_owned(),
            Choice::Act(r) => r.id.clone(),
            Choice::Stuck => "stuck".to_owned(),
        }
    }

    #[test]
    fn the_kit_from_nothing_gathers() {
        assert_eq!(choice(&[]), "gather");
    }

    #[test]
    fn a_log_becomes_planks_then_sticks_then_a_table() {
        assert_eq!(choice(&[(ItemKind::OakLog, 1)]), "craft:oak_planks");
        assert_eq!(choice(&[(ItemKind::OakPlanks, 4)]), "craft:stick");
        // Sticks first: with two planks left and four sticks held, the table wants four planks.
        assert_eq!(
            choice(&[(ItemKind::OakPlanks, 2), (ItemKind::Stick, 4)]),
            "gather"
        );
        assert_eq!(
            choice(&[
                (ItemKind::OakPlanks, 2),
                (ItemKind::Stick, 4),
                (ItemKind::OakLog, 1)
            ]),
            "craft:oak_planks"
        );
        assert_eq!(
            choice(&[(ItemKind::OakPlanks, 6), (ItemKind::Stick, 4)]),
            "craft:crafting_table"
        );
        assert_eq!(
            choice(&[(ItemKind::Stick, 4), (ItemKind::CraftingTable, 1)]),
            "done"
        );
    }
}
