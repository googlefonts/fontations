//! OpenType layout.

use std::collections::HashSet;

#[cfg(feature = "parsing")]
use read_fonts::FontRead;

/// A macro to implement the [LookupType] trait.
macro_rules! lookup_type {
    ($ty:ty, $val:expr) => {
        impl LookupType for $ty {
            const TYPE: u16 = $val;
        }
    };
}

/// A macro to define a newtype around an exisitng table, that defers all
/// impls to that table.
///
/// We use this to ensure that shared lookup types (Sequence/Chain
/// lookups) can be given different lookup ids for each of GSUB/GPOS.
macro_rules! table_newtype {
    ($name:ident, $inner:ident, $read_type:path) => {
        #[derive(Debug, Clone)]
        pub struct $name($inner);

        impl std::ops::Deref for $name {
            type Target = $inner;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $name {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        impl FontWrite for $name {
            fn write_into(&self, writer: &mut TableWriter) {
                self.0.write_into(writer)
            }
        }

        impl Validate for $name {
            fn validate_impl(&self, ctx: &mut ValidationCtx) {
                self.0.validate_impl(ctx)
            }
        }

        #[cfg(feature = "parsing")]
        impl<'a> FromObjRef<$read_type> for $name {
            fn from_obj_ref(obj: &$read_type, _data: FontData) -> Self {
                Self(FromObjRef::from_obj_ref(obj, _data))
            }
        }

        #[cfg(feature = "parsing")]
        impl<'a> FromTableRef<$read_type> for $name {}
    };
}

pub mod gdef;
pub mod gpos;
pub mod gsub;

mod value_record;

#[cfg(test)]
#[path = "./tests/layout.rs"]
#[cfg(feature = "parsing")]
mod spec_tests;

include!("../generated/generated_layout.rs");

impl<T: LookupType + FontWrite> FontWrite for Lookup<T> {
    fn write_into(&self, writer: &mut TableWriter) {
        T::TYPE.write_into(writer);
        self.lookup_flag.write_into(writer);
        u16::try_from(self.subtable_offsets.len())
            .unwrap()
            .write_into(writer);
        self.subtable_offsets.write_into(writer);
        self.mark_filtering_set.write_into(writer);
    }
}

/// A utility trait for writing lookup tables.
///
/// This allows us to attach the numerical lookup type to the appropriate concrete
/// types, so that we can write it as needed without passing it around.
pub trait LookupType {
    /// The lookup type of this layout subtable.
    const TYPE: u16;
}

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
impl FromObjRef<read_fonts::layout::FeatureParams<'_>> for FeatureParams {
    fn from_obj_ref(from: &read_fonts::layout::FeatureParams, data: FontData) -> Self {
        use read_fonts::layout::FeatureParams as FromType;
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
impl FromTableRef<read_fonts::layout::FeatureParams<'_>> for FeatureParams {}

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
fn convert_delta_format(from: read_fonts::layout::DeltaFormat) -> DeltaFormat {
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
