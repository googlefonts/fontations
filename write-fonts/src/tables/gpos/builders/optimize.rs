//! Size-optimized compilation of pair positioning lookups.
//!
//! The entry point is [`optimize_pairpos`]. Its docs say what it does; these
//! say how.
//!
//! # Terms
//!
//! - A *pair* is a first glyph followed by a second glyph. Its *value* is how
//!   the lookup adjusts it: a value record for each of the two glyphs. Values
//!   are interned (see [`Values`]) and referred to by [`ValueId`]. A pair the
//!   lookup doesn't adjust has the value [`NO_ADJUSTMENT`].
//! - A *glyph pair subtable* (PairPos format 1) lists pairs, each with its
//!   value. It applies only to the pairs it lists.
//! - A *class-based subtable* (PairPos format 2) groups first glyphs into
//!   classes and second glyphs into classes, and stores a value for every
//!   combination of a first class and a second class, zeros included. It
//!   applies to every pair whose first glyph it covers, so a shaper stops at
//!   it even for pairs it doesn't adjust.
//! - The *grid* ([`Grid`]) holds the classes that the class-based subtables
//!   are built from. Each *row* is a class of first glyphs, and each *column*
//!   is a class of second glyphs. The *cell* where a row and a column cross
//!   holds one value for every pair of a first glyph in the row and a second
//!   glyph in the column. A cell is *zero* if its value is
//!   [`NO_ADJUSTMENT`], and only non-zero cells are stored. A *line*
//!   ([`Line`]) is a row or a column, and is *live* if it has glyphs and at
//!   least one non-zero cell.
//! - An *exception* ([`Exception`]) is a pair whose value the class-based
//!   subtables get wrong. Exceptions go in glyph pair subtables placed before
//!   the class-based subtables, so a shaper finds them first. An exception
//!   for an unadjusted pair that a cell would adjust is an *explicit zero*.
//! - A *plan* ([`Plan`]) is a grid, the class-based subtables to build from
//!   it, and their exceptions, with an estimate of their total size.
//!
//! Sizes are estimates, in bytes, of the compiled subtables. They're used to
//! compare choices, and needn't be exact.
//!
//! # Steps
//!
//! [`optimize_pairpos`] runs these steps in order:
//!
//! 1. **Flatten** the rules into the value of each pair, following their
//!    precedence ([`FlattenedLookup::new`]). If a class-based rule adjusts
//!    the second glyph this isn't possible, and the lookup is built normally
//!    instead.
//! 2. **Make the exact grid** ([`Grid::with_exact_classes`]): group glyphs
//!    that are adjusted identically into classes. This grid's cells are right
//!    for every pair, except pairs that adjust the second glyph, which are
//!    always exceptions; but it usually has many small classes.
//! 3. **Make a plan** from the exact grid ([`Plan::new`]) with each guess in
//!    [`CELLS_PER_VALUE_GUESSES`], and keep the smallest, or on a tie, the
//!    one from the earliest guess. Making a plan has five steps:
//!    1. *Fold small classes* ([`Grid::fold_small_classes`]): fold each small
//!       class into a bigger one, so that its glyphs join that class and take
//!       on its cells, or drop it from the grid, where the exceptions that
//!       creates are guessed to cost less than its cells.
//!    2. *Group rows into subtables* ([`Grid::group_rows_into_subtables`]):
//!       start with a class-based subtable for each live row, and merge
//!       subtables while that makes them smaller in total.
//!    3. *Drop rows better as exceptions*
//!       ([`ClassSubtable::drop_rows_better_as_exceptions`]): take a row out
//!       of its subtable if its pairs cost less as exceptions.
//!    4. *Find the exceptions* to the subtables ([`find_exceptions`]).
//!    5. *Estimate the plan's size*: its exceptions ([`exceptions_size`])
//!       plus its class-based subtables.
//! 4. **Build its subtables** ([`Plan::build_subtables`]): glyph pair
//!    subtables for the exceptions, then the class-based subtables.

use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
};

use types::GlyphId16;

use super::{
    ClassPairPosSubtable, GlyphPairPosBuilder, GlyphSet, PairPosBuilder, ValueRecordBuilder,
};
use crate::tables::{
    gpos::{PairPos, ValueFormat},
    layout::builders::{Builder, Metric},
    variations::ivs_builder::VariationStoreBuilder,
};

/// An interned value: an index into [`Values::records`].
type ValueId = u32;

/// The value of a pair that isn't adjusted.
const NO_ADJUSTMENT: ValueId = 0;

/// Guesses at how many grid cells each of a line's non-zero cells costs.
///
/// A class-based subtable stores every cell of its rows, zeros included, so
/// a line costs more than its own non-zero cells: it also costs zeros where
/// the other lines in its subtable have values. How full the grids end up
/// varies from lookup to lookup, from a third to more than half, and the best
/// classes depend on it; so we make a plan with each guess, and keep the
/// smallest (see [`Grid::fold_small_classes`]).
const CELLS_PER_VALUE_GUESSES: [usize; 3] = [1, 2, 3];

/// Bytes an exception costs, besides its value record.
///
/// This is the second glyph id; the overhead of its `PairSet` and coverage
/// entry is usually shared by many pairs, and is counted separately where
/// it matters.
const EXCEPTION_OVERHEAD: usize = 2;

/// Bytes a first glyph with exceptions costs: its `PairSet`'s count and
/// offset, and a coverage entry.
const PAIR_SET_OVERHEAD: usize = 6;

/// Bytes a class-based subtable costs besides its tables and grid.
///
/// This is the subtable header, and the subtable's offset in its lookup.
const CLASS_SUBTABLE_OVERHEAD: usize = 16 + 2;

/// Build the subtables of a pair positioning lookup, optimized for size.
///
/// `builders` are the builders of one lookup, in order. A lookup with
/// explicit subtable breaks has one builder for each run of rules between
/// them (see [`LookupBuilder::force_subtable_break`]).
///
/// Unlike building each builder with [`build`], this discards the structure
/// of the rules: the subtable breaks, the classes of class-based rules, how
/// they are split into subtables, and which rules were written for glyph
/// pairs. The rules are flattened into the adjustment that applies to each
/// pair of glyphs, respecting their precedence (earlier builders before
/// later ones; within a builder, glyph pairs before class-based rules, and
/// earlier class subtables before later ones), and the subtables are then
/// rebuilt from scratch:
///
/// - glyphs that are adjusted identically are grouped into classes;
/// - a small class is folded into a bigger one when the pairs where they
///   differ cost less as glyph pair exceptions than the small class's own
///   row or column of class values;
/// - the classes of first glyphs are grouped into class-based subtables,
///   minimizing the total size of their coverage, class definitions and
///   value grids.
///
/// The glyph pair exceptions come first, followed by class-based subtables
/// with disjoint coverage. Every pair of glyphs is adjusted exactly as it
/// would be by the normal build. The result is usually smaller, but this is
/// not guaranteed, and it may have more subtables, which can make shaping
/// slower.
///
/// If a class-based rule adjusts the second glyph, each builder is built
/// normally instead: such a subtable adjusts the second glyph of every pair
/// it covers, which can't be expressed as a set of glyph pairs.
///
/// [`LookupBuilder::force_subtable_break`]: crate::tables::layout::builders::LookupBuilder::force_subtable_break
/// [`build`]: Builder::build
pub fn optimize_pairpos(
    builders: impl IntoIterator<Item = PairPosBuilder>,
    var_store: &mut VariationStoreBuilder,
) -> Vec<PairPos> {
    let builders = builders.into_iter().collect::<Vec<_>>();
    // the rules can't be flattened, so build them normally
    if class_rule_adjusts_second_glyph(&builders) {
        return builders
            .into_iter()
            .flat_map(|builder| builder.build(var_store))
            .collect();
    }

    // 1. Work out the value of each pair.
    let flat = FlattenedLookup::new(&builders);

    // 2. Group glyphs that are adjusted identically into classes.
    let exact_grid = Grid::with_exact_classes(&flat);

    // 3. Make a plan with each guess at how full the grids will be, and
    // keep the smallest.
    let best_plan = CELLS_PER_VALUE_GUESSES
        .into_iter()
        .map(|cells_per_value| Plan::new(&flat, exact_grid.clone(), cells_per_value))
        .min_by_key(|plan| plan.size)
        .expect("there is at least one guess");

    // 4. Build its subtables.
    best_plan.build_subtables(&flat, var_store)
}

/// `true` if any class-based rule adjusts the second glyph.
///
/// A class-based subtable has one value format for all of its second glyphs,
/// so if any of its rules adjusts the second glyph, it adjusts the second
/// glyph of every pair it covers, even pairs that no rule lists. That can't
/// be flattened into a value for each pair.
fn class_rule_adjusts_second_glyph(builders: &[PairPosBuilder]) -> bool {
    builders
        .iter()
        .flat_map(|builder| &builder.classes.0)
        .any(|subtable| !subtable.compute_value_formats().1.is_empty())
}

/// One candidate for a lookup's subtables: the classes, the class-based
/// subtables to build from them, and the exceptions to those subtables.
struct Plan {
    /// The classes, after folding.
    grid: Grid,
    /// The class-based subtables, each a group of the grid's rows.
    subtables: Vec<ClassSubtable>,
    /// The pairs the class-based subtables get wrong.
    exceptions: Exceptions,
    /// The estimated size of all of the subtables, in bytes.
    size: usize,
}

/// First glyph -> second glyph -> exception.
type Exceptions = BTreeMap<GlyphId16, BTreeMap<GlyphId16, Exception>>;

/// The value an exception gives its pair, in a glyph pair subtable.
#[derive(Clone, Copy, Debug)]
enum Exception {
    /// The pair's own value.
    OwnValue(ValueId),
    /// No adjustment, for a pair that a cell would otherwise adjust.
    ///
    /// The record has the fields of `overridden`, the cell's value, all set
    /// to zero.
    ExplicitZero { overridden: ValueId },
}

impl Plan {
    /// Make a plan from the exact grid (step 3).
    ///
    /// This runs once for each of the [`CELLS_PER_VALUE_GUESSES`], each time
    /// on a copy of the exact grid.
    fn new(flat: &FlattenedLookup, mut grid: Grid, cells_per_value: usize) -> Self {
        let values = &flat.values;

        // 3.1. Fold small classes into bigger ones, or drop them, where the
        // exceptions that creates cost less than their cells.
        grid.fold_small_classes(values, cells_per_value);

        // 3.2. Group the rows into class-based subtables.
        let summaries = ClassSummaries::new(&grid);
        let mut subtables = grid.group_rows_into_subtables(&summaries, values);

        // 3.3. Take out rows whose pairs cost less as exceptions, and then
        // any subtables left with no rows.
        let column_of = grid.column_of_each_second_glyph();
        for subtable in &mut subtables {
            subtable.drop_rows_better_as_exceptions(&grid, &summaries, flat, &column_of);
        }
        subtables.retain(|subtable| !subtable.rows.is_empty());

        // 3.4. Find the pairs the subtables get wrong.
        let exceptions = find_exceptions(flat, &grid, &subtables, &column_of);

        // 3.5. Estimate the size of the exceptions and the subtables.
        let size = exceptions_size(&exceptions, values)
            + subtables.iter().map(|sub| sub.size).sum::<usize>();

        Plan {
            grid,
            subtables,
            exceptions,
            size,
        }
    }

    /// Build the planned subtables (step 4).
    ///
    /// The exceptions come first, in glyph pair subtables, followed by the
    /// class-based subtables in order of their first glyph.
    fn build_subtables(
        self,
        flat: &FlattenedLookup,
        var_store: &mut VariationStoreBuilder,
    ) -> Vec<PairPos> {
        let values = &flat.values;

        // the exceptions; the builder splits them into subtables by format
        let mut exceptions = GlyphPairPosBuilder::default();
        for (g1, exceptions_of_g1) in &self.exceptions {
            let out = exceptions.0.entry(*g1).or_default();
            for (g2, exception) in exceptions_of_g1 {
                let records = match exception {
                    Exception::OwnValue(value) => values.get(*value).clone(),
                    Exception::ExplicitZero { overridden } => (
                        values.zeroed_record1(*overridden),
                        ValueRecordBuilder::new(),
                    ),
                };
                out.insert(*g2, records);
            }
        }
        let mut out = exceptions.build(var_store);

        // then a class-based subtable for each group of rows, with a rule
        // for each non-zero cell
        let mut subtables = self.subtables.iter().collect::<Vec<_>>();
        subtables.sort_by_key(|subtable| subtable.first_glyphs.first);
        for subtable in subtables {
            let mut builder = ClassPairPosSubtable::default();
            for r in &subtable.rows {
                let row = &self.grid.rows[*r];
                let class1 = row.glyphs.iter().copied().collect::<GlyphSet>();
                for (c, value) in &row.cells {
                    let class2 = self.grid.cols[*c]
                        .glyphs
                        .iter()
                        .copied()
                        .collect::<GlyphSet>();
                    let (record1, record2) = values.get(*value).clone();
                    builder.add(class1.clone(), class2, record1, record2);
                }
            }
            out.push(builder.build(var_store));
        }
        out
    }
}

/// The value of each adjusted pair in a lookup, and the values themselves.
struct FlattenedLookup {
    /// The distinct values of the lookup's rules.
    values: Values,
    /// First glyph -> second glyph -> value, for each adjusted pair.
    ///
    /// Unadjusted pairs are left out, and so are first glyphs with no
    /// adjusted pairs.
    pairs: BTreeMap<GlyphId16, BTreeMap<GlyphId16, ValueId>>,
}

impl FlattenedLookup {
    /// Flatten a lookup's rules into the value it applies to each pair (step 1).
    ///
    /// A pair's value comes from the first rule a shaper would reach: builders
    /// in order, and within each, glyph pair rules before class-based
    /// subtables. A class-based subtable claims every pair whose first glyph
    /// it covers, so no later rule applies to those pairs.
    ///
    /// No class-based rule may adjust the second glyph (see
    /// [`class_rule_adjusts_second_glyph`]).
    fn new(builders: &[PairPosBuilder]) -> Self {
        let mut values = Values::new();
        let mut pairs: BTreeMap<GlyphId16, BTreeMap<GlyphId16, ValueId>> = BTreeMap::new();
        // first glyphs covered by an earlier class-based subtable, whose
        // pairs no later rule can adjust
        let mut covered = HashSet::new();
        for builder in builders {
            // the builder's glyph pair rules; a pair's earliest value wins
            for (g1, seconds) in &builder.pairs.0 {
                if covered.contains(g1) {
                    continue;
                }
                let pairs_of_g1 = pairs.entry(*g1).or_default();
                for (g2, (record1, record2)) in seconds {
                    pairs_of_g1
                        .entry(*g2)
                        .or_insert_with(|| values.intern(record1, record2));
                }
            }
            // then its class-based subtables
            for subtable in &builder.classes.0 {
                let mut newly_covered = Vec::new();
                for (class1, seconds) in &subtable.items {
                    let seconds = seconds
                        .iter()
                        .map(|(class2, (record1, record2))| {
                            (class2, values.intern(record1, record2))
                        })
                        .collect::<Vec<_>>();
                    for g1 in class1.iter().filter(|g1| !covered.contains(g1)) {
                        newly_covered.push(g1);
                        let pairs_of_g1 = pairs.entry(g1).or_default();
                        for (class2, value) in &seconds {
                            for g2 in class2.iter() {
                                pairs_of_g1.entry(g2).or_insert(*value);
                            }
                        }
                    }
                }
                covered.extend(newly_covered);
            }
        }
        // Unadjusted pairs were only kept so that they could take precedence
        // over later rules.
        for pairs_of_g1 in pairs.values_mut() {
            pairs_of_g1.retain(|_, value| *value != NO_ADJUSTMENT);
        }
        pairs.retain(|_, pairs_of_g1| !pairs_of_g1.is_empty());
        FlattenedLookup { values, pairs }
    }

    /// The value of a pair, which is [`NO_ADJUSTMENT`] if it isn't adjusted.
    fn value_of(&self, g1: GlyphId16, g2: GlyphId16) -> ValueId {
        self.pairs
            .get(&g1)
            .and_then(|pairs_of_g1| pairs_of_g1.get(&g2))
            .copied()
            .unwrap_or(NO_ADJUSTMENT)
    }
}

/// A row or column of the [`Grid`]: a class of glyphs, and its cells.
#[derive(Clone, Debug, Default)]
struct Line {
    /// The glyphs in this class, sorted.
    glyphs: Vec<GlyphId16>,
    /// The non-zero cells: the index of the crossing line -> the cell's
    /// value.
    ///
    /// For a row, the crossing lines are columns; for a column, rows.
    cells: BTreeMap<usize, ValueId>,
}

impl Line {
    /// `true` if this line has glyphs and at least one non-zero cell.
    ///
    /// A line that isn't live adds nothing to a class-based subtable. This
    /// includes lines that have been folded away or dropped, which are left
    /// empty.
    fn is_live(&self) -> bool {
        !self.glyphs.is_empty() && !self.cells.is_empty()
    }
}

/// The classes that the class-based subtables are built from: rows of first
/// glyphs, columns of second glyphs, and the value of each cell.
///
/// Each cell is stored in both its row and its column. Folding a line into
/// another, or dropping it, leaves an empty line in its place, so indices
/// stay stable.
///
/// The grid doesn't record which pairs its cells get wrong: those
/// exceptions are found by comparing with the flattened pairs, once the
/// subtables are chosen (see [`find_exceptions`]).
#[derive(Clone, Debug)]
struct Grid {
    rows: Vec<Line>,
    cols: Vec<Line>,
}

impl Grid {
    /// Make the grid whose classes are exact (step 2).
    ///
    /// Two second glyphs share a column only if every first glyph gives them
    /// the same value, and two first glyphs share a row only if they give the
    /// same value to every column. So every cell is right for every pair it
    /// covers. Pairs that adjust the second glyph are left out of the grid,
    /// because they can only be exceptions (see [`Values::adjusts_second`]).
    ///
    /// Lines are numbered in order of their lowest glyph.
    fn with_exact_classes(flat: &FlattenedLookup) -> Grid {
        let values = &flat.values;

        // each second glyph's pairs, as (first glyph, value)
        let mut pairs_by_g2: BTreeMap<GlyphId16, Vec<(GlyphId16, ValueId)>> = BTreeMap::new();
        for (g1, pairs_of_g1) in &flat.pairs {
            for (g2, value) in pairs_of_g1 {
                if !values.adjusts_second(*value) {
                    pairs_by_g2.entry(*g2).or_default().push((*g1, *value));
                }
            }
        }

        // second glyphs with identical pairs share a column
        let mut cols: Vec<Line> = Vec::new();
        let mut col_with_pairs = BTreeMap::new();
        let mut col_of = HashMap::new();
        for (g2, pairs_of_g2) in pairs_by_g2 {
            let next = col_with_pairs.len();
            let c = *col_with_pairs.entry(pairs_of_g2).or_insert(next);
            if c == cols.len() {
                cols.push(Line::default());
            }
            cols[c].glyphs.push(g2);
            col_of.insert(g2, c);
        }

        // first glyphs with identical cells share a row
        let mut rows: Vec<Line> = Vec::new();
        let mut row_with_cells = BTreeMap::new();
        for (g1, pairs_of_g1) in &flat.pairs {
            let mut cells = pairs_of_g1
                .iter()
                .filter(|(_, value)| !values.adjusts_second(**value))
                .map(|(g2, value)| (col_of[g2], *value))
                .collect::<Vec<_>>();
            // every glyph in a column has the same value with this first
            // glyph, so this leaves one cell per column
            cells.sort_unstable();
            cells.dedup();
            if cells.is_empty() {
                continue;
            }
            let next = row_with_cells.len();
            let r = *row_with_cells.entry(cells.clone()).or_insert(next);
            if r == rows.len() {
                rows.push(Line {
                    glyphs: Vec::new(),
                    cells: cells.into_iter().collect(),
                });
            }
            rows[r].glyphs.push(*g1);
        }

        // and each cell is stored in its column too
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in &row.cells {
                cols[*c].cells.insert(r, *value);
            }
        }
        Grid { rows, cols }
    }

    /// The index of the column of each second glyph in the grid.
    fn column_of_each_second_glyph(&self) -> HashMap<GlyphId16, usize> {
        self.cols
            .iter()
            .enumerate()
            .flat_map(|(c, col)| col.glyphs.iter().map(move |g2| (*g2, c)))
            .collect()
    }

    /// Swap the rows and the columns.
    fn transpose(&mut self) {
        std::mem::swap(&mut self.rows, &mut self.cols);
    }
}

/// Step 3.1: folding small classes.
impl Grid {
    /// Fold small classes into bigger ones, or drop them, until that stops
    /// saving bytes (step 3.1).
    ///
    /// This runs first when making a plan, on a copy of the exact grid.
    ///
    /// A class folded into another joins it, and takes on its cells; pairs
    /// where the two classes' cells differ become exceptions. A dropped class
    /// leaves the grid, and all of its pairs become exceptions.
    ///
    /// The bytes a line costs in the class-based subtables are guessed as
    /// `cells_per_value` grid cells for each of its non-zero cells.
    fn fold_small_classes(&mut self, values: &Values, cells_per_value: usize) {
        // Fold columns, then rows, and repeat until nothing changes. Rows
        // are folded as the columns of the transposed grid.
        loop {
            let mut changed = self.fold_columns(values, cells_per_value);
            self.transpose();
            changed |= self.fold_columns(values, cells_per_value);
            self.transpose();
            if !changed {
                break;
            }
        }
    }

    /// Fold or drop columns where that saves bytes; returns `true` if any
    /// column was removed.
    ///
    /// Fewer columns make a smaller grid, but any pair the grid no longer
    /// gets right becomes an exception. There are two ways to remove a
    /// column:
    ///
    /// - Fold it into a bigger column that agrees with it in at least one
    ///   row: only the pairs where the two disagree become exceptions.
    /// - Drop it: all of its pairs become exceptions.
    ///
    /// Each of a column's non-zero cells is guessed to cost `cells_per_value`
    /// cells (see [`CELLS_PER_VALUE_GUESSES`]). Every column is priced
    /// against the grid as it stands, and then the removals are made,
    /// biggest saving first. The caller repeats this until nothing changes,
    /// and folds rows by calling this on the transposed grid.
    fn fold_columns(&mut self, values: &Values, cells_per_value: usize) -> bool {
        // (row, value) -> the columns with that non-zero value in that row
        let mut columns_with_cell: HashMap<(usize, ValueId), Vec<usize>> = HashMap::new();
        for (c, col) in self.cols.iter().enumerate() {
            for (r, value) in &col.cells {
                columns_with_cell.entry((*r, *value)).or_default().push(c);
            }
        }
        // a column with more glyphs is bigger; on a tie, the earlier one is
        let is_bigger = |a: usize, b: usize| {
            let (len_a, len_b) = (self.cols[a].glyphs.len(), self.cols[b].glyphs.len());
            len_a > len_b || (len_a == len_b && a < b)
        };

        // Price the best removal of each live column, if any saves bytes.
        let mut removals = Vec::new();
        for (c, col) in self.cols.iter().enumerate() {
            if !col.is_live() {
                continue;
            }
            // what the column's cells are guessed to cost in the subtables
            let cells_cost = col
                .cells
                .values()
                .map(|value| values.record1_size(*value) * cells_per_value)
                .sum::<usize>();
            // dropping it makes all of its pairs exceptions
            let drop_cost = col
                .cells
                .iter()
                .map(|(r, value)| {
                    self.rows[*r].glyphs.len()
                        * col.glyphs.len()
                        * (EXCEPTION_OVERHEAD + values.record1_size(*value))
                })
                .sum::<usize>();
            // (exceptions cost, column folded into, or None to drop it)
            let mut best = (drop_cost < cells_cost).then_some((drop_cost, None));
            // folding it into a bigger column that shares a cell with it
            // makes only the pairs where they differ exceptions
            let targets = col
                .cells
                .iter()
                .flat_map(|(r, value)| &columns_with_cell[&(*r, *value)])
                .copied()
                .filter(|target| *target != c && is_bigger(*target, c))
                .collect::<BTreeSet<_>>();
            for target in targets {
                let limit = best.map_or(cells_cost, |(best_cost, _)| best_cost);
                if let Some(cost) = self.cost_of_folding_column(c, target, values, limit) {
                    best = Some((cost, Some(target)));
                }
            }
            if let Some((cost, fold_into)) = best {
                removals.push(ColumnRemoval {
                    saving: cells_cost - cost,
                    column: c,
                    fold_into,
                });
            }
        }

        // Make the removals, biggest saving first, skipping those whose
        // prices are out of date because of an earlier removal.
        removals.sort_by_key(|removal| (Reverse(removal.saving), removal.column));
        let mut removed = HashSet::new();
        let mut targets = HashSet::new();
        for removal in &removals {
            // a column that has taken another was priced with fewer glyphs
            if targets.contains(&removal.column) {
                continue;
            }
            if let Some(target) = removal.fold_into {
                // a removed column can't take others
                if removed.contains(&target) {
                    continue;
                }
                targets.insert(target);
            }
            removed.insert(removal.column);
            self.remove_column(removal.column, removal.fold_into);
        }
        !removed.is_empty()
    }

    /// The estimated size, in bytes, of the exceptions made by folding
    /// `column` into `target`.
    ///
    /// Each row where the two columns' cells differ makes an exception of
    /// each pair of a glyph in the row and a glyph in `column`.
    ///
    /// Returns `None` if the cost isn't less than `limit`.
    fn cost_of_folding_column(
        &self,
        column: usize,
        target: usize,
        values: &Values,
        limit: usize,
    ) -> Option<usize> {
        let mut target_cells = self.cols[target].cells.iter().peekable();
        let mut column_cells = self.cols[column].cells.iter().peekable();
        let column_len = self.cols[column].glyphs.len();
        let mut cost = 0;
        // walk both columns' cells in row order, a missing cell being zero
        loop {
            let (r, target_value, column_value) = match (target_cells.peek(), column_cells.peek()) {
                (None, None) => break,
                (Some(&(&rt, &vt)), Some(&(&rc, &vc))) => match rt.cmp(&rc) {
                    Ordering::Equal => {
                        target_cells.next();
                        column_cells.next();
                        (rt, vt, vc)
                    }
                    Ordering::Less => {
                        target_cells.next();
                        (rt, vt, NO_ADJUSTMENT)
                    }
                    Ordering::Greater => {
                        column_cells.next();
                        (rc, NO_ADJUSTMENT, vc)
                    }
                },
                (Some(&(&rt, &vt)), None) => {
                    target_cells.next();
                    (rt, vt, NO_ADJUSTMENT)
                }
                (None, Some(&(&rc, &vc))) => {
                    column_cells.next();
                    (rc, NO_ADJUSTMENT, vc)
                }
            };
            if target_value != column_value {
                // each exception is priced with the bigger of the two records
                let size = values
                    .record1_size(target_value)
                    .max(values.record1_size(column_value));
                cost += self.rows[r].glyphs.len() * column_len * (EXCEPTION_OVERHEAD + size);
                if cost >= limit {
                    return None;
                }
            }
        }
        Some(cost)
    }

    /// Remove `column` from the grid, folding it into `fold_into` if that's
    /// `Some`, or dropping it if it's `None`.
    ///
    /// The column is left empty, and its cells are removed from their rows.
    /// A folded column's glyphs join the target column.
    fn remove_column(&mut self, column: usize, fold_into: Option<usize>) {
        let removed = std::mem::take(&mut self.cols[column]);
        for r in removed.cells.keys() {
            self.rows[*r].cells.remove(&column);
        }
        if let Some(target) = fold_into {
            let glyphs = &mut self.cols[target].glyphs;
            glyphs.extend(removed.glyphs);
            glyphs.sort_unstable();
        }
    }
}

/// A way to remove a column from the grid, priced by [`Grid::fold_columns`].
struct ColumnRemoval {
    /// The bytes the removal is guessed to save.
    saving: usize,
    /// The column to remove.
    column: usize,
    /// The column to fold it into, or `None` to drop it.
    fold_into: Option<usize>,
}

/// Step 3.2: grouping rows into class-based subtables.
impl Grid {
    /// Group the live rows into class-based subtables, minimizing their total
    /// estimated size (step 3.2).
    ///
    /// This runs once the classes are folded, and before rows better as
    /// exceptions are dropped.
    ///
    /// It starts with a subtable for each live row, and repeatedly merges
    /// the two subtables whose merge saves the most bytes, until no merge
    /// saves any.
    fn group_rows_into_subtables(
        &self,
        summaries: &ClassSummaries,
        values: &Values,
    ) -> Vec<ClassSubtable> {
        // A subtable for each live row. When subtable `i` is merged with
        // `j`, the result takes slot `i`, and slot `j` is left `None`.
        let mut subtables = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_live())
            .map(|(r, _)| Some(ClassSubtable::from_rows(self, summaries, values, [r])))
            .collect::<Vec<_>>();

        // The bytes saved by merging subtables `i` and `j`. A merge saves a
        // subtable's overhead, and stores the columns both use once; but each
        // row gains cells for the other's columns, and every cell takes the
        // union of both value formats.
        let saving = |subtables: &[Option<ClassSubtable>], i: usize, j: usize| -> i64 {
            let (a, b) = (
                subtables[i].as_ref().unwrap(),
                subtables[j].as_ref().unwrap(),
            );
            (a.size + b.size) as i64 - a.estimated_size_if_merged(b, summaries) as i64
        };
        // (saving, j) for the subtable `j` whose merge with `i` saves most,
        // the lowest `j` on a tie
        let best_partner = |subtables: &[Option<ClassSubtable>], i: usize| {
            (0..subtables.len())
                .filter(|j| *j != i && subtables[*j].is_some())
                .map(|j| (saving(subtables, i, j), j))
                .max_by_key(|(saving, j)| (*saving, Reverse(*j)))
        };
        // each subtable's best partner, kept up to date as subtables merge
        let mut best_partner_of = (0..subtables.len())
            .map(|i| best_partner(&subtables, i))
            .collect::<Vec<_>>();

        // Merge the pair that saves the most, the lowest `i` on a tie, until
        // no merge saves anything.
        while let Some((i, (merge_saving, j))) = best_partner_of
            .iter()
            .enumerate()
            .filter_map(|(i, best)| best.map(|best| (i, best)))
            .max_by_key(|(i, (saving, _))| (*saving, Reverse(*i)))
        {
            if merge_saving <= 0 {
                break;
            }
            let merged = subtables[i].take().unwrap().merge(
                subtables[j].take().unwrap(),
                self,
                summaries,
                values,
            );
            subtables[i] = Some(merged);
            best_partner_of[j] = None;
            best_partner_of[i] = best_partner(&subtables, i);
            for k in 0..subtables.len() {
                if k == i || subtables[k].is_none() {
                    continue;
                }
                match best_partner_of[k] {
                    // k's best partner changed or is gone: search again
                    Some((_, partner)) if partner == i || partner == j => {
                        best_partner_of[k] = best_partner(&subtables, k);
                    }
                    // otherwise only the merged subtable might beat it
                    current => {
                        let with_i = saving(&subtables, k, i);
                        if current.is_none_or(|(saving, _)| with_i > saving) {
                            best_partner_of[k] = Some((with_i, i));
                        }
                    }
                }
            }
        }
        subtables.into_iter().flatten().collect()
    }
}

/// A planned class-based subtable: a group of the grid's rows, and what's
/// needed to estimate its size.
#[derive(Clone, Debug)]
struct ClassSubtable {
    /// The indices of its rows in the grid.
    rows: Vec<usize>,
    /// The columns with a non-zero cell in at least one of its rows.
    used_columns: ColumnSet,
    /// The number of columns in `used_columns`.
    n_used_columns: usize,
    /// The glyphs of all of its rows: its coverage.
    first_glyphs: GlyphSetSummary,
    /// The glyphs of its biggest row, which is class 0 and so isn't in the
    /// first class definition.
    biggest_row: GlyphSetSummary,
    /// The glyphs of all of its used columns.
    second_glyphs: GlyphSetSummary,
    /// The union of the formats of its cells' first value records.
    value_format: ValueFormat,
    /// Its estimated size in bytes (see [`estimate_class_subtable_size`]).
    size: usize,
}

impl ClassSubtable {
    /// Make a subtable of these rows of the grid.
    fn from_rows(
        grid: &Grid,
        summaries: &ClassSummaries,
        values: &Values,
        rows: impl IntoIterator<Item = usize>,
    ) -> Self {
        let rows = rows.into_iter().collect::<Vec<_>>();
        let mut used_columns = ColumnSet::new(grid.cols.len());
        let mut first_glyphs = GlyphSetSummary::default();
        let mut biggest_row = GlyphSetSummary::default();
        let mut value_format = ValueFormat::empty();
        for r in &rows {
            let row = summaries.rows[*r];
            first_glyphs = first_glyphs.union(row);
            if row.count > biggest_row.count {
                biggest_row = row;
            }
            for (c, value) in &grid.rows[*r].cells {
                used_columns.insert(*c);
                value_format |= values.record1_format(*value);
            }
        }
        let second_glyphs = used_columns
            .iter()
            .fold(GlyphSetSummary::default(), |glyphs, c| {
                glyphs.union(summaries.cols[c])
            });
        let n_used_columns = used_columns.count();
        let mut subtable = ClassSubtable {
            rows,
            used_columns,
            n_used_columns,
            first_glyphs,
            biggest_row,
            second_glyphs,
            value_format,
            size: 0,
        };
        subtable.size = subtable.estimate_size();
        subtable
    }

    /// Estimate this subtable's size, in bytes.
    fn estimate_size(&self) -> usize {
        estimate_class_subtable_size(
            self.rows.len(),
            self.n_used_columns,
            self.first_glyphs,
            self.biggest_row,
            self.second_glyphs,
            self.value_format,
        )
    }

    /// The estimated size of the merge of two subtables, without merging
    /// them.
    fn estimated_size_if_merged(&self, other: &ClassSubtable, summaries: &ClassSummaries) -> usize {
        // the columns both use are only counted once
        let mut second_glyphs = self.second_glyphs.union(other.second_glyphs);
        let mut n_used_columns = self.n_used_columns + other.n_used_columns;
        for c in self.used_columns.intersection(&other.used_columns).iter() {
            second_glyphs = second_glyphs.without(summaries.cols[c]);
            n_used_columns -= 1;
        }
        let biggest_row = if other.biggest_row.count > self.biggest_row.count {
            other.biggest_row
        } else {
            self.biggest_row
        };
        estimate_class_subtable_size(
            self.rows.len() + other.rows.len(),
            n_used_columns,
            self.first_glyphs.union(other.first_glyphs),
            biggest_row,
            second_glyphs,
            self.value_format | other.value_format,
        )
    }

    /// Merge two subtables: `self`'s rows, then `other`'s.
    fn merge(
        self,
        other: ClassSubtable,
        grid: &Grid,
        summaries: &ClassSummaries,
        values: &Values,
    ) -> ClassSubtable {
        let rows = self.rows.into_iter().chain(other.rows);
        ClassSubtable::from_rows(grid, summaries, values, rows)
    }
}

/// Step 3.3: dropping rows better as exceptions.
impl ClassSubtable {
    /// Take out the rows whose pairs cost less as exceptions than in this
    /// subtable (step 3.3).
    ///
    /// This runs once the rows are grouped into subtables, and before the
    /// exceptions are found. Each row in turn is priced both ways, against
    /// the subtable as it stands. A row that is taken out is in no subtable,
    /// so all of its glyphs' pairs become exceptions (see [`find_exceptions`]).
    ///
    /// `column_of` is the column of each second glyph in the grid.
    fn drop_rows_better_as_exceptions(
        &mut self,
        grid: &Grid,
        summaries: &ClassSummaries,
        flat: &FlattenedLookup,
        column_of: &HashMap<GlyphId16, usize>,
    ) {
        let values = &flat.values;
        let mut i = 0;
        while i < self.rows.len() {
            let r = self.rows[i];
            let row = &grid.rows[r];
            // the exceptions the row's first glyphs need either way
            let (mut if_dropped, mut if_kept) = (0, 0);
            for g1 in &row.glyphs {
                let (g1_if_dropped, g1_if_kept) =
                    exception_costs_of_first_glyph(*g1, row, grid, flat, column_of);
                if_dropped += g1_if_dropped;
                if_kept += g1_if_kept;
            }
            // how much the row adds to this subtable's estimated size, as
            // it stands now; a subtable with no rows isn't built at all
            let others = self.rows.iter().copied().filter(|other| *other != r);
            let without_row = ClassSubtable::from_rows(grid, summaries, values, others);
            let row_size = self.size
                - if without_row.rows.is_empty() {
                    0
                } else {
                    without_row.size
                };
            // take the row out if its pairs cost less as exceptions than it
            // and its exceptions cost if it stays
            if if_dropped < row_size + if_kept {
                // the next row moves to index `i`
                *self = without_row;
            } else {
                i += 1;
            }
        }
    }
}

/// The estimated size, in bytes, of the exceptions a first glyph needs if
/// its row is dropped, and if it's kept.
///
/// This prices a row for [`ClassSubtable::drop_rows_better_as_exceptions`].
///
/// If the row is dropped, each of the glyph's adjusted pairs is an
/// exception. If it's kept, the exceptions are the pairs the row's cells get
/// wrong: adjusted pairs whose value isn't their cell's, including all pairs
/// that adjust the second glyph, and explicit zeros for unadjusted pairs
/// whose cell is non-zero. A glyph with any exceptions also costs
/// [`PAIR_SET_OVERHEAD`].
fn exception_costs_of_first_glyph(
    g1: GlyphId16,
    row: &Line,
    grid: &Grid,
    flat: &FlattenedLookup,
    column_of: &HashMap<GlyphId16, usize>,
) -> (usize, usize) {
    let values = &flat.values;
    let cost = |value| EXCEPTION_OVERHEAD + values.record1_size(value);
    let (mut if_dropped, mut if_kept) = (0, 0);
    // the glyph's adjusted pairs
    for (g2, value) in flat.pairs.get(&g1).into_iter().flatten() {
        if_dropped += cost(*value);
        let cell = column_of.get(g2).and_then(|c| row.cells.get(c));
        if values.adjusts_second(*value) || cell != Some(value) {
            if_kept += cost(*value);
        }
    }
    // its unadjusted pairs that the row's cells would adjust
    for (c, value) in &row.cells {
        let unadjusted = grid.cols[*c]
            .glyphs
            .iter()
            .filter(|g2| flat.value_of(g1, **g2) == NO_ADJUSTMENT)
            .count();
        if_kept += unadjusted * cost(*value);
    }
    let with_pair_set = |cost: usize| {
        if cost > 0 {
            cost + PAIR_SET_OVERHEAD
        } else {
            0
        }
    };
    (with_pair_set(if_dropped), with_pair_set(if_kept))
}

/// Find the pairs that the class-based subtables get wrong (step 3.4).
///
/// This runs once the subtables are final. The exceptions are:
///
/// - each adjusted pair whose value isn't its cell's, including all of the
///   pairs of first glyphs in no subtable, and all pairs that adjust the
///   second glyph;
/// - an explicit zero for each unadjusted pair that a non-zero cell would
///   adjust.
///
/// `column_of` is the column of each second glyph in the grid.
fn find_exceptions(
    flat: &FlattenedLookup,
    grid: &Grid,
    subtables: &[ClassSubtable],
    column_of: &HashMap<GlyphId16, usize>,
) -> Exceptions {
    let values = &flat.values;
    // the row of each first glyph in a subtable
    let mut row_of = HashMap::new();
    for subtable in subtables {
        for r in &subtable.rows {
            row_of.extend(grid.rows[*r].glyphs.iter().map(|g1| (*g1, *r)));
        }
    }
    // the value the subtables give a pair: its cell's, or none if either
    // glyph isn't in the subtables' classes
    let cell_value = |g1: &GlyphId16, g2: &GlyphId16| {
        row_of
            .get(g1)
            .zip(column_of.get(g2))
            .and_then(|(r, c)| grid.rows[*r].cells.get(c))
            .copied()
            .unwrap_or(NO_ADJUSTMENT)
    };

    let mut exceptions = Exceptions::new();
    // adjusted pairs the subtables get wrong
    for (g1, pairs_of_g1) in &flat.pairs {
        for (g2, value) in pairs_of_g1 {
            if values.adjusts_second(*value) || cell_value(g1, g2) != *value {
                let exception = Exception::OwnValue(*value);
                exceptions.entry(*g1).or_default().insert(*g2, exception);
            }
        }
    }
    // unadjusted pairs the subtables would adjust
    for subtable in subtables {
        for r in &subtable.rows {
            let row = &grid.rows[*r];
            for (c, value) in &row.cells {
                for g1 in &row.glyphs {
                    for g2 in &grid.cols[*c].glyphs {
                        if flat.value_of(*g1, *g2) == NO_ADJUSTMENT {
                            let exception = Exception::ExplicitZero { overridden: *value };
                            exceptions.entry(*g1).or_default().insert(*g2, exception);
                        }
                    }
                }
            }
        }
    }
    exceptions
}

/// The estimated size of a plan's exceptions, in bytes (step 3.5).
///
/// Each exception costs [`EXCEPTION_OVERHEAD`] and the first record of its
/// value, or of the value it overrides, and each first glyph with exceptions
/// costs [`PAIR_SET_OVERHEAD`].
fn exceptions_size(exceptions: &Exceptions, values: &Values) -> usize {
    exceptions
        .values()
        .map(|exceptions_of_g1| {
            let records = exceptions_of_g1.values().map(|exception| match exception {
                Exception::OwnValue(value) | Exception::ExplicitZero { overridden: value } => {
                    EXCEPTION_OVERHEAD + values.record1_size(*value)
                }
            });
            PAIR_SET_OVERHEAD + records.sum::<usize>()
        })
        .sum()
}

/// The distinct values in a lookup, each with a [`ValueId`].
struct Values {
    /// The values, indexed by id.
    records: Vec<(ValueRecordBuilder, ValueRecordBuilder)>,
    /// The id of each value.
    ids: BTreeMap<(ValueRecordBuilder, ValueRecordBuilder), ValueId>,
}

impl Values {
    /// A set with only [`NO_ADJUSTMENT`] in it.
    fn new() -> Self {
        let none = (ValueRecordBuilder::new(), ValueRecordBuilder::new());
        Values {
            records: vec![none.clone()],
            ids: BTreeMap::from([(none, NO_ADJUSTMENT)]),
        }
    }

    /// Intern a value, so that values with the same effect have the same id.
    fn intern(&mut self, record1: &ValueRecordBuilder, record2: &ValueRecordBuilder) -> ValueId {
        // A zero field in the first record has no effect. Any record for the
        // second glyph does, even if it is all zeros: the second glyph is
        // consumed, so it can't also be the first glyph of the next pair.
        let key = (record1.clone().clear_zeros(), record2.clone());
        if let Some(id) = self.ids.get(&key) {
            return *id;
        }
        let id = self.records.len() as ValueId;
        self.records.push(key.clone());
        self.ids.insert(key, id);
        id
    }

    /// The value with this id: its records for the first and second glyphs.
    fn get(&self, id: ValueId) -> &(ValueRecordBuilder, ValueRecordBuilder) {
        &self.records[id as usize]
    }

    /// `true` if this value applies to the second glyph.
    ///
    /// These can only be exceptions: the second value format is shared by
    /// every pair in a class-based subtable.
    fn adjusts_second(&self, id: ValueId) -> bool {
        !self.get(id).1.format().is_empty()
    }

    /// The format of this value's first record.
    fn record1_format(&self, id: ValueId) -> ValueFormat {
        self.get(id).0.format()
    }

    /// The size of this value's first record, in bytes.
    fn record1_size(&self, id: ValueId) -> usize {
        self.record1_format(id).record_byte_len()
    }

    /// A record with the same fields as this value's first record, all zero.
    fn zeroed_record1(&self, id: ValueId) -> ValueRecordBuilder {
        let zero = |metric: &Option<Metric>| metric.as_ref().map(|_| Metric::default());
        let record = &self.get(id).0;
        ValueRecordBuilder {
            x_advance: zero(&record.x_advance),
            y_advance: zero(&record.y_advance),
            x_placement: zero(&record.x_placement),
            y_placement: zero(&record.y_placement),
        }
    }
}

/// A summary of a set of glyphs, enough to estimate the size of a coverage
/// or class definition table listing them.
#[derive(Clone, Copy, Debug, Default)]
struct GlyphSetSummary {
    /// The number of glyphs.
    count: usize,
    /// The number of runs of consecutive glyph ids.
    ranges: usize,
    /// The lowest glyph id.
    first: u16,
    /// The highest glyph id.
    last: u16,
}

impl GlyphSetSummary {
    /// Summarize a set of glyphs; `glyphs` must be sorted.
    fn new(glyphs: &[GlyphId16]) -> Self {
        let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) else {
            return GlyphSetSummary::default();
        };
        let ranges = 1 + glyphs
            .windows(2)
            .filter(|pair| pair[1].to_u16() != pair[0].to_u16() + 1)
            .count();
        GlyphSetSummary {
            count: glyphs.len(),
            ranges,
            first: first.to_u16(),
            last: last.to_u16(),
        }
    }

    /// The union of two disjoint sets.
    ///
    /// Runs that become adjacent aren't merged, so `ranges` may overestimate.
    fn union(self, other: GlyphSetSummary) -> GlyphSetSummary {
        if self.count == 0 {
            return other;
        }
        if other.count == 0 {
            return self;
        }
        GlyphSetSummary {
            count: self.count + other.count,
            ranges: self.ranges + other.ranges,
            first: self.first.min(other.first),
            last: self.last.max(other.last),
        }
    }

    /// Remove a subset; `first` and `last` are left as they were.
    fn without(self, other: GlyphSetSummary) -> GlyphSetSummary {
        GlyphSetSummary {
            count: self.count - other.count,
            ranges: self.ranges.saturating_sub(other.ranges),
            ..self
        }
    }

    fn coverage_size(self) -> usize {
        4 + (2 * self.count).min(6 * self.ranges)
    }

    /// The size of a class definition table, for disjoint classes.
    ///
    /// Class ranges can't span classes, so `ranges` is exact here.
    fn class_def_size(self) -> usize {
        if self.count == 0 {
            return 4;
        }
        let format1 = 6 + 2 * (self.last - self.first) as usize + 2;
        let format2 = 4 + 6 * self.ranges;
        format1.min(format2)
    }
}

/// A [`GlyphSetSummary`] of each row and column of a [`Grid`].
struct ClassSummaries {
    rows: Vec<GlyphSetSummary>,
    cols: Vec<GlyphSetSummary>,
}

impl ClassSummaries {
    fn new(grid: &Grid) -> Self {
        let summarize = |lines: &[Line]| {
            lines
                .iter()
                .map(|line| GlyphSetSummary::new(&line.glyphs))
                .collect()
        };
        ClassSummaries {
            rows: summarize(&grid.rows),
            cols: summarize(&grid.cols),
        }
    }
}

/// A set of column indices, as a bitset.
#[derive(Clone, Debug)]
struct ColumnSet(Vec<u64>);

impl ColumnSet {
    /// An empty set, for a grid with `n_cols` columns.
    fn new(n_cols: usize) -> Self {
        ColumnSet(vec![0; n_cols.div_ceil(64)])
    }

    fn insert(&mut self, c: usize) {
        self.0[c / 64] |= 1 << (c % 64);
    }

    /// The number of columns in the set.
    fn count(&self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }

    /// The columns in both sets.
    fn intersection(&self, other: &ColumnSet) -> ColumnSet {
        ColumnSet(self.0.iter().zip(&other.0).map(|(a, b)| a & b).collect())
    }

    /// The columns in the set, in increasing order.
    fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.0.iter().enumerate().flat_map(|(i, word)| {
            let mut word = *word;
            std::iter::from_fn(move || {
                if word == 0 {
                    return None;
                }
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                Some(i * 64 + bit)
            })
        })
    }
}

/// Estimate the size of a class-based subtable from its glyphs, in bytes.
///
/// This is the subtable's header, its coverage, its two class definitions,
/// and its grid: in each row, a record for each used column, and one for
/// class 0, the second glyphs in no column.
fn estimate_class_subtable_size(
    n_rows: usize,
    n_used_columns: usize,
    first_glyphs: GlyphSetSummary,
    biggest_row: GlyphSetSummary,
    second_glyphs: GlyphSetSummary,
    value_format: ValueFormat,
) -> usize {
    CLASS_SUBTABLE_OVERHEAD
        + first_glyphs.coverage_size()
        + first_glyphs.without(biggest_row).class_def_size()
        + second_glyphs.class_def_size()
        + n_rows * (n_used_columns + 1) * value_format.record_byte_len()
}

#[cfg(test)]
mod tests {
    use font_types::GlyphId;
    use read_fonts::{tables::gpos as rgpos, FontData, FontRead};

    use super::*;

    fn gid(raw: u16) -> GlyphId16 {
        GlyphId16::new(raw)
    }

    fn glyphs(raw: impl IntoIterator<Item = u16>) -> GlyphSet {
        raw.into_iter().map(GlyphId16::new).collect()
    }

    fn advance(val: i16) -> ValueRecordBuilder {
        ValueRecordBuilder::new().with_x_advance(val)
    }

    fn none() -> ValueRecordBuilder {
        ValueRecordBuilder::new()
    }

    /// What a lookup does to a pair: the first glyph's placement and advance,
    /// and whether the second glyph is adjusted (and so consumed).
    type Effect = ([i16; 4], bool);

    struct Compiled(Vec<Vec<u8>>);

    impl Compiled {
        fn new(subtables: &[PairPos]) -> Self {
            Compiled(
                subtables
                    .iter()
                    .map(|subtable| crate::dump_table(subtable).unwrap())
                    .collect(),
            )
        }

        /// Apply the lookup to a pair, as a shaper would.
        fn apply(&self, g1: GlyphId16, g2: GlyphId16) -> Effect {
            let effect = |record: &rgpos::ValueRecord, adjusts_second| {
                let fields = [
                    record.x_placement(),
                    record.y_placement(),
                    record.x_advance(),
                    record.y_advance(),
                ];
                (fields.map(|field| field.unwrap_or(0)), adjusts_second)
            };
            for bytes in &self.0 {
                match rgpos::PairPos::read(FontData::new(bytes)).unwrap() {
                    rgpos::PairPos::Format1(table) => {
                        let Some(idx) = table.coverage().unwrap().get(g1) else {
                            continue;
                        };
                        let set = table.pair_sets().get(idx as usize).unwrap();
                        let record = set
                            .pair_value_records()
                            .iter()
                            .map(|record| record.unwrap())
                            .find(|record| record.second_glyph() == g2);
                        if let Some(record) = record {
                            let adjusts_second = !table.value_format2().is_empty();
                            return effect(record.value_record1(), adjusts_second);
                        }
                    }
                    rgpos::PairPos::Format2(table) => {
                        if table.coverage().unwrap().get(g1).is_none() {
                            continue;
                        }
                        let class1 = table.class_def1().unwrap().get(GlyphId::from(g1));
                        let class2 = table.class_def2().unwrap().get(GlyphId::from(g2));
                        let row = table.class1_records().get(class1 as usize).unwrap();
                        let record = row.class2_records().get(class2 as usize).unwrap();
                        let adjusts_second = !table.value_format2().is_empty();
                        return effect(record.value_record1(), adjusts_second);
                    }
                    _ => unreachable!("we only build formats 1 and 2"),
                }
            }
            ([0; 4], false)
        }
    }

    /// Build a lookup's builders both ways, and check every pair of glyphs is
    /// adjusted the same.
    ///
    /// Each boundary between `builders` is an explicit subtable break.
    fn assert_equivalent(builders: Vec<PairPosBuilder>, n_glyphs: u16) -> (usize, usize) {
        let mut var_store = VariationStoreBuilder::new(0);
        let normal = builders
            .clone()
            .into_iter()
            .flat_map(|builder| builder.build(&mut var_store))
            .collect::<Vec<_>>();
        let optimized = optimize_pairpos(builders, &mut var_store);
        let (normal, optimized) = (Compiled::new(&normal), Compiled::new(&optimized));
        for g1 in (0..n_glyphs).map(gid) {
            for g2 in (0..n_glyphs).map(gid) {
                assert_eq!(
                    normal.apply(g1, g2),
                    optimized.apply(g1, g2),
                    "pair ({g1}, {g2})"
                );
            }
        }
        let size = |compiled: &Compiled| compiled.0.iter().map(Vec::len).sum();
        (size(&normal), size(&optimized))
    }

    #[test]
    fn precedence_is_preserved() {
        // A glyph pair beats the class pair that follows it in its builder.
        let mut first = PairPosBuilder::default();
        first.insert_classes(glyphs([1, 2]), advance(-10), glyphs([5, 6]), none());
        first.insert_pair(gid(1), advance(-20), gid(5), none());
        // An explicit zero beats a later class pair.
        first.insert_pair(gid(3), advance(0), gid(5), none());
        // Glyphs 1 and 2 are covered by a class subtable in the first
        // builder, so nothing here applies to them, even for a second glyph
        // that subtable doesn't list; glyphs 3 and 4 aren't, so their class
        // pair applies.
        let mut second = PairPosBuilder::default();
        second.insert_pair(gid(1), advance(30), gid(7), none());
        second.insert_classes(glyphs([2, 3, 4]), advance(-40), glyphs([5, 6, 7]), none());
        assert_equivalent(vec![first, second], 10);
    }

    #[test]
    fn second_glyph_adjustments_are_exceptions() {
        let mut builder = PairPosBuilder::default();
        builder.insert_pair(gid(1), advance(-10), gid(2), advance(0));
        builder.insert_pair(gid(1), advance(-10), gid(3), none());
        builder.insert_classes(glyphs([1, 4]), advance(-20), glyphs([2, 3, 5]), none());
        assert_equivalent(vec![builder], 8);
    }

    #[test]
    fn class_rules_adjusting_second_glyph_fall_back() {
        let mut builder = PairPosBuilder::default();
        builder.insert_classes(glyphs([1, 2]), advance(-10), glyphs([3]), advance(5));
        builder.insert_pair(gid(4), advance(-20), gid(3), none());
        let mut var_store = VariationStoreBuilder::new(0);
        let normal = builder.clone().build(&mut var_store);
        let optimized = builder.build_optimized(&mut var_store);
        assert_eq!(normal, optimized);
    }

    #[test]
    fn breaks_and_exceptions_are_recomputed() {
        // Three subtables of class kerning that only need one, plus glyph
        // pairs that repeat their class value.
        let lefts = [glyphs(1..5), glyphs(5..9), glyphs(9..13)];
        let rights = [glyphs(20..25), glyphs(25..30), glyphs(30..35)];
        let builders = lefts
            .iter()
            .enumerate()
            .map(|(i, left)| {
                let mut builder = PairPosBuilder::default();
                for g1 in left.iter() {
                    builder.insert_pair(g1, advance(-(i as i16 + 1) * 10), gid(20), none());
                }
                for (j, right) in rights.iter().enumerate() {
                    let value = advance(-((i * 3 + j) as i16 + 1) * 10);
                    builder.insert_classes(left.clone(), value, right.clone(), none());
                }
                builder
            })
            .collect::<Vec<_>>();
        let (normal, optimized) = assert_equivalent(builders, 40);
        assert!(optimized < normal, "{optimized} >= {normal}");
    }

    /// A small deterministic pseudo-random number generator.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }

        fn glyphs(&mut self, n_glyphs: u16, max_len: u64) -> GlyphSet {
            let len = 1 + self.next(max_len);
            (0..len)
                .map(|_| gid(self.next(n_glyphs as u64) as u16))
                .collect()
        }

        fn value(&mut self) -> ValueRecordBuilder {
            advance(self.next(5) as i16 * 10 - 20)
        }
    }

    #[test]
    fn random_lookups_are_equivalent() {
        const N_GLYPHS: u16 = 30;
        for seed in 0..200 {
            let mut rng = Lcg(seed);
            let builders = (0..1 + rng.next(3))
                .map(|_| {
                    let mut builder = PairPosBuilder::default();
                    for _ in 0..rng.next(12) {
                        let (g1, g2) = (rng.next(30) as u16, rng.next(30) as u16);
                        let record2 = if rng.next(8) == 0 { advance(0) } else { none() };
                        builder.insert_pair(gid(g1), rng.value(), gid(g2), record2);
                    }
                    for _ in 0..rng.next(12) {
                        let class1 = rng.glyphs(N_GLYPHS, 6);
                        let class2 = rng.glyphs(N_GLYPHS, 8);
                        builder.insert_classes(class1, rng.value(), class2, none());
                    }
                    builder
                })
                .collect();
            assert_equivalent(builders, N_GLYPHS);
        }
    }
}
