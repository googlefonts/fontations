//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

#[path = "../../generated/generated_gdef.rs"]
mod generated;

pub use generated::*;

#[cfg(feature = "compile")]
use crate::{
    compile::{ToOwnedImpl, ToOwnedTable},
    layout::CoverageTable,
};
#[cfg(feature = "compile")]
use font_types::Offset;

use crate::layout::ClassDef;
use font_types::{OffsetHost, Tag};

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
    type Owned = compile::Gdef;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        compile::Gdef::new(
            offset_data,
            self.glyph_class_def_offset(),
            self.attach_list_offset(),
            self.lig_caret_list_offset(),
            self.mark_attach_class_def_offset(),
            None,
            None,
        )
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for Gdef1_2<'_> {
    type Owned = compile::Gdef;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        compile::Gdef::new(
            offset_data,
            self.glyph_class_def_offset(),
            self.attach_list_offset(),
            self.lig_caret_list_offset(),
            self.mark_attach_class_def_offset(),
            Some(self.mark_glyph_sets_def_offset()),
            None,
        )
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for Gdef1_3<'_> {
    type Owned = compile::Gdef;
    fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
        let offset_data = self.bytes();
        compile::Gdef::new(
            offset_data,
            self.glyph_class_def_offset(),
            self.attach_list_offset(),
            self.lig_caret_list_offset(),
            self.mark_attach_class_def_offset(),
            Some(self.mark_glyph_sets_def_offset()),
            Some(self.item_var_store_offset()),
        )
    }
}

#[cfg(feature = "compile")]
impl ToOwnedImpl for Gdef<'_> {
    type Owned = compile::Gdef;

    fn to_owned_impl(&self, data: &[u8]) -> Option<Self::Owned> {
        match self {
            Self::Gdef1_0(t) => t.to_owned_impl(data),
            Self::Gdef1_2(t) => t.to_owned_impl(data),
            Self::Gdef1_3(t) => t.to_owned_impl(data),
        }
    }
}

#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_0<'_> {}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_2<'_> {}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_3<'_> {}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef<'_> {}

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

#[cfg(feature = "compile")]
impl ToOwnedTable for AttachList<'_> {}

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
impl ToOwnedTable for LigCaretList<'_> {}

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

#[cfg(feature = "compile")]
impl ToOwnedTable for MarkGlyphSets<'_> {}
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

//#[cfg(feature = "compile")]
//impl ToOwnedImpl for AttachPoint<'_> {
//type Owned = compile::AttachPoint;

//fn to_owned_impl(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
//let point_indices = self.point_indices().iter().map(|idx| idx.get()).collect();
//Some(compile::AttachPoint { point_indices })
//}
//}

#[cfg(feature = "compile")]
mod compile {

    use std::collections::BTreeMap;

    use crate::compile::{Table, TableWriter, ToOwnedImpl};
    use crate::layout::compile::{ClassDef, CoverageTable};
    use font_types::{FontRead, GlyphId, Offset, Offset16, Offset32};

    pub struct Gdef {
        pub glyph_class_def: Option<ClassDef>,
        pub attach_list: Option<AttachList>,
        pub lig_caret_list: Option<LigCaretList>,
        pub mark_attach_class_def: Option<ClassDef>,
        pub mark_glyph_sets_def: Option<MarkGlyphSets>,
        //pub item_var_store: Option<ItemVariationStore>,
    }

    impl Gdef {
        pub fn new(
            bytes: &[u8],
            glyph_class_off: Offset16,
            attach_list_off: Offset16,
            lig_caret_off: Offset16,
            mark_attach_off: Offset16,
            mark_glyph_sets_off: Option<Offset16>,
            _item_var_store_off: Option<Offset32>,
        ) -> Option<Self> {
            let glyph_class_def = resolve_owned::<_, super::ClassDef>(glyph_class_off, bytes);
            let attach_list = resolve_owned::<_, super::AttachList>(attach_list_off, bytes);
            let lig_caret_list = resolve_owned::<_, super::LigCaretList>(lig_caret_off, bytes);
            let mark_attach_class_def = resolve_owned::<_, super::ClassDef>(mark_attach_off, bytes);
            let mark_glyph_sets_def = mark_glyph_sets_off
                .and_then(|off| resolve_owned::<_, super::MarkGlyphSets>(off, bytes));

            Some(Gdef {
                glyph_class_def,
                attach_list,
                lig_caret_list,
                mark_attach_class_def,
                mark_glyph_sets_def,
            })
        }
    }

    fn resolve_owned<'a, O: Offset, T: FontRead<'a> + ToOwnedImpl>(
        off: O,
        bytes: &'a [u8],
    ) -> Option<T::Owned> {
        off.read::<T>(bytes).and_then(|t| t.to_owned_impl(bytes))
    }

    //pub struct Gdef1_0 {
    //pub glyph_class_def: Option<ClassDef>,
    //pub attach_list: Option<AttachList>,
    //pub lig_caret_list: Option<LigCaretList>,
    //pub mark_attach_class_def: Option<ClassDef>,
    //}

    //struct Gdef1_2 {
    //glyph_class_def: Option<ClassDef>,
    //attach_list: Option<AttachList>,
    //lig_caret_list: Option<LigCaretList>,
    //mark_attach_class_def: Option<ClassDef>,
    //mark_glyph_sets_def: Option<MarkGlyphSets>,
    //}

    //struct Gdef1_3 {
    //glyph_class_def: Option<ClassDef>,
    //attach_list: Option<AttachList>,
    //lig_caret_list: Option<LigCaretList>,
    //mark_attach_class_def: Option<ClassDef>,
    //mark_glyph_sets_def: Option<MarkGlyphSets>,
    //item_var_store: Option<ItemVariationStore>,
    //}

    impl Table for Gdef {
        fn describe(&self, writer: &mut TableWriter) {
            // version
            let minor_version = if self.mark_glyph_sets_def.is_some() {
                2
            } else {
                0
            };
            writer.write([1u16, minor_version].as_slice());
            match &self.glyph_class_def {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => writer.write(0u16),
            }

            match &self.attach_list {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => writer.write(0u16),
            }

            match &self.lig_caret_list {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => writer.write(0u16),
            }

            match &self.mark_attach_class_def {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => writer.write(0u16),
            }

            if minor_version == 2 {
                match &self.mark_attach_class_def {
                    Some(obj) => writer.write_offset::<Offset16>(obj),
                    None => writer.write(0u16),
                }
            }
        }
    }

    pub struct AttachList {
        pub items: BTreeMap<GlyphId, Vec<u16>>,
    }

    impl Table for AttachList {
        fn describe(&self, writer: &mut TableWriter) {
            let coverage = self.items.keys().copied().collect::<CoverageTable>();
            writer.write_offset::<Offset16>(&coverage);
            writer.write(self.items.len() as u16);
            for points in self.items.values() {
                writer.write_offset::<Offset16>(&AttachPointTemp { points })
            }
        }
    }

    struct AttachPointTemp<'a> {
        points: &'a [u16],
    }

    impl Table for AttachPointTemp<'_> {
        fn describe(&self, writer: &mut TableWriter) {
            writer.write(self.points.len() as u16);
            writer.write(self.points);
        }
    }

    ///// this is more 'accurate' and easier to derive, but annoying to use
    //pub struct AttachListAlt {
    //coverage: CoverageTable,
    //attach_points: Vec<AttachPoint>,
    //}

    //pub struct AttachPoint {
    //pub point_indices: Vec<u16>,
    //}

    pub struct LigCaretList {
        pub items: BTreeMap<GlyphId, Vec<CaretValue>>,
    }

    impl Table for LigCaretList {
        fn describe(&self, writer: &mut TableWriter) {
            let coverage = self.items.keys().copied().collect::<CoverageTable>();
            writer.write_offset::<Offset16>(&coverage);
            writer.write(self.items.len() as u16);
            for carets in self.items.values() {
                writer.write_offset::<Offset16>(&LigGlyphTemp { carets });
            }
        }
    }

    struct LigGlyphTemp<'a> {
        carets: &'a [CaretValue],
    }

    impl Table for LigGlyphTemp<'_> {
        fn describe(&self, writer: &mut TableWriter) {
            writer.write(self.carets.len() as u16);
            for caret in self.carets {
                writer.write_offset::<Offset16>(caret);
            }
        }
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
            device: Box<dyn Table>,
        },
    }

    impl Table for CaretValue {
        fn describe(&self, writer: &mut TableWriter) {
            match self {
                Self::Format1 { coordinate } => {
                    writer.write(1u16);
                    writer.write(*coordinate);
                }
                Self::Format2 {
                    caret_value_point_index,
                } => {
                    writer.write(2u16);
                    writer.write(*caret_value_point_index);
                }
                Self::Format3 { coordinate, device } => {
                    writer.write(3u16);
                    writer.write(*coordinate);
                    writer.write_offset::<Offset16>(device);
                }
            }
        }
    }
}
