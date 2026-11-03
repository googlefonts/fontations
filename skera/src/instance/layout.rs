//! Plan layout deltas and residual rows for the borrowed subset serializers.
#[cfg(test)]
mod beyond64k_tests;
mod features;
mod native;
pub(crate) use native::NativeLayoutPlan;
#[cfg(test)]
#[path = "layout/test_helpers.rs"]
mod owned;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod values;

#[cfg(test)]
use super::optimize;
use super::{scalars::VariationScalars, AxisPlan, StorePlan};
use crate::SubsetError;
#[cfg(test)]
use features::{instance_features, own_feature_variations, remap_feature_conditions};
#[cfg(test)]
use owned::own_gpos;
use read_fonts::{
    tables::variations::{DeltaSetIndex, ItemVariationStore},
    types::{F2Dot14, Tag},
};
#[cfg(test)]
use read_fonts::{FontRef, TableProvider};
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
#[cfg(test)]
use values::{ApplyValues, ValuePass};
#[cfg(test)]
use write_fonts::from_obj::ToOwnedTable;

#[derive(Default)]
struct ConditionRows {
    rows: Vec<ConditionRow>,
    mapping: BTreeMap<(u32, i16), (u32, i16)>,
}
struct ConditionRow {
    indices: Vec<u16>,
    deltas: Vec<f64>,
    bias: Option<i32>,
}
struct LayoutContext<'a> {
    store: Option<ItemVariationStore<'a>>,
    coords: &'a [F2Dot14],
    axes: &'a AxisPlan,
    condition_rows: RefCell<ConditionRows>,
    scalars: OnceCell<Option<VariationScalars>>,
    store_plan: OnceCell<StorePlan>,
    constant_indices: RefCell<BTreeMap<u32, bool>>,
}
impl<'a> LayoutContext<'a> {
    fn new(store: Option<ItemVariationStore<'a>>, axes: &'a AxisPlan) -> Self {
        Self {
            store,
            coords: &axes.coords,
            axes,
            condition_rows: RefCell::default(),
            scalars: OnceCell::new(),
            store_plan: OnceCell::new(),
            constant_indices: RefCell::default(),
        }
    }
    fn scalars(&self) -> Option<&VariationScalars> {
        self.scalars
            .get_or_init(|| {
                self.store
                    .as_ref()
                    .and_then(|s| VariationScalars::new(s, self.coords))
            })
            .as_ref()
    }
    fn store_plan(&self) -> Result<Option<&StorePlan>, SubsetError> {
        let Some(store) = &self.store else {
            return Ok(None);
        };
        if self.store_plan.get().is_none() {
            let mut plan = StorePlan::new(store, self.axes)
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GDEF")))?;
            plan.preserve_original_region_order(store, self.axes)?;
            let _ = self.store_plan.set(plan);
        }
        Ok(self.store_plan.get())
    }
    fn condition_delta(&self, index: u32) -> f64 {
        if self.coords.iter().all(|&coord| coord == F2Dot14::ZERO) {
            return 0.;
        }
        self.store
            .as_ref()
            .zip(self.scalars())
            .and_then(|(store, scalars)| {
                scalars.condition_delta(
                    store,
                    DeltaSetIndex {
                        outer: (index >> 16) as u16,
                        inner: index as u16,
                    },
                )
            })
            .unwrap_or(0.)
    }
    fn index_is_constant(&self, index: u32) -> Result<bool, SubsetError> {
        if index == u32::MAX {
            return Ok(true);
        }
        if let Some(&constant) = self.constant_indices.borrow().get(&index) {
            return Ok(constant);
        }
        let Some(data) = self
            .store
            .as_ref()
            .and_then(|s| s.item_variation_data().get((index >> 16) as usize))
            .and_then(Result::ok)
        else {
            return Ok(true);
        };
        if index as u16 >= data.item_count() {
            return Ok(true);
        }
        let deltas = data
            .delta_set(index as u16)
            .map(|d| d as f64)
            .collect::<Vec<_>>();
        let transform = &self.store_plan()?.unwrap().transforms[(index >> 16) as usize];
        let constant = transform
            .residual(&deltas)?
            .iter()
            .all(|delta| (delta + 0.5).floor() == 0.);
        self.constant_indices.borrow_mut().insert(index, constant);
        Ok(constant)
    }
    fn wide_delta(&self, outer: u16, inner: u16) -> Result<i64, SubsetError> {
        if (DeltaSetIndex { outer, inner }) == DeltaSetIndex::NO_VARIATION_INDEX {
            return Ok(0);
        }
        if self.coords.iter().all(|&coord| coord == F2Dot14::ZERO) {
            return Ok(0);
        }
        // HarfBuzz evaluates an absent store or missing delta row as zero.
        // Subsetting can remove a store once all its residual regions vanish.
        Ok(self
            .store
            .as_ref()
            .zip(self.scalars())
            .and_then(|(store, scalars)| scalars.delta(store, DeltaSetIndex { outer, inner }))
            .map(|delta| super::round_f32(delta) as i64)
            .unwrap_or(0))
    }

    fn delta(&self, outer: u16, inner: u16) -> Result<i32, SubsetError> {
        Ok(self
            .wide_delta(outer, inner)?
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }
}
#[cfg(test)]
fn rebuild_store(
    c: &LayoutContext,
) -> Result<Option<write_fonts::tables::variations::ItemVariationStore>, SubsetError> {
    let Some(store) = &c.store else {
        return Ok(None);
    };
    let plan = c.store_plan()?.unwrap();
    let mut result = plan.rebuild(store)?;
    append_condition_rows(&mut result, &c.condition_rows.borrow())?;
    Ok(Some(result))
}
fn append_condition_rows(
    result: &mut write_fonts::tables::variations::ItemVariationStore,
    rows: &ConditionRows,
) -> Result<(), SubsetError> {
    use write_fonts::tables::variations::{RegionAxisCoordinates, VariationRegion};
    let err = || SubsetError::SubsetTableError(Tag::new(b"GDEF"));
    if rows.rows.is_empty() {
        return Ok(());
    }
    let regions = result.variation_region_list.as_mut();
    let neutral = if !rows.rows.iter().any(|row| row.bias.is_some()) {
        None
    } else if let Some(i) = regions
        .variation_regions
        .iter()
        .position(|r| r.region_axes.iter().all(|a| a.peak_coord == F2Dot14::ZERO))
    {
        Some(i as u16)
    } else {
        let index = regions.variation_regions.len();
        if index >= u16::MAX as usize {
            return Err(err());
        }
        regions.variation_regions.push(VariationRegion {
            region_axes: vec![
                RegionAxisCoordinates::new(
                    F2Dot14::ZERO,
                    F2Dot14::ZERO,
                    F2Dot14::ZERO
                );
                regions.axis_count as usize
            ],
        });
        Some(index as u16)
    };
    for row in &rows.rows {
        let mut region_indexes = row.indices.clone();
        let mut deltas = row.deltas.clone();
        if let Some(bias) = row.bias {
            region_indexes.push(neutral.unwrap());
            deltas.push(bias as f64);
        }
        result
            .item_variation_data
            .push(super::store::encode_rows(&region_indexes, &[deltas])?.into());
    }
    Ok(())
}

#[cfg(test)]
#[derive(Default)]
pub(super) struct LayoutTables {
    pub gpos: Option<Vec<u8>>,
}
#[cfg(test)]
pub(super) fn instance_layout(
    font: &read_fonts::FontRef,
    axes: &AxisPlan,
) -> Result<LayoutTables, SubsetError> {
    use read_fonts::TableProvider;
    let layout = NativeLayoutPlan::new(font, axes)?;
    let instance = Box::new(super::pipeline::InstancePlan {
        axes: axes.clone(),
        consumer_axes: axes.clone(),
        layout,
        color: Default::default(),
        metadata: Default::default(),
    });
    let mut plan = crate::Plan::new_with_instance(
        &read_fonts::collections::IntSet::all(),
        &read_fonts::collections::IntSet::all(),
        font,
        crate::SubsetFlags::KEEP_EVERYTHING,
        &read_fonts::collections::IntSet::empty(),
        &read_fonts::collections::IntSet::all(),
        &read_fonts::collections::IntSet::all(),
        &read_fonts::collections::IntSet::all(),
        &read_fonts::collections::IntSet::all(),
        Some(instance),
        true,
    );
    let mut instance = plan.instance.take().unwrap();
    instance.finish(font, &mut plan)?;
    plan.instance = Some(instance);
    let bytes = crate::subset_font(font, &plan)?;
    let font = read_fonts::FontRef::new(&bytes).unwrap();
    let table = |tag| {
        font.data_for_tag(Tag::new(tag))
            .map(|d| d.as_bytes().to_vec())
    };
    Ok(LayoutTables {
        gpos: table(b"GPOS"),
    })
}
