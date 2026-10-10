//! Optimize residual item stores using HarfBuzz's original encoding groups.
use crate::SubsetError;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
    sync::Arc,
};
use write_fonts::{
    read::{tables::variations::ItemVariationStore as ReadStore, FontData, FontRead},
    tables::variations::{ItemVariationData, ItemVariationStore},
    types::Tag,
};

pub(super) type IndexMap = BTreeMap<u32, u32>;
fn error() -> SubsetError {
    SubsetError::SubsetTableError(Tag::new(b"GDEF"))
}

struct Encoding {
    chars: Vec<u8>,
    items: Vec<usize>,
    width: usize,
    overhead: usize,
}
impl Encoding {
    fn new(items: Vec<usize>, rows: &[Arc<[i32]>], columns: usize) -> Self {
        let mut chars = vec![0; columns];
        for &item in &items {
            for (width, &delta) in chars.iter_mut().zip(rows[item].iter()) {
                let size = if delta == 0 {
                    0
                } else if i8::try_from(delta).is_ok() {
                    1
                } else if i16::try_from(delta).is_ok() {
                    2
                } else {
                    4
                };
                *width = (*width).max(size);
            }
        }
        if chars.contains(&4) {
            for width in &mut chars {
                if *width == 1 {
                    *width = 2;
                }
            }
        }
        let width = chars.iter().map(|&c| c as usize).sum();
        let overhead = Self::overhead(&chars);
        Self {
            chars,
            items,
            width,
            overhead,
        }
    }
    fn width(&self) -> usize {
        self.width
    }
    fn overhead(chars: &[u8]) -> usize {
        // Offset32 plus the VarData header and its region-index array.
        10 + 2 * chars.iter().filter(|&&c| c != 0).count()
    }
    fn gain(&self, other: &Self) -> i64 {
        let additional = other.width as i64 - self.width as i64;
        if additional > 0 {
            if self.overhead as i64 <= additional * self.items.len() as i64 {
                return 0;
            }
        } else if other.overhead as i64 <= -additional * other.items.len() as i64 {
            return 0;
        }
        let chars: Vec<_> = self
            .chars
            .iter()
            .zip(&other.chars)
            .map(|(&a, &b)| a.max(b))
            .collect();
        let width: usize = chars.iter().map(|&c| c as usize).sum();
        (self.overhead + other.overhead - Self::overhead(&chars)) as i64
            - ((width - self.width()) * self.items.len()) as i64
            - ((width - other.width()) * other.items.len()) as i64
    }
    fn merge(&mut self, other: Self) {
        self.items.extend(other.items);
        for (a, b) in self.chars.iter_mut().zip(other.chars) {
            *a = (*a).max(b);
        }
        self.width = self.chars.iter().map(|&c| c as usize).sum();
        self.overhead = Self::overhead(&self.chars);
    }
}

/// Deduplicate rows, retain their source groups, greedily merge profitable
/// encodings, and sort rows before assigning final VariationIndex values.
pub(super) fn optimize(
    source: ItemVariationStore,
) -> Result<(ItemVariationStore, IndexMap), SubsetError> {
    let bytes = write_fonts::dump_table(&source).map_err(|_| error())?;
    let store = ReadStore::read(FontData::new(&bytes)).map_err(|_| error())?;
    let columns = source.variation_region_list.variation_regions.len();
    let mut identity = IndexMap::new();
    let mut count = 0usize;
    for (outer, data) in store.item_variation_data().iter().enumerate() {
        let Some(data) = data else { continue };
        let data = data.map_err(|_| error())?;
        count += data.item_count() as usize;
        for inner in 0..data.item_count() {
            let index = ((outer as u32) << 16) | inner as u32;
            identity.insert(index, index);
        }
    }
    // Keep the original encoding if a dense matrix or its merge queue would
    // be disproportionate. References remain valid without optimization.
    if count.saturating_mul(columns) > 4_000_000 || store.item_variation_data_count() > 2048 {
        return Ok((source, identity));
    }
    let mut rows: Vec<Arc<[i32]>> = Vec::new();
    let mut unique = BTreeMap::new();
    let mut front = BTreeMap::new();
    let mut encodings = Vec::new();
    for (outer, data) in store.item_variation_data().iter().enumerate() {
        let Some(data) = data else { continue };
        let data = data.map_err(|_| error())?;
        let mut items = Vec::new();
        for inner in 0..data.item_count() {
            let old_index = ((outer as u32) << 16) | inner as u32;
            let mut row = vec![0i64; columns];
            for (region, delta) in data.region_indexes().iter().zip(data.delta_set(inner)) {
                let column = row.get_mut(region.get() as usize).ok_or_else(error)?;
                *column += delta as i64;
            }
            let Ok(row) = row
                .into_iter()
                .map(i32::try_from)
                .collect::<Result<Vec<_>, _>>()
            else {
                // Repeated region columns can represent wider sums. Preserve
                // that valid encoding rather than narrowing its coefficients.
                return Ok((source, identity));
            };
            if row.iter().all(|&d| d == 0) {
                front.insert(old_index, None);
                continue;
            }
            let row: Arc<[i32]> = row.into();
            let item = if let Some(&item) = unique.get(&row) {
                item
            } else {
                let item = rows.len();
                unique.insert(row.clone(), item);
                rows.push(row);
                items.push(item);
                item
            };
            front.insert(old_index, Some(item));
        }
        if !items.is_empty() {
            encodings.push(Encoding::new(items, &rows, columns));
        }
    }
    // HarfBuzz sorts by width, then reverse lexicographic column widths.
    encodings.sort_by(|a, b| {
        a.width()
            .cmp(&b.width())
            .then_with(|| b.chars.cmp(&a.chars))
    });
    let mut encodings: Vec<_> = encodings.into_iter().map(Some).collect();
    let mut queue = BinaryHeap::new();
    for i in 0..encodings.len() {
        for j in i + 1..encodings.len() {
            let gain = encodings[i]
                .as_ref()
                .unwrap()
                .gain(encodings[j].as_ref().unwrap());
            if gain > 0 {
                queue.push(Reverse((-gain, i, j)));
            }
        }
    }
    while let Some(Reverse((_, i, j))) = queue.pop() {
        if encodings[i].is_none() || encodings[j].is_none() {
            continue;
        }
        let mut merged = encodings[i].take().unwrap();
        merged.merge(encodings[j].take().unwrap());
        // Identical encodings share their overhead without another merge.
        for encoding in &mut encodings {
            if encoding.as_ref().is_some_and(|e| e.chars == merged.chars) {
                merged.items.extend(encoding.take().unwrap().items);
            }
        }
        let next = encodings.len();
        for (index, encoding) in encodings.iter().enumerate() {
            if let Some(encoding) = encoding {
                let gain = merged.gain(encoding);
                if gain > 0 {
                    queue.push(Reverse((-gain, index, next)));
                }
            }
        }
        encodings.push(Some(merged));
    }
    let mut used = vec![false; columns];
    for row in &rows {
        for (used, &delta) in used.iter_mut().zip(row.iter()) {
            *used |= delta != 0;
        }
    }
    let mut region_map = vec![0u16; columns];
    let mut result = ItemVariationStore::default();
    result.variation_region_list.axis_count = source.variation_region_list.axis_count;
    for (index, region) in source
        .variation_region_list
        .variation_regions
        .iter()
        .enumerate()
    {
        if used[index] {
            region_map[index] = result.variation_region_list.variation_regions.len() as u16;
            result
                .variation_region_list
                .variation_regions
                .push(region.clone());
        }
    }
    let mut back = vec![u32::MAX; rows.len()];
    let long_words = rows
        .iter()
        .any(|r| r.iter().any(|&d| i16::try_from(d).is_err()));
    for mut encoding in encodings.into_iter().flatten() {
        encoding.items.sort_by(|&a, &b| rows[a].cmp(&rows[b]));
        for items in encoding.items.chunks(u16::MAX as usize) {
            let outer = result.item_variation_data.len();
            if outer >= u16::MAX as usize {
                return Err(error());
            }
            let mut active: Vec<_> = encoding
                .chars
                .iter()
                .enumerate()
                .filter(|&(_, &c)| c != 0)
                .map(|(i, &c)| (i, c))
                .collect();
            active.sort_by(|&(a, ca), &(b, cb)| cb.cmp(&ca).then(a.cmp(&b)));
            let word_count = active
                .iter()
                .filter(|&&(_, c)| c > if long_words { 2 } else { 1 })
                .count();
            if word_count > 0x7fff {
                return Err(error());
            }
            let mut deltas = Vec::new();
            for (inner, &item) in items.iter().enumerate() {
                back[item] = ((outer as u32) << 16) | inner as u32;
                for &(column, width) in &active {
                    let delta = rows[item][column];
                    if long_words && width == 4 {
                        deltas.extend(delta.to_be_bytes());
                    } else if long_words || width == 2 {
                        deltas.extend((delta as i16).to_be_bytes());
                    } else {
                        deltas.push(delta as i8 as u8);
                    }
                }
            }
            result.item_variation_data.push(
                Some(ItemVariationData {
                    item_count: items.len() as u16,
                    word_delta_count: word_count as u16 | if long_words { 0x8000 } else { 0 },
                    region_indexes: active
                        .iter()
                        .map(|&(column, _)| region_map[column])
                        .collect(),
                    delta_sets: deltas,
                })
                .into(),
            );
        }
    }
    let map = front
        .into_iter()
        .map(|(old, item)| (old, item.map_or(u32::MAX, |i| back[i])))
        .collect();
    Ok((result, map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance::{scalars::VariationScalars, store::encode_rows};
    use write_fonts::{
        read::tables::variations::DeltaSetIndex,
        tables::variations::{RegionAxisCoordinates, VariationRegion, VariationRegionList},
        types::F2Dot14,
    };

    fn source(groups: &[Vec<Vec<f64>>]) -> ItemVariationStore {
        ItemVariationStore::new(
            VariationRegionList::new(
                1,
                [-1f64, 0.5, 1.]
                    .map(|peak| {
                        VariationRegion::new(vec![RegionAxisCoordinates::new(
                            F2Dot14::from_f64(peak.min(0.)),
                            F2Dot14::from_f64(peak),
                            F2Dot14::from_f64(peak.max(0.)),
                        )])
                    })
                    .to_vec(),
            ),
            groups
                .iter()
                .map(|rows| Some(encode_rows(&[0, 1, 2], rows).unwrap()))
                .collect(),
        )
    }

    fn check_deltas(original: &ItemVariationStore, optimized: &ItemVariationStore, map: &IndexMap) {
        let original = write_fonts::dump_table(original).unwrap();
        let optimized = write_fonts::dump_table(optimized).unwrap();
        let original = ReadStore::read(FontData::new(&original)).unwrap();
        let optimized = ReadStore::read(FontData::new(&optimized)).unwrap();
        for coord in [-1., -0.5, 0., 0.25, 0.5, 0.75, 1.] {
            let coords = [F2Dot14::from_f64(coord)];
            let before = VariationScalars::new(&original, &coords).unwrap();
            let after = VariationScalars::new(&optimized, &coords).unwrap();
            for (&old, &new) in map {
                let index = |v| DeltaSetIndex {
                    outer: (v >> 16) as u16,
                    inner: v as u16,
                };
                assert_eq!(
                    before.condition_delta(&original, index(old)),
                    after.condition_delta(&optimized, index(new)),
                    "coord={coord} old={old} new={new}",
                );
            }
        }
    }

    #[test]
    fn original_groups_merge_duplicate_rows_share_indices_and_unused_regions_disappear() {
        let original = source(&[
            vec![vec![128., 0., 0.], vec![128., 0., 0.], vec![0., 0., 0.]],
            vec![vec![0., 1., 0.], vec![128., 0., 0.], vec![0., -128., 0.]],
        ]);
        let (optimized, map) = optimize(original.clone()).unwrap();
        assert_eq!(optimized.item_variation_data.len(), 1);
        assert_eq!(
            optimized.item_variation_data[0]
                .as_ref()
                .unwrap()
                .item_count,
            3
        );
        assert_eq!(optimized.variation_region_list.variation_regions.len(), 2);
        assert_eq!(map[&0], map[&1]);
        assert_eq!(map[&0], map[&(1 << 16 | 1)]);
        assert_eq!(map[&2], u32::MAX);
        assert_eq!(map[&(1 << 16 | 2)], 0);
        assert_eq!(map[&(1 << 16)], 1);
        assert_eq!(map[&0], 2);
        check_deltas(&original, &optimized, &map);
    }

    #[test]
    fn signed_widths_long_words_and_repeated_columns_preserve_values() {
        let rows = [-32769., -32768., -129., -128., 127., 128., 32767., 32768.]
            .map(|delta| vec![delta, 1., -1.])
            .to_vec();
        let original = source(&[rows]);
        let (optimized, map) = optimize(original.clone()).unwrap();
        assert!(
            optimized.item_variation_data[0]
                .as_ref()
                .unwrap()
                .word_delta_count
                & 0x8000
                != 0
        );
        check_deltas(&original, &optimized, &map);

        let original = source(&[vec![vec![i32::MAX as f64 + 1., 0., 0.]]]);
        let (optimized, map) = optimize(original.clone()).unwrap();
        assert_eq!(optimized, original);
        assert_eq!(map[&0], 0);
        check_deltas(&original, &optimized, &map);
    }

    #[test]
    fn zero_rows_leave_an_empty_store_and_no_variation_indices() {
        let original = source(&[vec![vec![0., 0., 0.]; 4]]);
        let (optimized, map) = optimize(original.clone()).unwrap();
        assert!(optimized.item_variation_data.is_empty());
        assert!(optimized.variation_region_list.variation_regions.is_empty());
        assert!(map.values().all(|&v| v == u32::MAX));
        check_deltas(&original, &optimized, &map);
    }

    #[test]
    fn merged_encodings_split_before_the_inner_index_limit() {
        let group = |start, end| (start..end).map(|v| vec![v as f64, 0., 0.]).collect();
        let original = source(&[group(1, 65536), group(65536, 65538)]);
        let (optimized, map) = optimize(original).unwrap();
        assert_eq!(optimized.item_variation_data.len(), 2);
        assert_eq!(
            optimized.item_variation_data[0]
                .as_ref()
                .unwrap()
                .item_count,
            u16::MAX
        );
        assert_eq!(
            optimized.item_variation_data[1]
                .as_ref()
                .unwrap()
                .item_count,
            2
        );
        assert_eq!(map[&0], 0);
        assert_eq!(map[&(1 << 16)], 1 << 16);
        assert_eq!(map[&(1 << 16 | 1)], 1 << 16 | 1);
    }
}
