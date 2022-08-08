//! OpenType layout

use std::collections::HashSet;

#[cfg(feature = "parsing")]
use font_tables::FontRead;

pub mod gdef;
pub mod gpos;
mod value_record;

#[cfg(test)]
#[path = "./tests/layout.rs"]
mod spec_tests;

include!("../generated/layout.rs");

/// A lookup table that is generic over the lookup type.
#[derive(Debug, Clone)]
pub struct Lookup<T> {
    pub lookup_flag: u16,
    pub subtables: Vec<OffsetMarker<T>>,
    pub mark_filtering_set: u16,
}

impl<T: LookupType + FontWrite> FontWrite for Lookup<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        T::TYPE.write_into(writer);
        self.lookup_flag.write_into(writer);
        u16::try_from(self.subtables.len())
            .unwrap()
            .write_into(writer);
        self.subtables.write_into(writer);
        self.mark_filtering_set.write_into(writer);
    }
}

impl<T: Validate> Validate for Lookup<T> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_table("Lookup", |ctx| {
            ctx.in_field("subtables", |ctx| self.subtables.validate_impl(ctx))
        })
    }
}

/// An extension table that is generic over the subtable type.
#[derive(Debug, Clone)]
pub struct ExtensionSubtable<T> {
    pub extension_offset: OffsetMarker<T, 4>,
}

impl<T: LookupType + FontWrite> FontWrite for ExtensionSubtable<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        1u16.write_into(writer);
        T::TYPE.write_into(writer);
        self.extension_offset.write_into(writer);
    }
}

impl<T: Validate> Validate for ExtensionSubtable<T> {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        ctx.in_field("extension_offset", |ctx| {
            self.extension_offset.validate_impl(ctx)
        })
    }
}

#[cfg(feature = "parsing")]
impl<'a, U, T> FromObjRef<font_tables::layout::TypedLookup<'a, U>> for Lookup<T>
where
    U: FontRead<'a>,
    T: FromTableRef<U> + 'static,
{
    fn from_obj_ref(from: &font_tables::layout::TypedLookup<'a, U>, _data: &FontData) -> Self {
        Lookup {
            lookup_flag: from.lookup_flag(),
            mark_filtering_set: from.mark_filtering_set(),
            subtables: from
                .subtable_offsets()
                .iter()
                .map(|off| {
                    let table_ref = from.get_subtable(off.get());
                    table_ref.into()
                })
                .collect(),
        }
    }
}

#[cfg(feature = "parsing")]
impl<'a, U, T> FromObjRef<font_tables::layout::gpos::TypedExtension<'a, U>> for ExtensionSubtable<T>
where
    U: FontRead<'a>,
    T: FromTableRef<U> + 'static,
{
    fn from_obj_ref(
        from: &font_tables::layout::gpos::TypedExtension<'a, U>,
        _data: &FontData,
    ) -> Self {
        ExtensionSubtable {
            extension_offset: from.get().into(),
        }
    }
}

/// A utility trait for writing lookup tables
pub trait LookupType {
    /// The format type of this subtable.
    const TYPE: u16;
}

macro_rules! subtable_type {
    ($ty:ty, $val:expr) => {
        impl LookupType for $ty {
            const TYPE: u16 = $val;
        }
    };
}

subtable_type!(gpos::SinglePos, 1);
subtable_type!(gpos::PairPos, 2);
subtable_type!(gpos::CursivePosFormat1, 3);
subtable_type!(gpos::MarkBasePosFormat1, 4);
subtable_type!(gpos::MarkLigPosFormat1, 5);
subtable_type!(gpos::MarkMarkPosFormat1, 6);
subtable_type!(SequenceContext, 7);
subtable_type!(ChainedSequenceContext, 8);
subtable_type!(gpos::Extension, 9);

#[derive(Debug, Clone)]
pub enum FeatureParams {
    StylisticSet(StylisticSetParams),
    Size(SizeParams),
    CharacterVariant(CharacterVariantParams),
}

impl FontWrite for FeatureParams {
    fn write_into(&self, writer: &mut TableWriter) {
        match self {
            FeatureParams::StylisticSet(table) => table.write_into(writer),
            FeatureParams::Size(table) => table.write_into(writer),
            FeatureParams::CharacterVariant(table) => table.write_into(writer),
        }
    }
}

impl Validate for FeatureParams {
    fn validate_impl(&self, ctx: &mut ValidationCtx) {
        match self {
            Self::StylisticSet(table) => table.validate_impl(ctx),
            Self::Size(table) => table.validate_impl(ctx),
            Self::CharacterVariant(table) => table.validate_impl(ctx),
        }
    }
}

#[cfg(feature = "parsing")]
impl FromObjRef<font_tables::layout::FeatureParams<'_>> for FeatureParams {
    fn from_obj_ref(from: &font_tables::layout::FeatureParams, data: &FontData) -> Self {
        use font_tables::layout::FeatureParams as FromType;
        match from {
            FromType::Size(thing) => Self::Size(SizeParams::from_obj_ref(thing, data)),
            FromType::StylisticSet(thing) => {
                Self::StylisticSet(FromObjRef::from_obj_ref(thing, data))
            }
            FromType::CharacterVariant(thing) => {
                Self::CharacterVariant(FromObjRef::from_obj_ref(thing, data))
            }
        }
    }
}

#[cfg(feature = "parsing")]
impl FromTableRef<font_tables::layout::FeatureParams<'_>> for FeatureParams {}

impl ClassDefFormat1 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_value_array.iter().enumerate().map(|(i, cls)| {
            (
                GlyphId::new(self.start_glyph_id.to_u16().saturating_add(i as u16)),
                *cls,
            )
        })
    }
}

impl ClassRangeRecord {
    fn validate_glyph_range(&self, ctx: &mut ValidationCtx) {
        if self.start_glyph_id > self.end_glyph_id {
            ctx.report(format!(
                "start_glyph_id {} larger than end_glyph_id {}",
                self.start_glyph_id, self.end_glyph_id
            ));
        }
    }
}

impl ClassDefFormat2 {
    fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        self.class_range_records.iter().flat_map(|rcd| {
            (rcd.start_glyph_id.to_u16()..=rcd.end_glyph_id.to_u16())
                .map(|gid| (GlyphId::new(gid), rcd.class))
        })
    }
}

impl ClassDef {
    pub fn iter(&self) -> impl Iterator<Item = (GlyphId, u16)> + '_ {
        let (one, two) = match self {
            Self::Format1(table) => (Some(table.iter()), None),
            Self::Format2(table) => (None, Some(table.iter())),
        };

        one.into_iter().flatten().chain(two.into_iter().flatten())
    }

    pub fn class_count(&self) -> u16 {
        //TODO: implement a good integer set!!
        self.iter()
            .map(|(_gid, cls)| cls)
            .chain(std::iter::once(0))
            .collect::<HashSet<_>>()
            .len()
            .try_into()
            .unwrap()
    }
}

#[cfg(feature = "parsing")]
fn convert_delta_format(from: font_tables::layout::DeltaFormat) -> DeltaFormat {
    match from as u16 {
        0x0002 => DeltaFormat::Local4BitDeltas,
        0x0003 => DeltaFormat::Local8BitDeltas,
        0x8000 => DeltaFormat::VariationIndex,
        _ => DeltaFormat::Local2BitDeltas,
    }
}

impl Default for DeltaFormat {
    fn default() -> Self {
        Self::Local2BitDeltas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "array excedes max length")]
    fn array_len_smoke_test() {
        let table = ScriptList {
            script_records: vec![ScriptRecord {
                script_tag: Tag::new(b"hihi"),
                script_offset: OffsetMarker::new(Script {
                    default_lang_sys_offset: NullableOffsetMarker::new(None),
                    lang_sys_records: vec![LangSysRecord {
                        lang_sys_tag: Tag::new(b"coco"),
                        lang_sys_offset: OffsetMarker::new(LangSys {
                            required_feature_index: 0xffff,
                            feature_indices: vec![69; (u16::MAX) as usize + 5],
                        }),
                    }],
                }),
            }],
        };

        table.validate().unwrap();
    }

    #[test]
    #[should_panic(expected = "larger than end_glyph_id")]
    fn validate_classdef_ranges() {
        let classdef = ClassDefFormat2 {
            class_range_records: vec![ClassRangeRecord {
                start_glyph_id: GlyphId::new(12),
                end_glyph_id: GlyphId::new(3),
                class: 7,
            }],
        };

        classdef.validate().unwrap();
    }
}
