//! Size-optimized compilation of pair positioning lookups.
//!
//! See [`PairPosBuilder::build_optimized`].

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

/// An interned adjustment: an index into [`Values::records`].
type ValueId = u32;

/// The id of the adjustment that does nothing.
const NO_ADJUSTMENT: ValueId = 0;

/// Bytes a glyph pair exception costs, besides its value record.
///
/// This is the second glyph id; the overhead of its `PairSet` and coverage
/// entry is usually shared by many pairs, and is counted separately where
/// it matters.
const EXCEPTION_OVERHEAD: usize = 2;

/// Bytes a first glyph with exceptions costs: its `PairSet`'s count and
/// offset, and a coverage entry.
const PAIR_SET_OVERHEAD: usize = 6;

/// Guesses at the grid cells a class costs for each of its non-zero values.
///
/// Class-based subtables store a dense grid, so a class also pays for zeros
/// where other classes in its subtable have values. How full the grids end
/// up varies from lookup to lookup, from a third to more than half, and the
/// best classes depend on it; so we plan with each guess, and keep the
/// smallest result.
const CELLS_PER_VALUE_GUESSES: [usize; 3] = [1, 2, 3];

/// Bytes a class-based subtable costs besides its tables and grid.
///
/// This is the subtable header, and the subtable's offset in its lookup.
const SUBTABLE_OVERHEAD: usize = 16 + 2;

/// Build the subtables of a pair positioning lookup, optimized for size.
///
/// `subtables` are the builders of one lookup, in order. A lookup with
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
    subtables: impl IntoIterator<Item = PairPosBuilder>,
    var_store: &mut VariationStoreBuilder,
) -> Vec<PairPos> {
    let subtables = subtables.into_iter().collect::<Vec<_>>();
    let Some(flat) = Flattened::new(&subtables) else {
        return subtables
            .into_iter()
            .flat_map(|subtable| subtable.build(var_store))
            .collect();
    };
    let exact = Grid::exact(&flat);
    let plan = CELLS_PER_VALUE_GUESSES
        .into_iter()
        .map(|cells_per_value| Plan::new(&flat, exact.clone(), cells_per_value))
        .min_by_key(|plan| plan.size)
        .expect("there is at least one guess");
    plan.emit(&flat, var_store)
}

/// Classes, class-based subtables and exceptions for a lookup.
struct Plan {
    grid: Grid,
    subtables: Vec<Subtable>,
    /// First glyph -> second glyph -> exception.
    exceptions: BTreeMap<GlyphId16, BTreeMap<GlyphId16, Exception>>,
    /// The estimated size of the subtables.
    size: usize,
}

/// A pair that the class-based subtables would adjust wrongly.
#[derive(Clone, Copy, Debug)]
enum Exception {
    /// The pair's adjustment.
    Adjust(ValueId),
    /// No adjustment: an explicit zero, overriding this class-based adjustment.
    Zero(ValueId),
}

impl Plan {
    /// Plan a lookup, starting from its exact classes.
    ///
    /// `cells_per_value` is passed to [`Grid::fold_small_classes`].
    fn new(flat: &Flattened, mut grid: Grid, cells_per_value: usize) -> Self {
        grid.fold_small_classes(&flat.values, cells_per_value);
        let subtables = grid.group_rows(flat);
        let exceptions = find_exceptions(flat, &grid, &subtables);
        let values = &flat.values;
        let exceptions_size = exceptions
            .values()
            .map(|row| {
                let pairs = row.values().map(|exception| match exception {
                    Exception::Adjust(value) | Exception::Zero(value) => {
                        EXCEPTION_OVERHEAD + values.size(*value)
                    }
                });
                PAIR_SET_OVERHEAD + pairs.sum::<usize>()
            })
            .sum::<usize>();
        let size = exceptions_size + subtables.iter().map(|sub| sub.cost).sum::<usize>();
        Plan {
            grid,
            subtables,
            exceptions,
            size,
        }
    }

    /// Build the glyph pair exceptions, then the class-based subtables.
    fn emit(self, flat: &Flattened, var_store: &mut VariationStoreBuilder) -> Vec<PairPos> {
        let values = &flat.values;
        let mut exceptions = GlyphPairPosBuilder::default();
        for (g1, row) in &self.exceptions {
            let out = exceptions.0.entry(*g1).or_default();
            for (g2, exception) in row {
                let records = match exception {
                    Exception::Adjust(value) => values.get(*value).clone(),
                    Exception::Zero(value) => (values.zeroed(*value), ValueRecordBuilder::new()),
                };
                out.insert(*g2, records);
            }
        }

        let mut out = exceptions.build(var_store);
        let mut subtables = self.subtables.iter().collect::<Vec<_>>();
        subtables.sort_by_key(|subtable| subtable.left.first);
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

/// The distinct adjustments in a lookup.
struct Values {
    records: Vec<(ValueRecordBuilder, ValueRecordBuilder)>,
    ids: BTreeMap<(ValueRecordBuilder, ValueRecordBuilder), ValueId>,
}

impl Values {
    fn new() -> Self {
        let none = (ValueRecordBuilder::new(), ValueRecordBuilder::new());
        Values {
            records: vec![none.clone()],
            ids: BTreeMap::from([(none, NO_ADJUSTMENT)]),
        }
    }

    /// Intern an adjustment, so that adjustments with the same effect are equal.
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

    fn get(&self, id: ValueId) -> &(ValueRecordBuilder, ValueRecordBuilder) {
        &self.records[id as usize]
    }

    /// `true` if this adjustment applies to the second glyph.
    ///
    /// These can only be glyph pair exceptions: the second value format is
    /// shared by every pair in a class-based subtable.
    fn adjusts_second(&self, id: ValueId) -> bool {
        !self.get(id).1.format().is_empty()
    }

    fn format(&self, id: ValueId) -> ValueFormat {
        self.get(id).0.format()
    }

    /// The size of the first value record of this adjustment.
    fn size(&self, id: ValueId) -> usize {
        self.format(id).record_byte_len()
    }

    /// A record with the same fields as this adjustment's first record, all zero.
    fn zeroed(&self, id: ValueId) -> ValueRecordBuilder {
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

/// The adjustment that applies to each pair of glyphs.
struct Flattened {
    values: Values,
    /// First glyph -> second glyph -> adjustment, for each adjusted pair.
    pairs: BTreeMap<GlyphId16, BTreeMap<GlyphId16, ValueId>>,
}

impl Flattened {
    /// Flatten a lookup's rules, respecting the precedence of its subtables.
    ///
    /// Returns `None` if a class-based subtable adjusts the second glyph.
    fn new(subtables: &[PairPosBuilder]) -> Option<Self> {
        let mut values = Values::new();
        let mut pairs: BTreeMap<GlyphId16, BTreeMap<GlyphId16, ValueId>> = BTreeMap::new();
        // First glyphs covered by an earlier class-based subtable: it applies
        // to every pair starting with them, so no later rule can.
        let mut covered = HashSet::new();
        for builder in subtables {
            // a builder's glyph pair subtables come before its class subtables
            for (g1, seconds) in &builder.pairs.0 {
                if covered.contains(g1) {
                    continue;
                }
                let row = pairs.entry(*g1).or_default();
                for (g2, (record1, record2)) in seconds {
                    row.entry(*g2)
                        .or_insert_with(|| values.intern(record1, record2));
                }
            }
            for subtable in &builder.classes.0 {
                if !subtable.compute_value_formats().1.is_empty() {
                    return None;
                }
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
                        let row = pairs.entry(g1).or_default();
                        for (class2, value) in &seconds {
                            for g2 in class2.iter() {
                                row.entry(g2).or_insert(*value);
                            }
                        }
                    }
                }
                covered.extend(newly_covered);
            }
        }
        for row in pairs.values_mut() {
            row.retain(|_, value| *value != NO_ADJUSTMENT);
        }
        pairs.retain(|_, row| !row.is_empty());
        Some(Flattened { values, pairs })
    }

    fn get(&self, g1: GlyphId16, g2: GlyphId16) -> ValueId {
        self.pairs
            .get(&g1)
            .and_then(|row| row.get(&g2))
            .copied()
            .unwrap_or(NO_ADJUSTMENT)
    }
}

/// A class of glyphs, and its row or column of the grid.
#[derive(Clone, Debug, Default)]
struct Line {
    /// The glyphs in this class, sorted.
    glyphs: Vec<GlyphId16>,
    /// The non-zero cells: the crossing line's index -> adjustment.
    cells: BTreeMap<usize, ValueId>,
}

impl Line {
    fn is_live(&self) -> bool {
        !self.glyphs.is_empty() && !self.cells.is_empty()
    }
}

/// Classes of first glyphs (rows) and second glyphs (columns), and the
/// adjustment for each pair of classes.
///
/// Each cell is stored in both its row and its column. Folding a class into
/// another leaves an empty line in its place, so indices stay stable.
///
/// The grid doesn't record which pairs differ from their classes' cell: those
/// exceptions are found by comparing with the flattened pairs at the end.
#[derive(Clone, Debug)]
struct Grid {
    rows: Vec<Line>,
    cols: Vec<Line>,
}

impl Grid {
    /// Group glyphs that are adjusted identically into classes.
    ///
    /// Adjustments of the second glyph are left out: they are always exceptions.
    fn exact(flat: &Flattened) -> Self {
        let values = &flat.values;
        let mut columns: BTreeMap<GlyphId16, Vec<(GlyphId16, ValueId)>> = BTreeMap::new();
        for (g1, row) in &flat.pairs {
            for (g2, value) in row {
                if !values.adjusts_second(*value) {
                    columns.entry(*g2).or_default().push((*g1, *value));
                }
            }
        }
        let mut cols: Vec<Line> = Vec::new();
        let mut col_ids = BTreeMap::new();
        let mut col_of = HashMap::new();
        for (g2, column) in columns {
            let next = col_ids.len();
            let col = *col_ids.entry(column).or_insert(next);
            if col == cols.len() {
                cols.push(Line::default());
            }
            cols[col].glyphs.push(g2);
            col_of.insert(g2, col);
        }

        let mut rows: Vec<Line> = Vec::new();
        let mut row_ids = BTreeMap::new();
        for (g1, row) in &flat.pairs {
            let mut cells = row
                .iter()
                .filter(|(_, value)| !values.adjusts_second(**value))
                .map(|(g2, value)| (col_of[g2], *value))
                .collect::<Vec<_>>();
            // every glyph in a column class has the same value in this row
            cells.sort_unstable();
            cells.dedup();
            if cells.is_empty() {
                continue;
            }
            let next = row_ids.len();
            let row = *row_ids.entry(cells.clone()).or_insert(next);
            if row == rows.len() {
                rows.push(Line {
                    glyphs: Vec::new(),
                    cells: cells.into_iter().collect(),
                });
            }
            rows[row].glyphs.push(*g1);
        }
        for (r, row) in rows.iter().enumerate() {
            for (c, value) in &row.cells {
                cols[*c].cells.insert(r, *value);
            }
        }
        Grid { rows, cols }
    }

    fn transpose(&mut self) {
        std::mem::swap(&mut self.rows, &mut self.cols);
    }

    /// Fold small classes into bigger ones, until that stops saving bytes.
    ///
    /// A class folded into another takes on its values; pairs where they
    /// differ become exceptions. A class can also be dropped entirely, making
    /// all of its pairs exceptions.
    ///
    /// The bytes saved by removing a row or column are estimated as
    /// `cells_per_value` grid cells for each of its non-zero values.
    fn fold_small_classes(&mut self, values: &Values, cells_per_value: usize) {
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

    /// One round of folding columns; returns `true` if any were folded.
    fn fold_columns(&mut self, values: &Values, cells_per_value: usize) -> bool {
        let mut by_cell: HashMap<(usize, ValueId), Vec<usize>> = HashMap::new();
        for (c, col) in self.cols.iter().enumerate() {
            for (r, value) in &col.cells {
                by_cell.entry((*r, *value)).or_default().push(c);
            }
        }
        let is_bigger = |a: usize, b: usize| {
            let (len_a, len_b) = (self.cols[a].glyphs.len(), self.cols[b].glyphs.len());
            len_a > len_b || (len_a == len_b && a < b)
        };

        // (bytes saved, column, the column it's folded into, or None to drop it)
        let mut folds = Vec::new();
        for (b, col) in self.cols.iter().enumerate() {
            if !col.is_live() {
                continue;
            }
            let saved = col
                .cells
                .values()
                .map(|value| values.size(*value) * cells_per_value)
                .sum::<usize>();
            let drop_cost = col
                .cells
                .iter()
                .map(|(r, value)| {
                    self.rows[*r].glyphs.len()
                        * col.glyphs.len()
                        * (EXCEPTION_OVERHEAD + values.size(*value))
                })
                .sum::<usize>();
            let mut best = (drop_cost < saved).then_some((drop_cost, None));
            let candidates = col
                .cells
                .iter()
                .flat_map(|(r, value)| &by_cell[&(*r, *value)])
                .copied()
                .filter(|a| *a != b && is_bigger(*a, b))
                .collect::<BTreeSet<_>>();
            for a in candidates {
                let limit = best.map_or(saved, |(best_cost, _)| best_cost);
                if let Some(cost) = self.fold_cost(a, b, values, limit) {
                    best = Some((cost, Some(a)));
                }
            }
            if let Some((cost, into)) = best {
                folds.push((saved - cost, b, into));
            }
        }

        // A column is either folded or folded into in one round, so that the
        // costs computed above stay accurate.
        folds.sort_by_key(|(gain, b, _)| (Reverse(*gain), *b));
        let mut folded = HashSet::new();
        let mut targets = HashSet::new();
        for (_, b, into) in &folds {
            if folded.contains(b) || targets.contains(b) {
                continue;
            }
            if let Some(a) = into {
                if folded.contains(a) {
                    continue;
                }
                targets.insert(*a);
            }
            folded.insert(*b);
            self.fold_column(*b, *into);
        }
        !folded.is_empty()
    }

    /// The cost of the exceptions created by folding column `b` into column `a`.
    ///
    /// Returns `None` if the cost isn't less than `limit`.
    fn fold_cost(&self, a: usize, b: usize, values: &Values, limit: usize) -> Option<usize> {
        let mut col_a = self.cols[a].cells.iter().peekable();
        let mut col_b = self.cols[b].cells.iter().peekable();
        let len_b = self.cols[b].glyphs.len();
        let mut cost = 0;
        // walk both columns' cells in row order
        loop {
            let (r, value_a, value_b) = match (col_a.peek(), col_b.peek()) {
                (None, None) => break,
                (Some(&(&ra, &va)), Some(&(&rb, &vb))) => match ra.cmp(&rb) {
                    Ordering::Equal => {
                        col_a.next();
                        col_b.next();
                        (ra, va, vb)
                    }
                    Ordering::Less => {
                        col_a.next();
                        (ra, va, NO_ADJUSTMENT)
                    }
                    Ordering::Greater => {
                        col_b.next();
                        (rb, NO_ADJUSTMENT, vb)
                    }
                },
                (Some(&(&ra, &va)), None) => {
                    col_a.next();
                    (ra, va, NO_ADJUSTMENT)
                }
                (None, Some(&(&rb, &vb))) => {
                    col_b.next();
                    (rb, NO_ADJUSTMENT, vb)
                }
            };
            if value_a != value_b {
                let size = values.size(value_a).max(values.size(value_b));
                cost += self.rows[r].glyphs.len() * len_b * (EXCEPTION_OVERHEAD + size);
                if cost >= limit {
                    return None;
                }
            }
        }
        Some(cost)
    }

    /// Fold column `b` into column `into`, or drop it if `into` is `None`.
    fn fold_column(&mut self, b: usize, into: Option<usize>) {
        let col = std::mem::take(&mut self.cols[b]);
        for r in col.cells.keys() {
            self.rows[*r].cells.remove(&b);
        }
        if let Some(a) = into {
            let glyphs = &mut self.cols[a].glyphs;
            glyphs.extend(col.glyphs);
            glyphs.sort_unstable();
        }
    }

    /// Group rows into class-based subtables, minimizing their total size.
    ///
    /// Rows that are smaller as glyph pair exceptions are left out.
    fn group_rows(&self, flat: &Flattened) -> Vec<Subtable> {
        let spans = Spans {
            rows: self.rows.iter().map(|row| Span::new(&row.glyphs)).collect(),
            cols: self.cols.iter().map(|col| Span::new(&col.glyphs)).collect(),
        };
        let mut subtables = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.is_live())
            .map(|(r, _)| Some(Subtable::new(self, &spans, &flat.values, [r])))
            .collect::<Vec<_>>();

        // Agglomerative clustering: repeatedly merge the pair of subtables
        // that saves the most bytes. `best[i]` caches subtable `i`'s best merge.
        let gain = |subtables: &[Option<Subtable>], i: usize, j: usize| -> i64 {
            let (a, b) = (
                subtables[i].as_ref().unwrap(),
                subtables[j].as_ref().unwrap(),
            );
            (a.cost + b.cost) as i64 - a.merged_cost(b, &spans) as i64
        };
        let best_merge = |subtables: &[Option<Subtable>], i: usize| -> Option<(i64, usize)> {
            (0..subtables.len())
                .filter(|j| *j != i && subtables[*j].is_some())
                .map(|j| (gain(subtables, i, j), j))
                .max_by_key(|(gain, j)| (*gain, Reverse(*j)))
        };
        let mut best = (0..subtables.len())
            .map(|i| best_merge(&subtables, i))
            .collect::<Vec<_>>();
        while let Some((i, (merge_gain, j))) = best
            .iter()
            .enumerate()
            .filter_map(|(i, best)| best.map(|best| (i, best)))
            .max_by_key(|(i, (gain, _))| (*gain, Reverse(*i)))
        {
            if merge_gain <= 0 {
                break;
            }
            let merged = subtables[i].take().unwrap().merge(
                subtables[j].take().unwrap(),
                self,
                &spans,
                &flat.values,
            );
            subtables[i] = Some(merged);
            best[j] = None;
            best[i] = best_merge(&subtables, i);
            for k in 0..subtables.len() {
                if k == i || subtables[k].is_none() {
                    continue;
                }
                match best[k] {
                    Some((_, partner)) if partner == i || partner == j => {
                        best[k] = best_merge(&subtables, k);
                    }
                    current => {
                        let with_i = gain(&subtables, k, i);
                        if current.is_none_or(|(gain, _)| with_i > gain) {
                            best[k] = Some((with_i, i));
                        }
                    }
                }
            }
        }

        let col_of = self
            .cols
            .iter()
            .enumerate()
            .flat_map(|(c, col)| col.glyphs.iter().map(move |g2| (*g2, c)))
            .collect::<HashMap<_, _>>();
        let mut subtables = subtables.into_iter().flatten().collect::<Vec<_>>();
        for subtable in &mut subtables {
            subtable.drop_rows_better_as_exceptions(self, &spans, flat, &col_of);
        }
        subtables.retain(|subtable| !subtable.rows.is_empty());
        subtables
    }
}

/// A summary of a set of glyphs, enough to estimate the size of a coverage or
/// class definition table listing them.
#[derive(Clone, Copy, Debug, Default)]
struct Span {
    count: usize,
    /// The number of runs of consecutive glyph ids.
    ranges: usize,
    first: u16,
    last: u16,
}

impl Span {
    /// `glyphs` must be sorted.
    fn new(glyphs: &[GlyphId16]) -> Self {
        let (Some(first), Some(last)) = (glyphs.first(), glyphs.last()) else {
            return Span::default();
        };
        let ranges = 1 + glyphs
            .windows(2)
            .filter(|pair| pair[1].to_u16() != pair[0].to_u16() + 1)
            .count();
        Span {
            count: glyphs.len(),
            ranges,
            first: first.to_u16(),
            last: last.to_u16(),
        }
    }

    /// The union of two disjoint sets.
    ///
    /// Runs that become adjacent aren't merged, so `ranges` may overestimate.
    fn union(self, other: Span) -> Span {
        if self.count == 0 {
            return other;
        }
        if other.count == 0 {
            return self;
        }
        Span {
            count: self.count + other.count,
            ranges: self.ranges + other.ranges,
            first: self.first.min(other.first),
            last: self.last.max(other.last),
        }
    }

    /// Remove a subset; `first` and `last` are left as they were.
    fn without(self, other: Span) -> Span {
        Span {
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

/// The span of each row and column of a [`Grid`].
struct Spans {
    rows: Vec<Span>,
    cols: Vec<Span>,
}

/// A class-based subtable: a set of rows, and the columns they use.
#[derive(Clone, Debug)]
struct Subtable {
    rows: Vec<usize>,
    /// A bitset of the columns with a non-zero value in some row.
    cols: Vec<u64>,
    n_cols: usize,
    /// All of the first glyphs.
    left: Span,
    /// The biggest row, which is class 0 and so isn't in the class definition.
    biggest_row: Span,
    /// All of the second glyphs.
    right: Span,
    format: ValueFormat,
    cost: usize,
}

impl Subtable {
    fn new(
        grid: &Grid,
        spans: &Spans,
        values: &Values,
        rows: impl IntoIterator<Item = usize>,
    ) -> Self {
        let rows = rows.into_iter().collect::<Vec<_>>();
        let mut cols = vec![0u64; grid.cols.len().div_ceil(64)];
        let mut left = Span::default();
        let mut biggest_row = Span::default();
        let mut format = ValueFormat::empty();
        for r in &rows {
            let span = spans.rows[*r];
            left = left.union(span);
            if span.count > biggest_row.count {
                biggest_row = span;
            }
            for (c, value) in &grid.rows[*r].cells {
                cols[c / 64] |= 1 << (c % 64);
                format |= values.format(*value);
            }
        }
        let right = iter_bits(&cols).fold(Span::default(), |span, c| span.union(spans.cols[c]));
        let n_cols = cols.iter().map(|word| word.count_ones() as usize).sum();
        let mut subtable = Subtable {
            rows,
            cols,
            n_cols,
            left,
            biggest_row,
            right,
            format,
            cost: 0,
        };
        subtable.cost = subtable.compute_cost();
        subtable
    }

    fn compute_cost(&self) -> usize {
        subtable_size(
            self.rows.len(),
            self.n_cols,
            self.left,
            self.biggest_row,
            self.right,
            self.format,
        )
    }

    /// The cost of the union of two subtables, without computing all of it.
    fn merged_cost(&self, other: &Subtable, spans: &Spans) -> usize {
        let mut right = self.right.union(other.right);
        let mut n_cols = self.n_cols + other.n_cols;
        let shared = self
            .cols
            .iter()
            .zip(&other.cols)
            .map(|(a, b)| a & b)
            .collect::<Vec<_>>();
        for c in iter_bits(&shared) {
            right = right.without(spans.cols[c]);
            n_cols -= 1;
        }
        let biggest_row = if other.biggest_row.count > self.biggest_row.count {
            other.biggest_row
        } else {
            self.biggest_row
        };
        subtable_size(
            self.rows.len() + other.rows.len(),
            n_cols,
            self.left.union(other.left),
            biggest_row,
            right,
            self.format | other.format,
        )
    }

    fn merge(self, other: Subtable, grid: &Grid, spans: &Spans, values: &Values) -> Subtable {
        Subtable::new(grid, spans, values, self.rows.into_iter().chain(other.rows))
    }

    /// Remove rows that cost less as glyph pair exceptions than in this subtable.
    fn drop_rows_better_as_exceptions(
        &mut self,
        grid: &Grid,
        spans: &Spans,
        flat: &Flattened,
        col_of: &HashMap<GlyphId16, usize>,
    ) {
        let values = &flat.values;
        let mut i = 0;
        while i < self.rows.len() {
            let r = self.rows[i];
            let row = &grid.rows[r];
            let (dropped, kept) = row
                .glyphs
                .iter()
                .map(|g1| exception_costs(*g1, row, grid, flat, col_of))
                .fold((0, 0), |(a, b), (c, d)| (a + c, b + d));
            let others = self.rows.iter().copied().filter(|other| *other != r);
            let without = Subtable::new(grid, spans, values, others);
            let in_subtable = self.cost
                - if without.rows.is_empty() {
                    0
                } else {
                    without.cost
                };
            if dropped < in_subtable + kept {
                *self = without;
            } else {
                i += 1;
            }
        }
    }
}

/// The cost of a first glyph's exceptions if its row is dropped, and if it is kept.
///
/// A dropped row's pairs are all exceptions; a kept row's are the pairs its
/// class's row gets wrong.
fn exception_costs(
    g1: GlyphId16,
    row: &Line,
    grid: &Grid,
    flat: &Flattened,
    col_of: &HashMap<GlyphId16, usize>,
) -> (usize, usize) {
    let values = &flat.values;
    let cost = |value| EXCEPTION_OVERHEAD + values.size(value);
    let (mut dropped, mut kept) = (0, 0);
    for (g2, value) in flat.pairs.get(&g1).into_iter().flatten() {
        dropped += cost(*value);
        let class_value = col_of.get(g2).and_then(|c| row.cells.get(c));
        if values.adjusts_second(*value) || class_value != Some(value) {
            kept += cost(*value);
        }
    }
    for (c, value) in &row.cells {
        let unwanted = grid.cols[*c]
            .glyphs
            .iter()
            .filter(|g2| flat.get(g1, **g2) == NO_ADJUSTMENT)
            .count();
        kept += unwanted * cost(*value);
    }
    let pair_set = |cost: usize| {
        if cost > 0 {
            cost + PAIR_SET_OVERHEAD
        } else {
            0
        }
    };
    (pair_set(dropped), pair_set(kept))
}

/// The size of a class-based subtable, estimated from its glyphs.
fn subtable_size(
    n_rows: usize,
    n_cols: usize,
    left: Span,
    biggest_row: Span,
    right: Span,
    format: ValueFormat,
) -> usize {
    SUBTABLE_OVERHEAD
        + left.coverage_size()
        + left.without(biggest_row).class_def_size()
        + right.class_def_size()
        + n_rows * (n_cols + 1) * format.record_byte_len()
}

fn iter_bits(bits: &[u64]) -> impl Iterator<Item = usize> + '_ {
    bits.iter().enumerate().flat_map(|(i, word)| {
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

/// Find the pairs that the class-based subtables would adjust wrongly.
///
/// This includes pairs that they would adjust but shouldn't be.
fn find_exceptions(
    flat: &Flattened,
    grid: &Grid,
    subtables: &[Subtable],
) -> BTreeMap<GlyphId16, BTreeMap<GlyphId16, Exception>> {
    let values = &flat.values;
    let mut row_of = HashMap::new();
    for subtable in subtables {
        for r in &subtable.rows {
            row_of.extend(grid.rows[*r].glyphs.iter().map(|g1| (*g1, *r)));
        }
    }
    let mut col_of = HashMap::new();
    for (c, col) in grid.cols.iter().enumerate() {
        col_of.extend(col.glyphs.iter().map(|g2| (*g2, c)));
    }
    let class_value = |g1: &GlyphId16, g2: &GlyphId16| {
        row_of
            .get(g1)
            .zip(col_of.get(g2))
            .and_then(|(r, c)| grid.rows[*r].cells.get(c))
            .copied()
            .unwrap_or(NO_ADJUSTMENT)
    };

    let mut exceptions: BTreeMap<GlyphId16, BTreeMap<GlyphId16, Exception>> = BTreeMap::new();
    for (g1, row) in &flat.pairs {
        for (g2, value) in row {
            if values.adjusts_second(*value) || class_value(g1, g2) != *value {
                let exception = Exception::Adjust(*value);
                exceptions.entry(*g1).or_default().insert(*g2, exception);
            }
        }
    }
    for subtable in subtables {
        for r in &subtable.rows {
            let row = &grid.rows[*r];
            for (c, value) in &row.cells {
                for g1 in &row.glyphs {
                    for g2 in &grid.cols[*c].glyphs {
                        if flat.get(*g1, *g2) == NO_ADJUSTMENT {
                            let exception = Exception::Zero(*value);
                            exceptions.entry(*g1).or_default().insert(*g2, exception);
                        }
                    }
                }
            }
        }
    }
    exceptions
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
