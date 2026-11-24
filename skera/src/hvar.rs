//! impl subset() for HVAR

use crate::offset::SerializeSerialize;
use crate::FastHashMap;
use crate::{
    delta_set_counts,
    offset::SerializeSubset,
    serialize::{SerializeErrorFlags, Serializer},
    variations::DeltaSetIndexMapSerializePlan,
    IncBiMap, Plan, Serialize, Subset, SubsetError, SubsetFlags,
};
use read_fonts::{
    collections::IntSet,
    tables::{
        hvar::Hvar,
        variations::{DeltaSetIndex, DeltaSetIndexMap, ItemVariationStore},
    },
    types::{FixedSize, Offset32},
    FontRef, ReadError, TopLevelTable,
};
use write_fonts::FontBuilder;

// reference: subset() for HVAR in harfbuzz
// <https://github.com/harfbuzz/harfbuzz/blob/bcd5aa368d3fd3ef741ea29df15d3d56011811c0/src/hb-ot-var-hvar-table.hh#L330>
impl Subset for Hvar<'_> {
    fn subset(
        &self,
        plan: &Plan,
        _font: &FontRef,
        s: &mut Serializer,
        _builder: &mut FontBuilder,
    ) -> Result<(), SubsetError> {
        Self::serialize(s, (self, plan)).map_err(|_| SubsetError::SubsetTableError(Hvar::TAG))
    }
}

impl<'a> Serialize<'a> for Hvar<'_> {
    type Args = (&'a Hvar<'a>, &'a Plan);
    fn serialize(s: &mut Serializer, args: Self::Args) -> Result<(), SerializeErrorFlags> {
        let (hvar, plan) = args;
        s.embed(hvar.version())?;
        if hvar.item_variation_store_offset().is_null() {
            return s
                .allocate_size(5 * Offset32::RAW_BYTE_LEN, true)
                .map(|_| ());
        }

        let var_store = hvar
            .item_variation_store()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let index_maps = hvar
            .listup_index_maps()
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let hvar_subset_plan = HvarVvarSubsetPlan::new(plan, &var_store, &index_maps)
            .map_err(|_| s.set_err(SerializeErrorFlags::SERIALIZE_ERROR_READ_ERROR))?;

        let var_store_offset_pos = s.embed(0_u32)?;

        Offset32::serialize_subset(
            &var_store,
            s,
            plan,
            (hvar_subset_plan.inner_maps(), true),
            var_store_offset_pos,
        )?;

        serialize_index_maps(
            s,
            plan,
            &index_maps,
            hvar_subset_plan.index_map_subset_plans(),
        )
    }
}

pub(crate) trait ListupIndexMaps {
    fn listup_index_maps(&self) -> Result<Vec<Option<DeltaSetIndexMap<'_>>>, ReadError>;
}

impl ListupIndexMaps for Hvar<'_> {
    fn listup_index_maps(&self) -> Result<Vec<Option<DeltaSetIndexMap<'_>>>, ReadError> {
        let out = vec![
            self.advance_width_mapping().transpose()?,
            self.lsb_mapping().transpose()?,
            self.rsb_mapping().transpose()?,
        ];
        Ok(out)
    }
}

pub(crate) fn serialize_index_maps(
    s: &mut Serializer,
    _plan: &Plan,
    index_maps: &[Option<DeltaSetIndexMap>],
    index_map_plans: &[IndexMapSubsetPlan],
) -> Result<(), SerializeErrorFlags> {
    if index_maps.len() != index_map_plans.len() {
        return Err(SerializeErrorFlags::SERIALIZE_ERROR_OTHER);
    }

    for index_map_subset_plan in index_map_plans {
        let offset_pos = s.embed(0_u32)?;
        if index_map_subset_plan.is_identity() {
            continue;
        }

        Offset32::serialize_serialize::<DeltaSetIndexMap>(
            s,
            &index_map_subset_plan.to_serialize_plan(),
            offset_pos,
        )?;
    }
    Ok(())
}

#[derive(Default)]
pub(crate) struct IndexMapSubsetPlan {
    map_count: u32,
    /// Number of entries a map covering every output glyph would have,
    /// i.e. (largest new glyph id) + 1.
    full_map_count: u32,
    /// Whether this map is allowed to be dropped in favor of implicit
    /// glyph-id indexing. Only the advance mapping qualifies, and only when
    /// the source table was map-less too (for the other maps a null offset
    /// means "no deltas", so dropping would change the meaning).
    can_drop_to_implicit: bool,
    outer_bit_count: u8,
    inner_bit_count: u8,
    output_map: FastHashMap<u32, u32>,
}

/// The delta set that `old_gid` has in `index_map`, or the implicit one of its
/// glyph id without a map, with the inner index limited to `row_counts`.
///
/// An index can name a row that its ItemVariationData doesn't have, which
/// reads as no delta, and the glyphs of a font can name any number of them.
/// They are all taken for the first row past those of the subtable, the one at
/// its row count, so one row is retained for them. It is subset as a row of
/// zeros, which keeps their meaning, and the region scans of the store read no
/// more rows than the subtable has, and that one.
fn delta_set_index(
    index_map: Option<&DeltaSetIndexMap>,
    old_gid: u32,
    row_counts: &[u32],
) -> Result<DeltaSetIndex, ReadError> {
    let mut index = match index_map {
        Some(m) => m.get(old_gid)?,
        None => DeltaSetIndex {
            outer: (old_gid >> 16) as u16,
            inner: (old_gid & 0xFFFF) as u16,
        },
    };
    if let Some(row_count) = row_counts.get(index.outer as usize) {
        index.inner = u32::from(index.inner).min(*row_count) as u16;
    }
    Ok(index)
}

impl IndexMapSubsetPlan {
    fn new(
        index_map: Option<&DeltaSetIndexMap>,
        plan: &Plan,
        bypass_empty: bool,
        outer_map: &mut IncBiMap,
        inner_sets: &mut [IntSet<u16>],
        row_counts: &[u32],
    ) -> Result<Self, ReadError> {
        let mut this = IndexMapSubsetPlan {
            full_map_count: plan
                .new_to_old_gid_list
                .last()
                .map(|(new_gid, _)| new_gid.to_u32() + 1)
                .unwrap_or(0),
            // Only the advance mapping may be dropped in favor of implicit
            // glyph-id indexing, and only when the source was map-less too.
            can_drop_to_implicit: !bypass_empty && index_map.is_none(),
            ..Default::default()
        };

        if bypass_empty && index_map.is_none() {
            return Ok(this);
        }

        let mut last_gid = None;
        let mut last_val = DeltaSetIndex::NO_VARIATION_INDEX;
        for (gid, old_gid) in plan.new_to_old_gid_list.iter().rev() {
            let val = delta_set_index(index_map, old_gid.to_u32(), row_counts)?;
            if last_gid.is_none() {
                last_gid = Some(*gid);
                last_val = val;
                continue;
            }
            if val != last_val {
                break;
            }
            last_gid = Some(*gid);
        }

        if last_gid.is_none() {
            return Ok(this);
        }
        this.map_count = last_gid.unwrap().to_u32() + 1;

        if index_map.is_none() {
            // the row of a glyph id, or the one past the rows of the subtable
            let row = |gid: u32| (gid as u16).min(row_counts[0] as u16);
            outer_map.add(0);
            inner_sets[0].extend(plan.glyphset.iter().map(|g| row(g.to_u32())));
        } else {
            for (new_gid, old_gid) in plan.new_to_old_gid_list.iter() {
                if new_gid.to_u32() >= this.map_count {
                    break;
                }

                let v = delta_set_index(index_map, old_gid.to_u32(), row_counts)?;
                let outer = v.outer as usize;
                // An outer index past the store's data count selects no delta set;
                // skip the entry, as remap() does, and keep collecting for the
                // glyphs after it.
                if outer >= inner_sets.len() {
                    continue;
                }

                outer_map.add(v.outer as u32);
                inner_sets[outer].insert(v.inner);
            }
        }

        Ok(this)
    }

    fn remap(
        &mut self,
        plan: &Plan,
        index_map: Option<&DeltaSetIndexMap>,
        outer_map: &IncBiMap,
        inner_maps: &[IncBiMap],
        row_counts: &[u32],
    ) {
        // Both bit counts are those the new indices need, whatever the widths
        // of the source map were.
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/3225861de/src/hb-ot-var-hvar-table.hh#L127>
        // For the inner one HarfBuzz takes the new index of the largest old one
        // of each subtable, which isn't always its largest new one: the row that
        // stands for the ones a subtable doesn't have comes before the rows that
        // only the other maps name.
        self.outer_bit_count = 1;
        self.inner_bit_count = 1;

        for (new_gid, old_gid) in plan.new_to_old_gid_list.iter() {
            if new_gid.to_u32() >= self.map_count {
                break;
            }

            // new() read the same entries
            let v = delta_set_index(index_map, old_gid.to_u32(), row_counts).unwrap();

            let outer = v.outer;
            if outer as usize >= inner_maps.len() {
                continue;
            }

            // Every in-range entry was collected by new(), but don't rely on that
            // for a font's data: an entry with nothing to remap to is skipped.
            let (Some(new_outer), Some(new_inner)) = (
                outer_map.get(outer as u32),
                inner_maps[outer as usize].get(v.inner as u32),
            ) else {
                continue;
            };
            let outer_bits = (32 - new_outer.leading_zeros()).max(1) as u8;
            self.outer_bit_count = outer_bits.max(self.outer_bit_count);
            let inner_bits = (32 - new_inner.leading_zeros()) as u8;
            self.inner_bit_count = inner_bits.max(self.inner_bit_count);

            self.output_map
                .insert(new_gid.to_u32(), (*new_outer << 16) | *new_inner);
        }
    }

    fn is_identity(&self) -> bool {
        if self.output_map.is_empty() {
            return true;
        }
        // An advance map whose entries are the identity over every output
        // glyph is equivalent to implicit glyph-id indexing; drop it. A
        // truncated map doesn't qualify: glyphs past its end share its last
        // value, not their own ids.
        if !self.can_drop_to_implicit || self.map_count != self.full_map_count {
            return false;
        }
        (0..self.full_map_count).all(|i| self.output_map.get(&i).copied().unwrap_or(0) == i)
    }

    fn to_serialize_plan(&self) -> DeltaSetIndexMapSerializePlan<'_> {
        DeltaSetIndexMapSerializePlan::new(
            self.outer_bit_count,
            self.inner_bit_count,
            &self.output_map,
            self.map_count,
        )
    }
}

#[derive(Default)]
pub(crate) struct HvarVvarSubsetPlan {
    inner_maps: Vec<IncBiMap>,
    index_map_subset_plans: Vec<IndexMapSubsetPlan>,
}

impl HvarVvarSubsetPlan {
    pub(crate) fn new(
        plan: &Plan,
        var_store: &ItemVariationStore,
        index_maps: &[Option<DeltaSetIndexMap>],
    ) -> Result<Self, ReadError> {
        let mut this = HvarVvarSubsetPlan::default();
        // the rows that each subtable has: see delta_set_index
        let row_counts = delta_set_counts(var_store)?;
        let vardata_count = row_counts.len();
        // An empty store has no delta sets for any index map to refer to, so there
        // is nothing to remap: keep the store empty and drop every index map (a null
        // map means "no deltas", as does the empty store).
        // ref: <https://github.com/harfbuzz/harfbuzz/blob/75d8ac5d2313842550174eaab0cbaf653c8f2f77/src/hb-ot-var-hvar-table.hh#L239>
        if vardata_count == 0 {
            this.index_map_subset_plans
                .resize_with(index_maps.len(), Default::default);
            return Ok(this);
        }

        let mut inner_sets = Vec::new();
        inner_sets.resize(vardata_count, Default::default());

        let mut outer_map = IncBiMap::default();
        // don't bypass empty adv map
        let index_map_subset_plan = IndexMapSubsetPlan::new(
            index_maps[0].as_ref(),
            plan,
            false,
            &mut outer_map,
            &mut inner_sets,
            &row_counts,
        )?;
        this.index_map_subset_plans.push(index_map_subset_plan);

        let mut adv_set = IntSet::empty();
        if index_maps[0].is_none() {
            adv_set.union(&inner_sets[0]);
        }

        for index_map in index_maps.iter().skip(1) {
            this.index_map_subset_plans.push(IndexMapSubsetPlan::new(
                index_map.as_ref(),
                plan,
                true,
                &mut outer_map,
                &mut inner_sets,
                &row_counts,
            )?);
        }

        outer_map.sort();

        let retain_adv_map = index_maps[0].is_none()
            && plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS);

        let mut inner_map = if retain_adv_map {
            plan.new_to_old_gid_list
                .iter()
                .map(|(_, old_gid)| old_gid.to_u32().min(row_counts[0]))
                .collect::<IncBiMap>()
        } else {
            adv_set.iter().map(|g| g as u32).collect::<IncBiMap>()
        };
        // The other index maps (lsb/rsb, tsb/bsb/vorg) may refer to rows of the
        // first subtable too, whether or not the advance map is retained.
        inner_sets[0].subtract(&adv_set);
        for i in inner_sets[0].iter() {
            inner_map.add(i as u32);
        }
        this.inner_maps.push(inner_map);

        for set in inner_sets.iter().skip(1) {
            let inner_map = set.iter().map(|g| g as u32).collect();
            this.inner_maps.push(inner_map);
        }

        this.index_map_subset_plans
            .iter_mut()
            .zip(index_maps)
            .for_each(|(index_map_subset_plan, index_map)| {
                index_map_subset_plan.remap(
                    plan,
                    index_map.as_ref(),
                    &outer_map,
                    &this.inner_maps,
                    &row_counts,
                )
            });
        Ok(this)
    }

    pub(crate) fn inner_maps(&self) -> &[IncBiMap] {
        &self.inner_maps
    }

    pub(crate) fn index_map_subset_plans(&self) -> &[IndexMapSubsetPlan] {
        &self.index_map_subset_plans
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use read_fonts::{types::GlyphId, FontData, FontRead};

    #[test]
    fn test_subset_hvar_noop() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(2_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(3_u32)));

        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(3_u32));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        // Nothing is dropped, but the advance map is written with the entry
        // size its indices need: 1 byte instead of the 2 of the source.
        let expected_data: [u8; 94] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x04, 0x06, 0x05,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20,
            0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02,
            0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_hvar() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(3_u32)));

        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 91] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x03, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00,
            0x10, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20, 0x02,
            0x25, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_hvar_retain_gids() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(3_u32)));

        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));

        plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 92] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x02, 0x00, 0x03,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20,
            0x02, 0x25, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0xc0, 0x00,
            0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
            0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_identity_hvar_noop() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(2_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(3_u32)));

        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(3_u32));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        // Nothing is dropped, but the advance map is written with the entry
        // size its indices need: 1 byte instead of the 2 of the source.
        let expected_data: [u8; 94] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x04, 0x00, 0x04, 0x06, 0x05,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20,
            0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02,
            0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_identity_hvar() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(3_u32)));

        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 91] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1b, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x02, 0x03, 0x00,
            0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00,
            0x10, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20, 0x02,
            0x25, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    #[test]
    fn test_subset_identity_hvar_retain_gids() {
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x00, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(3_u32), GlyphId::from(3_u32)));

        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));

        plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let subsetted_data = s.copy_bytes();
        let expected_data: [u8; 92] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x02, 0x00, 0x03,
            0x00, 0x01, 0x00, 0x00, 0x00, 0x24, 0x00, 0x02, 0x00, 0x00, 0x00, 0x1e, 0x00, 0x00,
            0x00, 0x10, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0xfd, 0x20,
            0x02, 0x25, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x02, 0xc0, 0x00,
            0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00,
            0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        assert_eq!(subsetted_data, expected_data);
    }

    // An HVAR whose store has one subtable of two rows, with the deltas 10
    // and 11 for its only region, and an advance mapping of `entries`, if any.
    fn hvar_with_two_rows(entries: &[(u16, u16)]) -> Vec<u8> {
        use font_test_data::bebuffer::BeBuffer;

        let mut buf = BeBuffer::new()
            .extend([1_u16, 0])
            .push(20_u32)
            .push(if entries.is_empty() { 0_u32 } else { 52 })
            .extend([0_u32, 0])
            // ItemVariationStore, with its region list and its subtable
            .push(1_u16)
            .push(12_u32)
            .push(1_u16)
            .push(22_u32)
            .extend([1_u16, 1])
            .extend([0_i16, 0x4000, 0x4000])
            .extend([2_u16, 0, 1, 0])
            .extend([10_u8, 11]);
        if !entries.is_empty() {
            // entries of 4 bytes, 16 of their bits for the inner index
            buf = buf.extend([0_u8, 0x3F]).push(entries.len() as u16);
            for (outer, inner) in entries {
                buf = buf.extend([*outer, *inner]);
            }
        }
        buf.to_vec()
    }

    // Subsets `bytes` to the glyphs `gids`; returns the rows of the subtable
    // that is left and the advance delta of each glyph.
    fn rows_and_advance_deltas(bytes: &[u8], gids: &[u32], retain_gids: bool) -> (u16, Vec<i32>) {
        use read_fonts::types::F2Dot14;

        let hvar = Hvar::read(FontData::new(bytes)).unwrap();
        //dummy font
        let font = FontRef::new(bytes).unwrap();
        let mut plan = Plan::default();
        let new_gid = |i: usize| GlyphId::from(if retain_gids { gids[i] } else { i as u32 });
        for (i, gid) in gids.iter().enumerate() {
            plan.new_to_old_gid_list
                .push((new_gid(i), GlyphId::from(*gid)));
            plan.glyphset.insert(GlyphId::from(*gid));
        }
        if retain_gids {
            plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
        }

        let mut builder = FontBuilder::new();
        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        let out = s.copy_bytes();
        let out = Hvar::read(FontData::new(&out)).unwrap();
        let store = out.item_variation_store().unwrap();
        let rows = store.item_variation_data().get(0).unwrap().unwrap();
        let coords = [F2Dot14::from_f32(1.0)];
        let deltas = (0..gids.len())
            .map(|i| out.advance_delta(new_gid(i), &coords).unwrap().to_i32())
            .collect();
        (rows.item_count(), deltas)
    }

    /// A glyph can name a row that its subtable doesn't have. One row is kept
    /// for all of them, and they keep having no delta.
    #[test]
    fn test_subset_hvar_rows_past_the_store() {
        // without an advance mapping, glyph g has row g: only glyphs 0 and 1
        // have one
        let bytes = hvar_with_two_rows(&[]);
        for retain_gids in [false, true] {
            assert_eq!(
                rows_and_advance_deltas(&bytes, &[0, 1, 2, 3, 4], retain_gids),
                (3, vec![10, 11, 0, 0, 0])
            );
            assert_eq!(
                rows_and_advance_deltas(&bytes, &[0, 3, 4], retain_gids),
                (2, vec![10, 0, 0])
            );
            // nothing to fold: the rows are those of the glyphs
            assert_eq!(
                rows_and_advance_deltas(&bytes, &[0, 1], retain_gids),
                (2, vec![10, 11])
            );
        }

        // an advance mapping with entries past the rows
        let bytes = hvar_with_two_rows(&[(0, 1), (0, 7), (0, 0), (0, 0xFFFF), (0, 2)]);
        for retain_gids in [false, true] {
            assert_eq!(
                rows_and_advance_deltas(&bytes, &[0, 1, 2, 3, 4], retain_gids),
                (3, vec![11, 0, 10, 0, 0])
            );
            assert_eq!(
                rows_and_advance_deltas(&bytes, &[0, 1, 3], retain_gids),
                (2, vec![11, 0, 0])
            );
        }
    }

    #[test]
    fn test_subset_hvar_outer_index_out_of_range() {
        // test_subset_hvar's table, with .notdef's advance map entry
        // pointing at outer index 2 of a store with two variation datas.
        let raw_bytes: [u8; 98] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x11, 0x00, 0x04, 0x00, 0x08, 0x00, 0x04,
            0x00, 0x06, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x26, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00,
            0x00, 0x01, 0xfd, 0x20, 0x02, 0x25, 0x05, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x02, 0x00, 0x02, 0xc0, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();
        let mut builder = FontBuilder::new();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));
        plan.new_to_old_gid_list
            .push((GlyphId::from(2_u32), GlyphId::from(3_u32)));

        plan.glyphset.insert(GlyphId::NOTDEF);
        plan.glyphset.insert(GlyphId::from(1_u32));
        plan.glyphset.insert(GlyphId::from(3_u32));

        let mut s = Serializer::new(1024).unwrap();
        assert_eq!(s.start_serialize(), Ok(()));
        let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
        assert!(ret.is_ok());
        assert!(!s.in_error());
        s.end_serialize();

        // .notdef has no delta set to carry over; the other retained
        // glyphs keep theirs.
        let subsetted_data = s.copy_bytes();
        let subsetted = Hvar::read(FontData::new(&subsetted_data)).unwrap();
        let delta_set = |hvar: &Hvar, gid: u32| -> Option<Vec<i32>> {
            let index = hvar.advance_width_mapping()?.ok()?.get(gid).ok()?;
            let store = hvar.item_variation_store().ok()?;
            let data = store
                .item_variation_data()
                .get(index.outer as usize)?
                .ok()?;
            Some(data.delta_set(index.inner).collect())
        };
        assert_eq!(delta_set(&hvar, 0), None);
        assert_eq!(delta_set(&subsetted, 1), delta_set(&hvar, 1));
        assert_eq!(delta_set(&subsetted, 2), delta_set(&hvar, 3));
        assert_eq!(
            subsetted
                .item_variation_store()
                .unwrap()
                .item_variation_data_count(),
            1
        );
    }

    // An HVAR without an advance mapping, so glyph g uses row g of the only
    // subtable, whose 8 rows have the deltas 10 to 13 and 20 to 23. The lsb
    // mapping sends glyph g to row 4 + g of that same subtable, and the glyphs
    // from 3 on to row 7.
    fn hvar_no_adv_map_with_lsb_map() -> [u8; 66] {
        [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x3a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0c, 0x00, 0x01,
            0x00, 0x00, 0x00, 0x16, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00, 0x40, 0x00,
            0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x0a, 0x0b, 0x0c, 0x0d, 0x14, 0x15,
            0x16, 0x17, 0x00, 0x03, 0x00, 0x04, 0x04, 0x05, 0x06, 0x07,
        ]
    }

    #[test]
    fn test_subset_hvar_retain_gids_no_adv_map_with_lsb_map() {
        use read_fonts::types::F2Dot14;

        let raw_bytes = hvar_no_adv_map_with_lsb_map();
        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        for gids in [[0_u32, 1], [0, 2], [1, 3]] {
            let mut builder = FontBuilder::new();
            let mut plan = Plan::default();
            for gid in gids {
                plan.new_to_old_gid_list
                    .push((GlyphId::from(gid), GlyphId::from(gid)));
                plan.glyphset.insert(GlyphId::from(gid));
            }
            plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;

            let mut s = Serializer::new(1024).unwrap();
            assert_eq!(s.start_serialize(), Ok(()));
            let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
            assert!(ret.is_ok());
            assert!(!s.in_error());
            s.end_serialize();

            let subsetted_data = s.copy_bytes();
            let subsetted_hvar = Hvar::read(FontData::new(&subsetted_data)).unwrap();
            let coords = [F2Dot14::from_f32(1.0)];
            for gid in gids {
                let advance = subsetted_hvar.advance_delta(gid.into(), &coords).unwrap();
                assert_eq!(advance.to_i32(), 10 + gid as i32);
                let lsb = subsetted_hvar.lsb_delta(gid.into(), &coords).unwrap();
                assert_eq!(lsb.to_i32(), 20 + gid as i32);
            }
        }
    }

    /// The rows that only the lsb mapping names come after the rows of the
    /// glyphs, so the largest row that the lsb mapping names needn't be the one
    /// with the largest new index. Its entries hold the new indices all the same.
    #[test]
    fn test_subset_hvar_lsb_rows_after_a_larger_glyph_row() {
        use read_fonts::types::F2Dot14;

        let raw_bytes = hvar_no_adv_map_with_lsb_map();
        let hvar = Hvar::read(FontData::new(&raw_bytes)).unwrap();

        //dummy font
        let font = FontRef::new(&raw_bytes).unwrap();

        // Rows 0, 1, 2 and 7 of the glyphs become rows 0 to 3. The lsb rows 4, 5
        // and 6 of the first three glyphs follow as rows 4 to 6, which take a
        // bit more than row 3, the new index of the lsb row of glyph 7.
        let gids = [0_u32, 1, 2, 7];
        for retain_gids in [false, true] {
            let new_gid = |i: usize| GlyphId::from(if retain_gids { gids[i] } else { i as u32 });
            let mut plan = Plan::default();
            for (i, gid) in gids.iter().enumerate() {
                plan.new_to_old_gid_list
                    .push((new_gid(i), GlyphId::from(*gid)));
                plan.glyphset.insert(GlyphId::from(*gid));
            }
            if retain_gids {
                plan.subset_flags |= SubsetFlags::SUBSET_FLAGS_RETAIN_GIDS;
            }

            let mut builder = FontBuilder::new();
            let mut s = Serializer::new(1024).unwrap();
            assert_eq!(s.start_serialize(), Ok(()));
            let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
            assert!(ret.is_ok());
            assert!(!s.in_error());
            s.end_serialize();

            let subsetted_data = s.copy_bytes();
            let subsetted_hvar = Hvar::read(FontData::new(&subsetted_data)).unwrap();
            let coords = [F2Dot14::from_f32(1.0)];
            let deltas = |lsb: bool| -> Vec<i32> {
                (0..gids.len())
                    .map(|i| {
                        if lsb {
                            subsetted_hvar.lsb_delta(new_gid(i), &coords)
                        } else {
                            subsetted_hvar.advance_delta(new_gid(i), &coords)
                        }
                        .unwrap()
                        .to_i32()
                    })
                    .collect()
            };
            assert_eq!(deltas(false), [10, 11, 12, 23]);
            assert_eq!(deltas(true), [20, 21, 22, 23]);
        }
    }

    #[test]
    fn test_subset_hvar_empty_var_store() {
        // itemVariationDataCount = 0, no index maps
        let no_adv_map: [u8; 32] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ];
        // same store, with a one-entry advance width mapping
        let with_adv_map: [u8; 37] = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00,
        ];

        let mut plan = Plan::default();
        plan.new_to_old_gid_list
            .push((GlyphId::NOTDEF, GlyphId::NOTDEF));
        plan.new_to_old_gid_list
            .push((GlyphId::from(1_u32), GlyphId::from(1_u32)));

        plan.glyphset
            .insert_range(GlyphId::NOTDEF..=GlyphId::from(1_u32));

        for raw_bytes in [no_adv_map.as_slice(), with_adv_map.as_slice()] {
            let hvar = Hvar::read(FontData::new(raw_bytes)).unwrap();
            let mut builder = FontBuilder::new();

            //dummy font
            let font = FontRef::new(raw_bytes).unwrap();

            let mut s = Serializer::new(1024).unwrap();
            assert_eq!(s.start_serialize(), Ok(()));
            let ret = hvar.subset(&plan, &font, &mut s, &mut builder);
            assert!(ret.is_ok());
            assert!(!s.in_error());
            s.end_serialize();

            // the empty store is kept and the index maps are dropped
            let subsetted_data = s.copy_bytes();
            assert_eq!(subsetted_data, no_adv_map);
        }
    }
}
