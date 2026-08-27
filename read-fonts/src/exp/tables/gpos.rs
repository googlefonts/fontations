//! GPOS, generated into the reworked framework.

use super::super::prelude::*;
use super::layout::{
    ClassDef, CoverageTable, DeviceOrVariationIndex, FeatureList, FeatureVariations, Lookup,
    ScriptList,
};

include!("../../../generated/exp/generated_gpos.rs");

/// A GPOS [SequenceContext](super::layout::SequenceContext).
pub type PositionSequenceContext<'a> = super::layout::SequenceContext<'a>;

/// A GPOS [ChainedSequenceContext](super::layout::ChainedSequenceContext).
pub type PositionChainContext<'a> = super::layout::ChainedSequenceContext<'a>;

/// A typed GPOS lookup list.
pub type PositionLookupList<'a> = super::layout::LookupList<'a, PositionLookup<'a>>;

/// A GPOS `ValueRecord`: `extern record ValueRecord` in the codegen input.
///
/// This is the record the whole rework is aimed at. Its size and contents come
/// from a `ValueFormat` held by an ancestor, and its device offsets are
/// measured from the enclosing subtable — so it is neither zerocopy nor
/// readable from data sliced to itself.
///
/// It needs no special case here: it is a `ComputedSize` record like any other,
/// a cursor holding the parent and its own position. Constructing one performs
/// no reads, so walking a `PairSet` to compare glyph ids never touches a value.
#[derive(Clone, Copy)]
pub struct ValueRecord<'a> {
    parent: Bytes<'a>,
    pos: usize,
    format: ValueFormat,
}

impl ComputedSize for ValueRecord<'_> {
    type Args = ValueFormat;

    #[inline]
    fn computed_size(format: ValueFormat) -> usize {
        format.bits().count_ones() as usize * u16::RAW_BYTE_LEN
    }
}

impl<'a> ArrayElement<'a> for ValueRecord<'a> {
    type Args = ValueFormat;
    type Store = StridedStore<'a>;
    type Output = Self;

    #[inline]
    fn read(store: StridedStore<'a>, item: usize, args: ValueFormat) -> Self {
        Self::at(store.data(), item, args)
    }
}

impl<'a> ValueRecord<'a> {
    /// Locates the record at `pos` bytes into `parent`. Performs no reads.
    #[inline]
    pub fn at(parent: Bytes<'a>, pos: usize, format: ValueFormat) -> Self {
        Self {
            parent,
            pos,
            format,
        }
    }

    pub fn format(&self) -> ValueFormat {
        self.format
    }

    /// The data this record's offsets are measured from.
    pub fn offset_data(&self) -> Bytes<'a> {
        self.parent
    }

    /// Fields are packed in flag order, so a field's position is the number of
    /// lower flags that are set.
    #[inline]
    fn field_pos(&self, flag: ValueFormat) -> Option<usize> {
        if !self.format.contains(flag) {
            return None;
        }
        let lower = self.format.bits() & (flag.bits() - 1);
        Some(self.pos + lower.count_ones() as usize * u16::RAW_BYTE_LEN)
    }

    #[inline]
    fn scalar(&self, flag: ValueFormat) -> Option<i16> {
        self.parent.read_at(self.field_pos(flag)?)
    }

    #[inline]
    fn device(&self, flag: ValueFormat) -> Option<DeviceOrVariationIndex<'a>> {
        let raw: Nullable<Offset16> = self.parent.read_at(self.field_pos(flag)?)?;
        // measured from the enclosing subtable, which is the data this record
        // was handed
        self.parent.resolve(raw)
    }

    pub fn x_placement(&self) -> Option<i16> {
        self.scalar(ValueFormat::X_PLACEMENT)
    }

    pub fn y_placement(&self) -> Option<i16> {
        self.scalar(ValueFormat::Y_PLACEMENT)
    }

    pub fn x_advance(&self) -> Option<i16> {
        self.scalar(ValueFormat::X_ADVANCE)
    }

    pub fn y_advance(&self) -> Option<i16> {
        self.scalar(ValueFormat::Y_ADVANCE)
    }

    pub fn x_placement_device(&self) -> Option<DeviceOrVariationIndex<'a>> {
        self.device(ValueFormat::X_PLACEMENT_DEVICE)
    }

    pub fn y_placement_device(&self) -> Option<DeviceOrVariationIndex<'a>> {
        self.device(ValueFormat::Y_PLACEMENT_DEVICE)
    }

    pub fn x_advance_device(&self) -> Option<DeviceOrVariationIndex<'a>> {
        self.device(ValueFormat::X_ADVANCE_DEVICE)
    }

    pub fn y_advance_device(&self) -> Option<DeviceOrVariationIndex<'a>> {
        self.device(ValueFormat::Y_ADVANCE_DEVICE)
    }
}

#[cfg(feature = "fast_sanitize")]
impl<'a> FastSanitize<'a> for ValueRecord<'a> {
    fn fast_sanitize_in(&self, ctx: &mut FastSanitizeContext) -> bool {
        if self.pos + Self::computed_size(self.format) > self.parent.len() {
            return false;
        }
        for flag in [
            ValueFormat::X_PLACEMENT_DEVICE,
            ValueFormat::Y_PLACEMENT_DEVICE,
            ValueFormat::X_ADVANCE_DEVICE,
            ValueFormat::Y_ADVANCE_DEVICE,
        ] {
            let Some(pos) = self.field_pos(flag) else {
                continue;
            };
            let raw: Nullable<Offset16> = self.parent.read_at(pos).unwrap_or_default();
            // always nullable in practice: the format says which fields exist,
            // and a zero means no device table
            if raw.offset().to_u32() == 0 {
                continue;
            }
            let Some(target) = self.device(flag) else {
                return false;
            };
            if !target.fast_sanitize_in(ctx) {
                return false;
            }
        }
        true
    }
}

#[cfg(feature = "sanitize")]
impl<'a> Sanitize<'a> for ValueRecord<'a> {
    const TYPE_NAME: &'static str = "ValueRecord";

    /// A value record's fields are located by a popcount over its format, so
    /// codegen cannot emit this; the checks are the same ones it would make.
    fn sanitize_in(&self, ctx: &mut SanitizeContext) {
        let end = self.pos + Self::computed_size(self.format);
        if end > self.parent.len() {
            ctx.report(Problem::FieldOutOfBounds {
                needed: end,
                available: self.parent.len(),
            });
        }
        for (name, flag) in [
            ("x_placement_device", ValueFormat::X_PLACEMENT_DEVICE),
            ("y_placement_device", ValueFormat::Y_PLACEMENT_DEVICE),
            ("x_advance_device", ValueFormat::X_ADVANCE_DEVICE),
            ("y_advance_device", ValueFormat::Y_ADVANCE_DEVICE),
        ] {
            let Some(pos) = self.field_pos(flag) else {
                continue;
            };
            let raw: Nullable<Offset16> = self.parent.read_at(pos).unwrap_or_default();
            let target = self.device(flag);
            // these offsets are always nullable in practice: a value record
            // says which fields it has, and a zero means no device table
            ctx.check_offset(name, raw.offset().to_u32(), target.is_some(), true);
            if let Some(target) = target {
                ctx.enter_field(name);
                target.sanitize_in(ctx);
                ctx.exit_field();
            }
        }
    }
}
