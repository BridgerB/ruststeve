//! The needs planner (brain/ruststeve/design/goal-table-and-needs.md): a port of the part of
//! clojurecraft's `make.clj` that wood and the 2x2 grid need. A goal row says what it provides;
//! when the inventory does not hold it, the planner walks back through producer rows (the goal
//! table's own, gather a log, and one generated row per 2x2 recipe) to the first row whose needs
//! are all met, and that row is the act. It is re-derived from the inventory every tick, so no
//! progress is stored: a consumed or lost item is planned for again. Needs are netted against one
//! working inventory, so two needs never count the same item twice.
//!
//! Issue 11b adds block needs (`block:crafting_table`: a remembered table near, else a row that
//! places one; 3x3 recipes need it) and `holds` (a tool a row keeps rather than spends, resolved
//! once). Not ported yet: spares, fuel and smelting: their issues bring them.

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
    /// `item:stick`, `tag:logs`, `block:crafting_table`, or the set's items joined by `|`.
    pub key: String,
    pub items: Vec<ItemKind>,
    /// A `block:<name>` need: such a block remembered near (its count is not counted).
    pub block: Option<data::BlockKind>,
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
            "block" => {
                return Some(Need {
                    key: s.to_owned(),
                    items: Vec::new(),
                    block: Some(data::BlockKind::from_name(name)?),
                });
            }
            _ => return None,
        };
        Some(Need {
            key: s.to_owned(),
            items: Self::sorted(items),
            block: None,
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
        Need {
            key,
            items,
            block: None,
        }
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
    /// Put a held block down near the player (`place-table`).
    Place(data::BlockKind),
    /// Dig an exposed block of these kinds with the held tool, or cut stairs down to find one.
    #[serde(skip)]
    Mine(&'static [data::BlockKind]),
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
    /// What the row keeps rather than spends (a tool): resolved once, never consumed.
    pub holds: Vec<(Need, i32)>,
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
    /// `gather`, `place` or `mine` for a producer the table itself carries.
    #[serde(default)]
    pub act: Option<String>,
    /// What a row keeps rather than spends (`[["item:wooden_pickaxe", 1]]`).
    #[serde(default)]
    pub holds: Vec<(String, i32)>,
    /// The block a `place` row puts down, or the blocks a `mine` row digs.
    #[serde(default)]
    pub blocks: Vec<String>,
}

/// `config/goals.toml`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Goals {
    pub goal: Vec<GoalRow>,
    /// The always-rows (issue 12b): checked every tick ahead of every goal, in order.
    #[serde(default)]
    pub always: Vec<AlwaysRow>,
    /// The eat and hunt rows' numbers (issue 12c).
    #[serde(default)]
    pub survival: Survival,
}

/// `[survival]` in `config/goals.toml` (clojurecraft's eat.clj and forage.clj).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Survival {
    /// Food under this is hunger to answer (`hungry-below`).
    pub hungry_below: i32,
    /// Food the bot never eats, by item name (`unsafe`).
    pub unsafe_foods: Vec<String>,
    /// Blocks from the feet an animal is hunted within (`hunt-radius`) ...
    pub hunt_radius: f64,
    /// ... and within this when starving: food at or under `starving_below`.
    pub starving_below: i32,
    pub starving_radius: f64,
}

impl Default for Survival {
    fn default() -> Self {
        Self {
            hungry_below: 14,
            unsafe_foods: Vec::new(),
            hunt_radius: 24.0,
            starving_below: 6,
            starving_radius: 48.0,
        }
    }
}

/// What an always-row answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Always {
    Escape(crate::escape::TrapKind),
    /// Hungry with something edible held (issue 12c).
    Eat,
    /// Hungry with nothing edible held: kill an animal for its meat (issue 12c).
    Hunt,
}

/// The survival numbers, read.
#[derive(Clone, Debug, PartialEq)]
pub struct SurvivalRules {
    pub hungry_below: i32,
    pub unsafe_foods: Vec<data::ItemKind>,
    pub hunt_radius: f64,
    pub starving_below: i32,
    pub starving_radius: f64,
}

/// An always-row of `config/goals.toml` (clojurecraft's `:goal/always?` rows): a trap it
/// answers, ahead of whatever task runs.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AlwaysRow {
    pub id: String,
    pub priority: u32,
    #[serde(default)]
    pub doc: String,
    /// `water`, `lava` or `fire` for an escape row ...
    #[serde(default)]
    pub trap: Option<String>,
    /// ... or `eat` or `hunt`.
    #[serde(default)]
    pub act: Option<String>,
}

fn row_of(g: &GoalRow) -> Result<Row, String> {
    let need = |(s, n): &(String, i32)| {
        Need::parse(s)
            .map(|k| (k, *n))
            .ok_or_else(|| format!("goal {}: no need {s}", g.id))
    };
    let blocks: Vec<data::BlockKind> = g
        .blocks
        .iter()
        .map(|b| {
            data::BlockKind::from_name(b).ok_or_else(|| format!("goal {}: no block {b}", g.id))
        })
        .collect::<Result<_, _>>()?;
    let act = match (g.act.as_deref(), blocks.first()) {
        (None, _) => None,
        (Some("gather"), _) => Some(Act::Gather),
        (Some("place"), Some(b)) => Some(Act::Place(*b)),
        (Some("mine"), Some(_)) => Some(Act::Mine(Box::leak(blocks.clone().into_boxed_slice()))),
        (Some(other), _) => return Err(format!("goal {}: no act {other} (or no blocks)", g.id)),
    };
    Ok(Row {
        id: g.id.clone(),
        needs: g.needs.iter().map(need).collect::<Result<_, _>>()?,
        provides: g.provides.iter().map(need).collect::<Result<_, _>>()?,
        holds: g.holds.iter().map(need).collect::<Result<_, _>>()?,
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
    /// The always-rows, by priority then as written (issue 12b).
    pub always: Vec<(String, Always)>,
    pub survival: SurvivalRules,
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
        producers.extend(data::generated::recipes::RECIPES.iter().map(|r| {
            // A recipe that does not fit the 2x2 grid needs a crafting table near,
            // after its ingredients (clojurecraft's `craft-row`).
            let mut needs = recipe_needs(r);
            if !fits_grid(r) {
                needs.push((
                    Need {
                        key: "block:crafting_table".to_owned(),
                        items: Vec::new(),
                        block: Some(data::BlockKind::CraftingTable),
                    },
                    1,
                ));
            }
            Row {
                id: format!("craft:{}", r.id),
                needs,
                provides: vec![(
                    Need {
                        key: format!("item:{}", r.result.name().unwrap_or_default()),
                        items: vec![r.result],
                        block: None,
                    },
                    i32::from(r.count),
                )],
                holds: Vec::new(),
                act: Some(Act::Craft(r)),
            }
        }));
        let mut rows: Vec<&AlwaysRow> = g.always.iter().collect();
        rows.sort_by_key(|r| r.priority);
        let always = rows
            .iter()
            .map(|r| {
                let a = match (r.trap.as_deref(), r.act.as_deref()) {
                    (Some(t), None) => crate::escape::TrapKind::parse(t).map(Always::Escape),
                    (None, Some("eat")) => Some(Always::Eat),
                    (None, Some("hunt")) => Some(Always::Hunt),
                    _ => None,
                };
                a.map(|a| (r.id.clone(), a))
                    .ok_or_else(|| format!("always {}: needs one trap or act eat/hunt", r.id))
            })
            .collect::<Result<_, _>>()?;
        let unsafe_foods = g
            .survival
            .unsafe_foods
            .iter()
            .map(|n| data::ItemKind::from_name(n).ok_or_else(|| format!("survival: no item {n}")))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            goals,
            producers,
            always,
            survival: SurvivalRules {
                hungry_below: g.survival.hungry_below,
                unsafe_foods,
                hunt_radius: g.survival.hunt_radius,
                starving_below: g.survival.starving_below,
                starving_radius: g.survival.starving_radius,
            },
        })
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
    /// Whether a block of a kind is remembered near (`make/near`): the world, read by the shell.
    near: &'a dyn Fn(data::BlockKind) -> bool,
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
        // What the row holds is resolved once and handed back unconsumed (`resolve-holds`).
        for (k, n) in &row.holds {
            match self.resolve_need(counts.clone(), k, *n, path) {
                (_, Some(Err(()))) => return (counts, Some(Err(()))),
                (c2, Some(Ok(r))) => return (c2, Some(Ok(r))),
                (_, None) => {}
            }
        }
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
        if let Some(b) = k.block {
            // `resolve-block-need`: met when one is remembered near, else the first producer that
            // provides it and can be done once.
            if (self.near)(b) {
                return (counts, None);
            }
            let rows: Vec<&'a Row> = self
                .table
                .producers
                .iter()
                .filter(|r| r.provides.iter().any(|(p, _)| p.block == Some(b)))
                .collect();
            return self.first_resolved(counts, &rows, 1, path);
        }
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
pub fn make<'a>(
    table: &'a Table,
    counts: &Counts,
    near: &'a dyn Fn(data::BlockKind) -> bool,
    goal: &'a Row,
) -> Choice<'a> {
    let p = Planner { table, near };
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
                    holds: Vec::new(),
                    blocks: Vec::new(),
                },
                GoalRow {
                    id: "gather".to_owned(),
                    priority: 10,
                    until: None,
                    doc: String::new(),
                    needs: Vec::new(),
                    provides: vec![("tag:logs".to_owned(), 1)],
                    act: Some("gather".to_owned()),
                    holds: Vec::new(),
                    blocks: Vec::new(),
                },
            ],
            always: Vec::new(),
            survival: Survival::default(),
        };
        Table::new(&g).unwrap()
    }

    fn choice(held: &[(ItemKind, i32)]) -> String {
        let t = table();
        let goal = t.until("table").unwrap();
        let counts: Counts = held.iter().copied().collect();
        match make(&t, &counts, &|_| false, goal) {
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
