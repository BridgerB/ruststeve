//! The keep rule (issue 77, brain/ruststeve/design/keep-rule.md): what a full pack drops first,
//! as data. `config/keep.toml` has one row per item and context. Each row lists the stages
//! that need the item and how many. `Keep::rank` ranks a kind's count by that need, least
//! needed first, and the drop rule (`Plan::equip`) drops the least needed stack it may. The
//! rows are the old goals.toml junk lists, counts and gates unchanged (the tests below hold
//! them equal). The table is compiled in (commit-pinned) and parsed into `Plan::keep` at
//! construction. This module also writes the ledger's `want` line, for a craft or a smelt that
//! fails for want of an item.

use std::cmp::Ordering;
use std::sync::Arc;

use data::ItemKind;
use world::World;
use world::stage::Stage;

use crate::{Plan, Task, craft, smelt};

/// The table, compiled in: an edit needs a rebuild (unlike goals.toml, read by the shell).
pub const KEEP_TOML: &str = include_str!("../../../config/keep.toml");

/// Where a row applies: the old drop gates (`Plan::equip`), in their find-first order. A
/// kind's first context in this order that holds and has a row is the one that counts.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    /// The [survival] junk list: everywhere but in the overland hut.
    Always,
    /// The [stronghold] junk list: the overworld with the stronghold the aim.
    StrongholdAim,
    /// The [fortress_way] junk list (Q11): the Nether's home leg once the tunnel has begun.
    FortressTunnel,
    /// The Q8 list: the Nether's home leg, the top-up over and the tunnel not begun.
    HomeLeg,
}

impl Context {
    /// Every context, in the find-first order.
    pub const ALL: [Context; 4] = [
        Context::Always,
        Context::StrongholdAim,
        Context::FortressTunnel,
        Context::HomeLeg,
    ];

    /// The name keep.toml and the ledger use.
    pub fn name(self) -> &'static str {
        match self {
            Context::Always => "always",
            Context::StrongholdAim => "stronghold_aim",
            Context::FortressTunnel => "fortress_tunnel",
            Context::HomeLeg => "home_leg",
        }
    }
}

/// How much a kind's count is needed, ordered least needed first: surplus, then needed by a
/// later stage, then needed by a nearer one (a larger count breaks a tie), then a kind with no
/// row in the contexts that hold (never dropped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    /// No stage still counting needs it, or more is held than the largest need.
    Surplus,
    /// Held within its count: `stage` is the nearest stage still counting that needs it, `by`
    /// how many that stage needs.
    Needed { stage: Stage, by: i32 },
    /// No row for it in the contexts that hold: never dropped.
    Unlisted,
}

impl Rank {
    /// Sort key: least needed first (a nearer stage is more needed, so it sorts later).
    fn key(self) -> (u8, std::cmp::Reverse<Stage>, i32) {
        match self {
            Rank::Surplus => (0, std::cmp::Reverse(Stage::Win), 0),
            Rank::Needed { stage, by } => (1, std::cmp::Reverse(stage), by),
            Rank::Unlisted => (2, std::cmp::Reverse(Stage::Win), 0),
        }
    }

    /// The rank as the drop line writes it.
    pub fn text(self) -> String {
        match self {
            Rank::Surplus => "surplus".to_owned(),
            Rank::Needed { stage, by } => format!("needed: {by} by {}", stage.name()),
            Rank::Unlisted => "unlisted".to_owned(),
        }
    }
}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        self.key().cmp(&other.key())
    }
}

/// One row: a kind, where it applies, and the stages that need it with their counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: ItemKind,
    pub when: Vec<Context>,
    pub needs: Vec<(Stage, i32)>,
}

impl Row {
    /// The needs still counting with `reached` the run's stage: a need counts until the run
    /// passes its stage (none reached yet: every need counts).
    fn counting(&self, reached: Option<Stage>) -> impl Iterator<Item = (Stage, i32)> + '_ {
        self.needs
            .iter()
            .copied()
            .filter(move |(s, _)| reached.is_none_or(|r| r <= *s))
    }

    /// The count kept: the largest need still counting, 0 with none.
    pub fn kept(&self, reached: Option<Stage>) -> i32 {
        self.counting(reached)
            .map(|(_, n)| n)
            .max()
            .unwrap_or(0)
            .max(0)
    }
}

/// The keep table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Keep {
    pub rows: Vec<Row>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawKeep {
    #[serde(default)]
    item: Vec<RawRow>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRow {
    name: String,
    #[serde(default = "always")]
    when: Vec<Context>,
    #[serde(default)]
    needs: Vec<(String, i32)>,
}

fn always() -> Vec<Context> {
    vec![Context::Always]
}

impl Keep {
    /// The table from keep.toml's text. An unknown item or stage name is an error, not a
    /// skipped row.
    pub fn parse(text: &str) -> Result<Keep, String> {
        let raw: RawKeep = toml::from_str(text).map_err(|e| e.to_string())?;
        let rows = raw
            .item
            .into_iter()
            .map(|r| {
                let kind = ItemKind::from_name(&r.name)
                    .ok_or_else(|| format!("keep.toml: no item {}", r.name))?;
                let needs = r
                    .needs
                    .iter()
                    .map(|(s, n)| {
                        Stage::from_name(s)
                            .map(|st| (st, *n))
                            .ok_or_else(|| format!("keep.toml: {}: no stage {s}", r.name))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(Row {
                    kind,
                    when: r.when,
                    needs,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Keep { rows })
    }

    /// The row that counts for `kind` where the contexts `ctx` hold: the first context in
    /// `Context::ALL` order that holds and has a row for it.
    pub fn row(&self, kind: ItemKind, ctx: &[Context]) -> Option<&Row> {
        Context::ALL
            .iter()
            .filter(|c| ctx.contains(c))
            .find_map(|c| {
                self.rows
                    .iter()
                    .find(|r| r.kind == kind && r.when.contains(c))
            })
    }

    /// How much `held` of `kind` is needed with `reached` the run's stage and `ctx` the
    /// contexts that hold. A pure function of the table and the counts.
    pub fn rank(&self, kind: ItemKind, held: i32, reached: Option<Stage>, ctx: &[Context]) -> Rank {
        let Some(row) = self.row(kind, ctx) else {
            return Rank::Unlisted;
        };
        let kept = row.kept(reached);
        if kept == 0 || held > kept {
            return Rank::Surplus;
        }
        row.counting(reached)
            .filter(|(_, n)| *n > 0)
            .min_by_key(|(s, _)| *s)
            .map_or(Rank::Surplus, |(stage, by)| Rank::Needed { stage, by })
    }

    /// The droppable kinds with the count each keeps, as the old junk lists had them: every
    /// kind with a row where `ctx` holds, its first row's count.
    pub fn junk(&self, reached: Option<Stage>, ctx: &[Context]) -> Vec<(ItemKind, i32)> {
        let mut out: Vec<(ItemKind, i32)> = Vec::new();
        for c in Context::ALL.iter().filter(|c| ctx.contains(c)) {
            for r in self.rows.iter().filter(|r| r.when.contains(c)) {
                if !out.iter().any(|(k, _)| *k == r.kind) {
                    out.push((r.kind, r.kept(reached)));
                }
            }
        }
        out
    }
}

/// The table as the planner holds it: the compiled-in one, or why it could not be read (the
/// drop rule then falls back to the goals.toml junk lists, noted once).
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub table: Result<Arc<Keep>, String>,
}

impl Loaded {
    /// The compiled-in keep.toml.
    pub fn compiled() -> Loaded {
        Loaded {
            table: Keep::parse(KEEP_TOML).map(Arc::new),
        }
    }
}

impl Default for Loaded {
    fn default() -> Self {
        Loaded::compiled()
    }
}

/// The main slot (9 to 44) to drop with its kind and rank, from the stacks whose drop still
/// leaves their kind's keep held (`junk`): the least needed by `rank` of what the drop leaves,
/// then the lowest slot. None with no such stack.
pub fn least_needed(
    slots: &std::collections::BTreeMap<i16, (ItemKind, i32)>,
    junk: &[(ItemKind, i32)],
    rank: &dyn Fn(ItemKind, i32) -> Rank,
) -> Option<(i16, ItemKind, Rank)> {
    let main = |s: &i16| (9..=44).contains(s);
    slots
        .iter()
        .filter(|(s, _)| main(s))
        .filter_map(|(s, (k, n))| {
            let keep = junk.iter().find(|(j, _)| j == k)?.1;
            let total: i32 = slots
                .iter()
                .filter(|(s2, (k2, _))| main(s2) && k2 == k)
                .map(|(_, (_, n2))| n2)
                .sum();
            let left = total - n;
            (left >= keep).then(|| (*s, *k, rank(*k, left)))
        })
        .min_by(|a, b| a.2.cmp(&b.2).then(a.0.cmp(&b.0)))
}

/// Kinds as a want line names them: one, or every kind the recipe's cell takes, joined by `|`.
fn kinds_text(set: &[ItemKind]) -> String {
    set.iter()
        .map(|k| format!("{k:?}"))
        .collect::<Vec<_>>()
        .join("|")
}

/// What a craft is short of: for each ingredient set of its layout, the cells that take it,
/// less what is held of it (the inventory, the cursor and the grid). Only the short ones.
pub fn craft_short(
    w: &World,
    recipe: &data::types::Recipe,
    table: bool,
) -> Vec<(&'static [ItemKind], i32)> {
    let side = if table { 3 } else { 2 };
    let mut sets: Vec<(&'static [ItemKind], i32)> = Vec::new();
    for (_, set) in craft::layout(recipe, side) {
        match sets.iter_mut().find(|(s, _)| *s == set) {
            Some((_, n)) => *n += 1,
            None => sets.push((set, 1)),
        }
    }
    let held = crate::held(w);
    // A table's grid is in its window's slots 1 to 9; window 0's grid (1 to 4) is in `held`.
    let grid = |k: ItemKind| -> i32 {
        if !table {
            return 0;
        }
        w.window.as_ref().map_or(0, |win| {
            (1i16..=9)
                .filter_map(|s| win.slots.get(&s))
                .filter(|(gk, _)| *gk == k)
                .map(|(_, n)| *n)
                .sum()
        })
    };
    sets.into_iter()
        .filter_map(|(set, need)| {
            let have: i32 = set
                .iter()
                .map(|k| held.get(k).copied().unwrap_or(0) + grid(*k))
                .sum();
            (have < need).then_some((set, need - have))
        })
        .collect()
}

impl Plan {
    /// The contexts that hold now: the old drop gates of `Plan::equip`, unchanged.
    pub(crate) fn keep_contexts(&self, w: &World) -> Vec<Context> {
        // Q4: the Nether's leftovers go in the overworld while the stronghold is the aim (race
        // 45695074789 lane 0's full pack took no cobblestone from nine stair legs, and lane 1's
        // crafts failed on a full pack).
        let stronghold_aim = self.aim == crate::Aim::Race
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == crate::stronghold::OVERWORLD)
            && (craft::count(w, ItemKind::EnderEye) > 0 || self.stronghold.room.is_some());
        // Q11 (issue 19): the fortress stretch's tunnel fills the pack (race 45766983995: every
        // wooden pickaxe craft failed on a full pack) (`crate::fortress_way`).
        let nether_home = self.ender.phase == crate::endermen::EnderPhase::Home
            && w.dimension
                .as_ref()
                .is_some_and(|d| d.name == crate::home::NETHER);
        let fortress_tunnel = nether_home && self.ender.home.fortress.begun;
        // Q8 (issue 19): the same leftovers on the whole trip home in the Nether, the top-up over
        // (the coordinator, 2026-11-26: Q11's race 45766983995 filled every pack before its
        // tunnel), basalt kept to 64 for the guard and the bridges (the deltas' restock digs it).
        let home_leg =
            nether_home && self.ender.home.top_up_over && !self.ender.home.fortress.begun;
        [
            // The [survival] list came with the goal table: a planner without one dropped none.
            (Context::Always, self.table.is_some()),
            (Context::StrongholdAim, stronghold_aim),
            (Context::FortressTunnel, fortress_tunnel),
            (Context::HomeLeg, home_leg),
        ]
        .into_iter()
        .filter_map(|(c, on)| on.then_some(c))
        .collect()
    }

    /// The rank the drop rules order by: the keep table's for the stage reached and the
    /// contexts that hold, or one rank for all (the old order) when the table is unreadable.
    pub(crate) fn keep_rank(&self, w: &World) -> impl Fn(ItemKind, i32) -> Rank + use<> {
        let table = self.keep.table.clone().ok();
        let reached = w.stage_reached;
        let ctx = self.keep_contexts(w);
        move |kind, held| {
            table
                .as_ref()
                .map_or(Rank::Surplus, |k| k.rank(kind, held, reached, &ctx))
        }
    }

    /// The old junk lists as `Plan::equip` composed them before the keep table, for the
    /// contexts `ctx` (survival, then the leg's list; Q8 is the fortress list with netherrack and
    /// basalt at home's kept counts). Only the fallback for a keep table that could not be read.
    pub(crate) fn junk_lists(&self, ctx: &[Context]) -> Vec<(ItemKind, i32)> {
        let mut junk = self
            .table
            .as_ref()
            .map(|t| t.survival.junk.clone())
            .unwrap_or_default();
        if ctx.contains(&Context::StrongholdAim) {
            junk.extend(self.stronghold_rules().junk_kinds());
        }
        if ctx.contains(&Context::FortressTunnel) {
            junk.extend(self.fortress_rules().junk_kinds());
        }
        if ctx.contains(&Context::HomeLeg) {
            junk.extend(
                self.fortress_rules()
                    .junk_kinds()
                    .into_iter()
                    .map(|(k, n)| {
                        if k == ItemKind::Basalt {
                            (k, crate::home::BASALT_KEPT)
                        } else if k == ItemKind::Netherrack {
                            (k, crate::home::NETHERRACK_KEPT)
                        } else {
                            (k, n)
                        }
                    }),
            );
        }
        junk
    }

    /// The ledger's `want` lines for the needs planner (`make::make_traced`): one for each item
    /// need its chosen path meets from a producer, not from what is held (`want <kinds> <n> for
    /// goal <id>`, or `want fuel <ticks> ...`). One line an episode: a key is written when it
    /// joins the short set, not again while it stays short, again if it leaves and comes back;
    /// a new goal starts afresh.
    pub(crate) fn goal_wants(&mut self, goal: &str, short: &[crate::make::Short], tick: u64) {
        if self.keep_wants.0 != goal {
            self.keep_wants = (goal.to_owned(), Vec::new());
        }
        self.keep_wants
            .1
            .retain(|k| short.iter().any(|s| s.key == *k));
        for s in short {
            if self.keep_wants.1.contains(&s.key) {
                continue;
            }
            self.keep_wants.1.push(s.key.clone());
            let kinds = if s.items.is_empty() {
                "fuel".to_owned()
            } else {
                kinds_text(&s.items)
            };
            self.note(
                tick,
                format!("want {kinds} {} for goal {goal}", s.n),
                format!("the needs planner makes {} rather than holds it", s.key),
            );
        }
    }

    /// The ledger's `want` lines for a craft that failed `Missing`: one for each ingredient it is
    /// short of (`want <kinds> <n> for craft <recipe>`), or one note when nothing is short (a
    /// full pack or a stale grid made the failure, not a want).
    pub(crate) fn craft_want(&mut self, w: &World, f: craft::CraftFailure, tick: u64) {
        let missing = match f {
            craft::CraftFailure::Missing => true,
            craft::CraftFailure::StaleWindow
            | craft::CraftFailure::WrongResult
            | craft::CraftFailure::TooManyClicks
            | craft::CraftFailure::Unopened => false,
        };
        let Task::Craft(c) = &self.task else {
            return;
        };
        if !missing {
            return;
        }
        let (id, table) = (c.recipe.clone(), c.table.is_some());
        let Some(recipe) = data::generated::recipes::RECIPES
            .iter()
            .find(|r| r.id == id)
        else {
            return;
        };
        let short = craft_short(w, recipe, table);
        if short.is_empty() {
            self.note(
                tick,
                format!("craft {id} missing with every ingredient held"),
                "no want: the failure was the pack or the grid".to_owned(),
            );
            return;
        }
        for (set, n) in short {
            self.note(
                tick,
                format!("want {} {n} for craft {id}", kinds_text(set)),
                "the craft failed Missing".to_owned(),
            );
        }
    }

    /// The ledger's `want` lines for a smelt that failed `Missing`: its input short of the batch
    /// (`want <kind> <n> for smelt <result>`), or no fuel held (`want fuel 1 ...`), or one note
    /// when neither is short.
    pub(crate) fn smelt_want(&mut self, w: &World, f: smelt::SmeltFailure, tick: u64) {
        let missing = match f {
            smelt::SmeltFailure::Missing => true,
            smelt::SmeltFailure::Unopened
            | smelt::SmeltFailure::Occupied
            | smelt::SmeltFailure::StaleWindow
            | smelt::SmeltFailure::TooManyClicks
            | smelt::SmeltFailure::Slow => false,
        };
        let Task::Smelt(sm) = &self.task else {
            return;
        };
        if !missing {
            return;
        }
        let (input, result, count) = (sm.input, sm.result, sm.count);
        // What the furnace's input slot already holds counts toward the batch.
        let loaded = w
            .window
            .as_ref()
            .and_then(|win| win.slots.get(&smelt::INPUT).copied())
            .filter(|(k, _)| *k == input)
            .map_or(0, |(_, n)| n);
        let short_in = count - crate::held(w).get(&input).copied().unwrap_or(0) - loaded;
        let no_fuel = smelt::fuels_held(w).is_empty();
        let mut wanted = false;
        if short_in > 0 {
            wanted = true;
            self.note(
                tick,
                format!("want {input:?} {short_in} for smelt {result:?}"),
                "the smelt failed Missing".to_owned(),
            );
        }
        if no_fuel {
            wanted = true;
            self.note(
                tick,
                format!("want fuel 1 for smelt {result:?}"),
                "the smelt failed Missing: no fuel held".to_owned(),
            );
        }
        if !wanted {
            self.note(
                tick,
                format!("smelt {result:?} missing with input and fuel held"),
                "no want: the failure was the pack or a slot".to_owned(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data::ItemKind::{
        Andesite, Basalt, Bucket, Cobblestone, Feather, Gravel, Netherrack, SoulSoil, String as Str,
    };
    use std::collections::BTreeMap;

    fn table() -> Keep {
        Keep::parse(KEEP_TOML).unwrap()
    }

    fn full_of_buckets() -> BTreeMap<i16, (ItemKind, i32)> {
        (9i16..=44).map(|s| (s, (Bucket, 1))).collect()
    }

    /// The committed keep.toml parses, every name an item and every stage a stage.
    #[test]
    fn the_committed_table_parses() {
        let k = Keep::parse(KEEP_TOML);
        assert!(k.is_ok(), "{k:?}");
        assert!(table().rows.len() > 40);
    }

    /// An unknown stage is an error, never a row skipped.
    #[test]
    fn an_unknown_stage_or_item_is_an_error() {
        assert!(Keep::parse("[[item]]\nname = \"string\"\nneeds = [[\"bow\", 3]]").is_err());
        assert!(Keep::parse("[[item]]\nname = \"no_such_item\"").is_err());
    }

    /// The issue's test: a full pack drops cobblestone before string. Both have two stacks, and
    /// each drop would leave its keep, but string's need (the bow line) is nearer.
    #[test]
    fn a_full_inventory_drops_cobblestone_before_string() {
        let k = table();
        let ctx = [Context::Always];
        let reached = Some(Stage::Nether);
        let junk = k.junk(reached, &ctx);
        let mut slots = full_of_buckets();
        slots.insert(9, (Str, 8));
        slots.insert(10, (Str, 8));
        slots.insert(11, (Cobblestone, 64));
        slots.insert(12, (Cobblestone, 64));
        let rank = |kind: ItemKind, held: i32| k.rank(kind, held, reached, &ctx);
        let (slot, kind, r) = least_needed(&slots, &junk, &rank).unwrap();
        assert_eq!((slot, kind), (11, Cobblestone));
        assert_eq!(
            r,
            Rank::Needed {
                stage: Stage::Win,
                by: 64
            }
        );
        // With the cobblestone gone to its keep, string is next: its drop still leaves 8.
        slots.remove(&11);
        slots.insert(11, (Bucket, 1));
        let (slot, kind, _) = least_needed(&slots, &junk, &rank).unwrap();
        assert_eq!((slot, kind), (9, Str));
        // Pure junk (needed by nothing) goes before either.
        slots.insert(44, (Andesite, 3));
        let (slot, kind, r) = least_needed(&slots, &junk, &rank).unwrap();
        assert_eq!((slot, kind, r), (44, Andesite, Rank::Surplus));
    }

    /// Ranks are ordered least needed first.
    #[test]
    fn ranks_order_least_needed_first() {
        let far = Rank::Needed {
            stage: Stage::Win,
            by: 64,
        };
        let near = Rank::Needed {
            stage: Stage::BowArmed,
            by: 8,
        };
        assert!(Rank::Surplus < far && far < near && near < Rank::Unlisted);
        let k = table();
        let ctx = [Context::Always];
        assert_eq!(k.rank(Cobblestone, 65, None, &ctx), Rank::Surplus);
        assert_eq!(k.rank(Cobblestone, 64, None, &ctx), far);
        assert_eq!(k.rank(Andesite, 5, None, &ctx), Rank::Surplus);
        assert_eq!(k.rank(Netherrack, 500, None, &ctx), Rank::Unlisted);
        assert_eq!(k.rank(Bucket, 1, None, &ctx), Rank::Unlisted);
        // Past the bow line, string's nearest need is the end's.
        assert_eq!(
            k.rank(Str, 8, Some(Stage::End), &ctx),
            Rank::Needed {
                stage: Stage::Win,
                by: 8
            }
        );
    }

    /// The gates are the old ones: the Nether's leftovers drop only where their list did, and
    /// netherrack keeps each list's count.
    #[test]
    fn the_contexts_keep_the_old_gates() {
        let k = table();
        let count = |ctx: &[Context], kind| {
            k.junk(None, ctx)
                .iter()
                .find(|(j, _)| *j == kind)
                .map(|(_, n)| *n)
        };
        assert_eq!(count(&[Context::Always], SoulSoil), None);
        assert_eq!(count(&[Context::Always], Netherrack), None);
        assert_eq!(
            count(&[Context::Always, Context::StrongholdAim], Netherrack),
            Some(64)
        );
        assert_eq!(
            count(&[Context::Always, Context::FortressTunnel], Netherrack),
            Some(128)
        );
        assert_eq!(
            count(&[Context::Always, Context::HomeLeg], Netherrack),
            Some(192)
        );
        assert_eq!(
            count(&[Context::Always, Context::HomeLeg], Basalt),
            Some(64)
        );
        assert_eq!(
            count(&[Context::Always, Context::FortressTunnel], Basalt),
            Some(0)
        );
        // Gravel: survival's 16 first, as the old find-first order had it.
        assert_eq!(
            count(&[Context::Always, Context::FortressTunnel], Gravel),
            Some(16)
        );
        assert_eq!(count(&[Context::Always], Feather), Some(16));
        // The same full pack in the Nether drops soul soil only on the home leg.
        let mut slots = full_of_buckets();
        slots.insert(20, (SoulSoil, 40));
        for (ctx, want) in [
            (vec![Context::Always], None),
            (vec![Context::Always, Context::HomeLeg], Some(20)),
            (vec![Context::Always, Context::FortressTunnel], Some(20)),
        ] {
            let junk = k.junk(None, &ctx);
            let rank = |kind: ItemKind, held: i32| k.rank(kind, held, None, &ctx);
            assert_eq!(
                least_needed(&slots, &junk, &rank).map(|(s, _, _)| s),
                want,
                "{ctx:?}"
            );
        }
    }

    /// The forest base's drop (`base::drop_slot_by`) keeps its gate and orders by rank: with
    /// string listed before cobblestone, cobblestone (needed later, by the end) goes first;
    /// one rank for all is the old list order.
    #[test]
    fn the_base_drops_cobblestone_before_string() {
        let k = table();
        let ctx = [Context::Always];
        let mut w = World::new();
        w.inventory.slots = full_of_buckets();
        w.inventory.slots.insert(9, (Str, 8));
        w.inventory.slots.insert(10, (Cobblestone, 64));
        let base = crate::base::Base {
            drop: vec!["string".to_owned(), "cobblestone".to_owned()],
            ..crate::base::Base::default()
        };
        let rank = |kind: ItemKind, held: i32| k.rank(kind, held, Some(Stage::Nether), &ctx);
        assert_eq!(
            crate::base::drop_slot_by(&w, &base, &rank).map(|(s, k, _)| (s, k)),
            Some((10, Cobblestone))
        );
        assert_eq!(
            crate::base::drop_slot(&w, &base),
            Some((9, Str)),
            "the old order"
        );
        // A kind not in the base's list never goes, whatever its rank.
        let only = crate::base::Base {
            drop: vec!["string".to_owned()],
            ..crate::base::Base::default()
        };
        assert_eq!(
            crate::base::drop_slot_by(&w, &only, &rank).map(|(s, k, _)| (s, k)),
            Some((9, Str))
        );
    }

    fn goals_table() -> crate::make::Table {
        let goals: crate::make::Goals =
            toml::from_str(include_str!("../../../config/goals.toml")).unwrap();
        crate::make::Table::new(&goals).unwrap()
    }

    /// The needs planner's path names what it makes rather than holds: from nothing, the kit's
    /// sticks; with the kit held, nothing.
    #[test]
    fn the_needs_planners_path_names_its_wants() {
        let t = goals_table();
        let kit = t.until("table").unwrap();
        let near = |_: data::BlockKind| false;
        let none = crate::make::Counts::new();
        let (_, short) =
            crate::make::make_traced(&t, &none, &near, true, &crate::make::Wear::default(), kit);
        assert!(
            short.iter().any(|s| s.key == "item:stick" && s.n == 4),
            "{short:?}"
        );
        let mut held = crate::make::Counts::new();
        held.insert(ItemKind::Stick, 4);
        held.insert(ItemKind::CraftingTable, 1);
        let (choice, short) =
            crate::make::make_traced(&t, &held, &near, true, &crate::make::Wear::default(), kit);
        assert_eq!(choice, crate::make::Choice::Done);
        assert!(short.is_empty(), "{short:?}");
    }

    /// One want line an episode: a key short again on the next tick writes nothing; one that
    /// left and came back writes again; a new goal starts afresh.
    #[test]
    fn a_want_is_written_once_an_episode() {
        let mut p = Plan::new(crate::Aim::Race);
        let stick = crate::make::Short {
            key: "item:stick".to_owned(),
            items: vec![ItemKind::Stick],
            n: 4,
        };
        let wants = |p: &Plan| {
            p.events
                .iter()
                .filter(|e| e.what.starts_with("want "))
                .count()
        };
        p.goal_wants("kit", std::slice::from_ref(&stick), 1);
        p.goal_wants("kit", std::slice::from_ref(&stick), 2);
        assert_eq!(wants(&p), 1);
        assert_eq!(
            p.events.last().map(|e| e.what.as_str()),
            Some("want Stick 4 for goal kit")
        );
        p.goal_wants("kit", &[], 3);
        p.goal_wants("kit", std::slice::from_ref(&stick), 4);
        assert_eq!(wants(&p), 2);
        p.goal_wants("iron", std::slice::from_ref(&stick), 5);
        assert_eq!(wants(&p), 3);
    }

    #[derive(serde::Deserialize)]
    struct Lists {
        survival: List,
        stronghold: List,
        fortress_way: List,
    }

    #[derive(serde::Deserialize)]
    struct List {
        junk: Vec<(String, i32)>,
    }

    type Junk = Vec<(ItemKind, i32)>;

    fn kinds(l: &[(String, i32)]) -> Junk {
        l.iter()
            .map(|(n, c)| (ItemKind::from_name(n).unwrap(), *c))
            .collect()
    }

    /// No regression: for every context and stage, keep.toml's counts are the goals.toml junk
    /// lists' as `Plan::equip` composed them (survival, then the leg's list; Q8 the fortress list
    /// with netherrack and basalt at home's kept counts), find-first.
    #[test]
    fn the_rows_are_the_old_junk_lists() {
        let text = include_str!("../../../config/goals.toml");
        let lists: Lists = toml::from_str(text).unwrap();
        let survival = kinds(&lists.survival.junk);
        let stronghold = kinds(&lists.stronghold.junk);
        let fortress = kinds(&lists.fortress_way.junk);
        let q8: Vec<(ItemKind, i32)> = fortress
            .iter()
            .map(|(k, n)| match k {
                Basalt => (*k, crate::home::BASALT_KEPT),
                Netherrack => (*k, crate::home::NETHERRACK_KEPT),
                _ => (*k, *n),
            })
            .collect();
        let k = table();
        let cases: [(Vec<Context>, Junk); 4] = [
            (vec![Context::Always], survival.clone()),
            (
                vec![Context::Always, Context::StrongholdAim],
                [survival.clone(), stronghold].concat(),
            ),
            (
                vec![Context::Always, Context::FortressTunnel],
                [survival.clone(), fortress].concat(),
            ),
            (
                vec![Context::Always, Context::HomeLeg],
                [survival, q8].concat(),
            ),
        ];
        let first = |l: &[(ItemKind, i32)], kind: ItemKind| {
            l.iter().find(|(j, _)| *j == kind).map(|(_, n)| *n)
        };
        for (ctx, old) in &cases {
            for reached in std::iter::once(None).chain(Stage::ALL.into_iter().map(Some)) {
                let new = k.junk(reached, ctx);
                for (kind, _) in old.iter().chain(new.iter()) {
                    assert_eq!(
                        first(&new, *kind),
                        first(old, *kind),
                        "{kind:?} in {ctx:?} at {reached:?}"
                    );
                }
            }
        }
    }
}
