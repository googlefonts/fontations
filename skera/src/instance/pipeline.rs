//! Axis state belongs to the subset plan; table serializers consume original data.
use super::{AxisPlan, FontContext, Instance};
use crate::{Plan, SubsetError, SubsetFlags};
use read_fonts::{types::Tag, FontRef, TableProvider};

pub(crate) struct InstancePlan {
    pub axes: AxisPlan,
    pub consumer_axes: AxisPlan,
    pub layout: super::layout::NativeLayoutPlan,
    pub color: super::color::NativeColorPlan,
    pub metadata: std::collections::BTreeMap<Tag, super::TableChange>,
}
impl InstancePlan {
    pub(crate) fn rewrites_outlines(&self) -> bool {
        !self.axes.coupled || !self.consumer_axes.values.is_empty()
    }
    pub(crate) fn cull_ranges(&self) -> Option<&[Option<(i16, i16)>]> {
        (self.axes.coupled && self.consumer_axes.values.is_empty()).then_some(&self.axes.reachable)
    }

    pub fn new(
        font: &FontRef,
        limits: &[super::AxisLimits],
        flags: SubsetFlags,
    ) -> Result<Self, SubsetError> {
        let axes = AxisPlan::new(font, limits)?;
        for tag in [
            b"GLYF", b"LOCA", b"GVAR", b"MAXP", b"HHEA", b"HMTX", b"VHEA", b"VMTX",
        ] {
            let tag = Tag::new(tag);
            if !axes.values.is_empty() && font.data_for_tag(tag).is_some() {
                return Err(SubsetError::InvalidAxis(format!(
                    "instancing the {tag} table is not supported"
                )));
            }
        }
        if font.glyf().is_err() && font.cff2().is_err() {
            return Err(SubsetError::InvalidAxis(
                "instancing requires CFF2 or TrueType outlines".into(),
            ));
        }
        let consumer_axes = if axes.coupled {
            axes.final_space(font)
        } else {
            axes.clone()
        };
        let layout = super::layout::NativeLayoutPlan::new(font, &consumer_axes)?;
        let mut metadata = super::FontOutput::default();
        metadata.finish_metadata(&mut FontContext {
            font,
            axes: &axes,
            flags,
        })?;
        Ok(Self {
            axes,
            consumer_axes,
            layout,
            color: Default::default(),
            metadata: metadata.changes,
        })
    }
    pub fn finish(&mut self, font: &FontRef, plan: &mut Plan) -> Result<(), SubsetError> {
        self.color = super::color::NativeColorPlan::new(font, &self.consumer_axes, plan)?;
        self.layout.finish(font, &self.consumer_axes, plan)
    }
}

/// Produce outline-dependent outputs directly in the subset's glyph order.
/// Layout and color remain borrowed and use their regular table serializers.
pub(crate) fn collect(
    font: &FontRef,
    plan: &Plan,
) -> Result<std::collections::BTreeMap<Tag, super::TableChange>, SubsetError> {
    let Some(instance) = &plan.instance else {
        return Ok(Default::default());
    };
    let mut output = super::FontOutput::default();
    let mut metrics = super::metrics::MetricTables::new(font);
    let context = FontContext {
        font,
        axes: &instance.consumer_axes,
        flags: plan.subset_flags,
    };
    if instance.consumer_axes.values.is_empty() && instance.axes.coupled {
        super::metrics::finish(font, &instance.consumer_axes, &mut metrics)?;
    } else if let Ok(glyf) = font.glyf() {
        let tables = glyf.instance(&mut super::truetype::TrueTypeContext {
            input: &context,
            plan,
            metrics: &mut metrics,
        })?;
        output.apply(Tag::new(b"glyf"), super::TableChange::Replace(tables.glyf));
        output.apply(Tag::new(b"loca"), super::TableChange::Replace(tables.loca));
        output.apply(Tag::new(b"gvar"), tables.gvar);
        output.apply(Tag::new(b"cvar"), tables.cvar);
        if let Some(bytes) = tables.cvt {
            output.apply(Tag::new(b"cvt "), super::TableChange::Replace(bytes));
        }
    } else {
        let data = crate::cff::instance_subset(font, plan)?;
        super::metrics::cff2_subset_metrics(font, &instance.consumer_axes, &mut metrics, plan)?;
        if instance.axes.all_pinned()
            && plan
                .subset_flags
                .contains(SubsetFlags::SUBSET_FLAGS_DOWNGRADE_CFF2)
        {
            let hmtx = metrics
                .hmtx
                .as_ref()
                .ok_or_else(|| SubsetError::SubsetTableError(Tag::new(b"hmtx")))?;
            let advances = hmtx
                .chunks_exact(4)
                .map(|v| u16::from_be_bytes([v[0], v[1]]))
                .collect::<Vec<_>>();
            let head = metrics
                .head
                .as_ref()
                .ok_or_else(|| SubsetError::SubsetTableError(Tag::new(b"head")))?;
            let bbox =
                std::array::from_fn(|i| i16::from_be_bytes([head[36 + i * 2], head[37 + i * 2]]));
            let data = crate::cff::downgrade_table(&data, &advances, bbox)?;
            output.apply(Tag::new(b"CFF2"), super::TableChange::Remove);
            output.apply(Tag::new(b"CFF "), super::TableChange::Replace(data));
        } else {
            output.apply(Tag::new(b"CFF2"), super::TableChange::Replace(data));
        }
    }
    if instance.axes.coupled {
        super::metrics::update_os2(&instance.axes, &mut metrics);
    }
    output.apply_all(metrics.into_changes());
    output.apply_all(instance.metadata.clone());
    Ok(output.changes)
}
