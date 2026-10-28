//! Apply variation deltas while walking owned layout tables.
#[cfg(test)]
mod beyond64k_tests;
mod features;
mod owned;
#[cfg(test)]
mod tests;
mod values;

use super::{optimize, scalars::VariationScalars, AxisPlan, StorePlan};
use crate::SubsetError;
use features::{instance_features, own_feature_variations, remap_feature_conditions};
use owned::{own_gpos, own_gsub};
use read_fonts::{
    tables::variations::{DeltaSetIndex, ItemVariationStore},
    types::{Compatible, F2Dot14, Tag},
    FontRef, TableProvider,
};
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;
use values::{ApplyValues, ValuePass};
use write_fonts::{
    from_obj::ToOwnedTable,
    tables::{base::Base, gdef::Gdef, gpos::Gpos},
};

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
        // HarfBuzz evaluates an absent store or missing delta row as zero.
        // Subsetting can remove a store once all its residual regions vanish.
        Ok(self
            .store
            .as_ref()
            .zip(self.scalars())
            .and_then(|(store, scalars)| scalars.delta(store, DeltaSetIndex { outer, inner }))
            .map(|delta| (delta + 0.5).floor() as i64)
            .unwrap_or(0))
    }

    fn delta(&self, outer: u16, inner: u16) -> Result<i32, SubsetError> {
        Ok(self
            .wide_delta(outer, inner)?
            .clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }
}
fn rebuild_store(
    c: &LayoutContext,
) -> Result<Option<write_fonts::tables::variations::ItemVariationStore>, SubsetError> {
    use write_fonts::tables::variations::{RegionAxisCoordinates, VariationRegion};
    let err = || SubsetError::SubsetTableError(Tag::new(b"GDEF"));
    let Some(store) = &c.store else {
        return Ok(None);
    };
    let plan = c.store_plan()?.unwrap();
    let mut result = plan.rebuild(store)?;
    let rows = c.condition_rows.borrow();
    if rows.rows.is_empty() {
        return Ok(Some(result));
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
    Ok(Some(result))
}

impl super::Instance<LayoutContext<'_>> for read_fonts::tables::gpos::Gpos<'_> {
    type Output = Gpos;
    fn instance(&self, context: &mut LayoutContext<'_>) -> Result<Self::Output, SubsetError> {
        let gpos = self;

        let vars = own_feature_variations(
            gpos.feature_variations()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GPOS")))?,
            &gpos
                .feature_list()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GPOS")))?,
            Tag::new(b"GPOS"),
        )?;
        let mut table = own_gpos(gpos)?;
        table.feature_variations = vars.into();
        table.lookup_list.apply(&ValuePass::Instance(context))?;
        table.lookup_list2.apply(&ValuePass::Instance(context))?;
        instance_features(
            table
                .feature_list2
                .as_mut()
                .or(table.feature_list.as_mut())
                .ok_or(SubsetError::SubsetTableError(Tag::new(b"GSUB")))?,
            &mut table.feature_variations,
            context,
        )?;
        Ok(table)
    }
}

impl super::Instance<LayoutContext<'_>> for read_fonts::tables::gsub::Gsub<'_> {
    type Output = write_fonts::tables::gsub::Gsub;
    fn instance(&self, context: &mut LayoutContext<'_>) -> Result<Self::Output, SubsetError> {
        let gsub = self;

        let vars = own_feature_variations(
            gsub.feature_variations()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GSUB")))?,
            &gsub
                .feature_list()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GSUB")))?,
            Tag::new(b"GSUB"),
        )?;
        let mut table = own_gsub(gsub);
        table.feature_variations = vars.into();
        instance_features(
            table
                .feature_list2
                .as_mut()
                .or(table.feature_list.as_mut())
                .ok_or(SubsetError::SubsetTableError(Tag::new(b"GSUB")))?,
            &mut table.feature_variations,
            context,
        )?;
        Ok(table)
    }
}

impl super::Instance<LayoutContext<'_>> for read_fonts::tables::gdef::Gdef<'_> {
    type Output = Gdef;
    fn instance(&self, context: &mut LayoutContext<'_>) -> Result<Self::Output, SubsetError> {
        let mut table: Gdef = self.to_owned_table();
        if table.lig_caret_list2.is_some() {
            table.lig_caret_list = Default::default();
        }
        table.lig_caret_list.apply(&ValuePass::Instance(context))?;
        table.lig_caret_list2.apply(&ValuePass::Instance(context))?;
        Ok(table)
    }
}

impl super::Instance<super::FontContext<'_>> for read_fonts::tables::base::Base<'_> {
    type Output = Vec<u8>;
    fn instance(&self, input: &mut super::FontContext<'_>) -> Result<Self::Output, SubsetError> {
        let base = self;
        let axes = input.axes;

        let context = LayoutContext::new(
            base.item_var_store()
                .transpose()
                .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"BASE")))?,
            axes,
        );
        let mut table: Base = base.to_owned_table();
        table.horiz_axis.apply(&ValuePass::Instance(&context))?;
        table.vert_axis.apply(&ValuePass::Instance(&context))?;
        let store = if axes.all_pinned() {
            None
        } else {
            context
                .store
                .as_ref()
                .map(|s| context.store_plan()?.unwrap().rebuild(s))
                .transpose()?
        };
        if let Some(store) = store {
            let (store, map) = optimize::optimize(store)?;
            let remap = ValuePass::Remap(&map);
            table.horiz_axis.apply(&remap)?;
            table.vert_axis.apply(&remap)?;
            table.item_var_store = Some(store)
                .filter(|store| !store.item_variation_data.is_empty())
                .into();
        } else {
            table.item_var_store = Default::default();
        }
        serialize(b"BASE", &VersionedBase(&table, base.version()))
    }
}

#[derive(Default)]
pub(super) struct LayoutTables {
    pub gpos: Option<Vec<u8>>,
    pub gsub: Option<Vec<u8>>,
    pub gdef: Option<Vec<u8>>,
    pub base: Option<Vec<u8>>,
}

pub(super) fn instance_layout(
    font: &FontRef,
    axes: &AxisPlan,
) -> Result<LayoutTables, SubsetError> {
    use super::Instance;
    let mut output = LayoutTables::default();
    let mut gpos_table = None;
    let mut gsub_table = None;
    let gdef = font.gdef().ok();
    let mut context = LayoutContext::new(
        gdef.as_ref()
            .and_then(|g| g.item_var_store())
            .transpose()
            .map_err(|_| SubsetError::SubsetTableError(Tag::new(b"GDEF")))?,
        axes,
    );
    if let Ok(source) = font.gpos() {
        gpos_table = Some(source.instance(&mut context)?);
    }
    if let Ok(source) = font.gsub() {
        gsub_table = Some(source.instance(&mut context)?);
    }
    let gdef_table = gdef
        .as_ref()
        .map(|table| table.instance(&mut context))
        .transpose()?;
    let (store, map) = if axes.all_pinned() {
        (None, optimize::IndexMap::new())
    } else if let Some(store) = rebuild_store(&context)? {
        let (store, map) = optimize::optimize(store)?;
        (Some(store), map)
    } else {
        (None, optimize::IndexMap::new())
    };
    let remap_context = ValuePass::Remap(&map);
    if let Some(table) = &mut gpos_table {
        if !axes.all_pinned() {
            table.lookup_list.apply(&remap_context)?;
            table.lookup_list2.apply(&remap_context)?;
            remap_feature_conditions(&mut table.feature_variations, &map);
        }
        output.gpos = Some(serialize(b"GPOS", table)?);
    }
    if let Some(table) = &mut gsub_table {
        if !axes.all_pinned() {
            remap_feature_conditions(&mut table.feature_variations, &map);
        }
        output.gsub = Some(serialize(b"GSUB", table)?);
    }
    if let Some(mut table) = gdef_table {
        if !axes.all_pinned() {
            table.lig_caret_list.apply(&remap_context)?;
            table.lig_caret_list2.apply(&remap_context)?;
        }
        table.item_var_store = store
            .filter(|store| !store.item_variation_data.is_empty())
            .map(Into::into)
            .unwrap_or_default();
        output.gdef = Some(serialize(b"GDEF", &table)?);
    }
    if let Ok(base) = font.base() {
        let mut input = super::FontContext {
            font,
            axes,
            flags: crate::SubsetFlags::default(),
        };
        output.base = Some(base.instance(&mut input)?);
    }
    Ok(output)
}

// The owned BASE writer chooses a version from the presence of its store.
// HarfBuzz preserves the source version after dropping an instantiated store,
// including the null Offset32 field required by a BASE 1.1 header.
struct VersionedBase<'a>(&'a Base, read_fonts::types::MajorMinor);
impl write_fonts::FontWrite for VersionedBase<'_> {
    fn write_into(&self, writer: &mut write_fonts::TableWriter) {
        self.1.write_into(writer);
        self.0.horiz_axis.write_into(writer);
        self.0.vert_axis.write_into(writer);
        if self.1.compatible((1, 1)) {
            self.0.item_var_store.write_into(writer);
        }
    }
}
impl write_fonts::validate::Validate for VersionedBase<'_> {
    fn validate_impl(&self, ctx: &mut write_fonts::validate::ValidationCtx) {
        self.0.validate_impl(ctx);
    }
}

fn serialize(
    tag: &[u8; 4],
    table: &(impl write_fonts::FontWrite + write_fonts::validate::Validate),
) -> Result<Vec<u8>, SubsetError> {
    write_fonts::dump_table(table).map_err(|_| SubsetError::SubsetTableError(Tag::new(tag)))
}
