//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

#[path = "../../generated/generated_gdef.rs"]
mod generated;

pub use generated::*;

#[cfg(feature = "compile")]
use crate::compile::ToOwnedImpl;

use crate::layout::{ClassDef, CoverageTable};
use font_types::{Offset, OffsetHost, Tag};

/// 'GDEF'
pub const TAG: Tag = Tag::new(b"GDEF");

impl<'a> Gdef<'a> {
    pub fn glyph_class_def(&self) -> Option<ClassDef> {
        self.resolve_offset(self.glyph_class_def_offset())
    }

    pub fn attach_list(&self) -> Option<AttachList> {
        self.resolve_offset(self.attach_list_offset())
    }

    pub fn lig_caret_list(&self) -> Option<LigCaretList> {
        self.resolve_offset(self.lig_caret_list_offset())
    }

    pub fn mark_attach_class_def(&self) -> Option<ClassDef> {
        self.resolve_offset(self.mark_attach_class_def_offset())
    }

    pub fn mark_glyph_sets_def(&self) -> Option<MarkGlyphSets> {
        self.mark_glyph_sets_def_offset()
            .and_then(|off| self.resolve_offset(off))
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for Gdef1_0<'_> {
    type Owned = compile::Gdef1_0;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        let glyph_class_def = self
            .glyph_class_def_offset()
            .read::<ClassDef>(offset_data)
            .and_then(|t| t.to_owned_impl(offset_data));

        let attach_list = self
            .glyph_class_def_offset()
            .read::<AttachList>(offset_data)
            .and_then(|t| t.to_owned_impl(offset_data));

        let lig_caret_list = self
            .glyph_class_def_offset()
            .read::<LigCaretList>(offset_data)
            .and_then(|t| t.to_owned_impl(offset_data));

        let mark_attach_class_def = self
            .glyph_class_def_offset()
            .read::<ClassDef>(offset_data)
            .and_then(|t| t.to_owned_impl(offset_data));

        Some(compile::Gdef1_0 {
            glyph_class_def,
            attach_list,
            lig_caret_list,
            mark_attach_class_def,
        })
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for AttachList<'_> {
    type Owned = compile::AttachList;

    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        let coverage = self.coverage_offset().read::<CoverageTable>(offset_data)?;

        let attach_points = self.attach_point_offsets().iter().map(|off| {
            off.get()
                .read::<AttachPoint>(offset_data)
                .map(|x| {
                    x.point_indices()
                        .iter()
                        .map(|pt| pt.get())
                        .collect::<Vec<_>>()
                })
                .expect("invalid offset in AttachList")
        });
        Some(compile::AttachList {
            items: coverage.iter().zip(attach_points).collect(),
        })
    }
}

impl LigGlyph<'_> {
    fn caret_values(&self) -> impl Iterator<Item = CaretValue> + '_ {
        //FIXME: this flat_map call silently discards invalid tables
        self.caret_value_offsets()
            .iter()
            .flat_map(|off| self.resolve_offset(off.get()))
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for LigCaretList<'_> {
    type Owned = compile::LigCaretList;

    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        let coverage = self.coverage_offset().read::<CoverageTable>(offset_data)?;

        let attach_points = self.lig_glyph_offsets().iter().map(|off| {
            off.get()
                .read::<LigGlyph>(offset_data)
                .map(|x| {
                    x.caret_values()
                        .flat_map(|car| car.to_owned_impl(x.bytes()))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        });
        Some(compile::LigCaretList {
            items: coverage.iter().zip(attach_points).collect(),
        })
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for CaretValue {
    type Owned = compile::CaretValue;

    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let caret = match self {
            CaretValue::Format1(caret) => compile::CaretValue::Format1 {
                coordinate: caret.coordinate(),
            },
            CaretValue::Format2(caret) => compile::CaretValue::Format2 {
                caret_value_point_index: caret.caret_value_point_index(),
            },
            CaretValue::Format3(_) => return None,
        };
        Some(caret)
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for MarkGlyphSets<'_> {
    type Owned = compile::MarkGlyphSets;

    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let tables = self
            .coverage_offsets()
            .iter()
            .flat_map(|off| {
                off.get()
                    .read::<CoverageTable>(self.bytes())
                    .and_then(|cov| cov.to_owned_impl(self.bytes()))
            })
            .collect::<Vec<_>>();
        Some(compile::MarkGlyphSets { tables })
    }
}

//impl ToOwnedImpl for AttachList<'_> {
//type Owned = compile::AttachListAlt;

//fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
//let offset_data = self.bytes();
//let coverage = self
//.coverage_offset()
//.read::<CoverageTable>(offset_data)?
//.to_owned_impl(offset_data)?;
//let attach_points = self.attach_point_offsets().iter().map(|off| {
//off.get()
//.read::<AttachPoint>(offset_data)
//.and_then(|x| x.to_owned_impl(offset_data))
//.expect("invalid offset in AttachList")
//});
//Some(compile::AttachList {
//coverage,
//attach_points: attach_points.collect(),
//})
//}
//}

#[cfg(feature = "compile")]
impl ToOwnedImpl for AttachPoint<'_> {
    type Owned = compile::AttachPoint;

    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let point_indices = self.point_indices().iter().map(|idx| idx.get()).collect();
        Some(compile::AttachPoint { point_indices })
    }
}

#[cfg(feature = "compile")]
mod compile {

    use std::collections::BTreeMap;

    use crate::layout::compile::{ClassDef, CoverageTable};
    use font_types::GlyphId;

    pub struct Gdef1_0 {
        pub glyph_class_def: Option<ClassDef>,
        pub attach_list: Option<AttachList>,
        pub lig_caret_list: Option<LigCaretList>,
        pub mark_attach_class_def: Option<ClassDef>,
    }

    struct Gdef1_2 {
        glyph_class_def: Option<ClassDef>,
        attach_list: Option<AttachList>,
        lig_caret_list: Option<LigCaretList>,
        mark_attach_class_def: Option<ClassDef>,
        mark_glyph_sets_def: Option<MarkGlyphSets>,
    }

    //struct Gdef1_3 {
    //glyph_class_def: Option<ClassDef>,
    //attach_list: Option<AttachList>,
    //lig_caret_list: Option<LigCaretList>,
    //mark_attach_class_def: Option<ClassDef>,
    //mark_glyph_sets_def: Option<MarkGlyphSets>,
    //item_var_store: Option<ItemVariationStore>,
    //}

    pub struct AttachList {
        pub items: BTreeMap<GlyphId, Vec<u16>>,
    }

    ///// this is more 'accurate' and easier to derive, but annoying to use
    //pub struct AttachListAlt {
    //coverage: CoverageTable,
    //attach_points: Vec<AttachPoint>,
    //}

    pub struct AttachPoint {
        pub point_indices: Vec<u16>,
    }

    pub struct LigCaretList {
        pub items: BTreeMap<GlyphId, Vec<CaretValue>>,
    }

    //struct LigCaretList {
    //coverage: CoverageTable,
    //lig_glyphs: Vec<LigGlyph>,
    //}

    //struct LigGlyph {
    //caret_value_offsets: Vec<CaretValue>,
    //}

    pub struct MarkGlyphSets {
        pub tables: Vec<CoverageTable>,
    }

    //impl LigCaretList {
    //pub fn add_lig_glyph(&mut self, glyph: GlyphId, caret_value_offsets: Vec<CaretValue>) {
    //self.glyphs.insert(
    //glyph,
    //LigGlyph {
    //caret_value_offsets,
    //},
    //);
    //}
    //}

    pub enum CaretValue {
        Format1 {
            coordinate: i16,
        },
        Format2 {
            caret_value_point_index: u16,
        },
        Format3 {
            coordinate: i16,
            //device: Box<dyn Table>,
        },
    }
}
