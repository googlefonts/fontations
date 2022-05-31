//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

#[path = "../../generated/generated_gdef.rs"]
mod generated;

pub use generated::*;

#[cfg(feature = "compile")]
use crate::{
    compile::{FromObjRef, ToOwnedTable},
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
impl FromObjRef<Gdef1_0<'_>> for compile::Gdef {
    fn from_obj(obj: &Gdef1_0<'_>, _offset_data: &[u8]) -> Option<Self> {
        let offset_data = obj.bytes();
        compile::Gdef::new(
            offset_data,
            obj.glyph_class_def_offset(),
            obj.attach_list_offset(),
            obj.lig_caret_list_offset(),
            obj.mark_attach_class_def_offset(),
            None,
            None,
        )
    }
}

#[cfg(feature = "compile")]
impl FromObjRef<Gdef1_2<'_>> for compile::Gdef {
    fn from_obj(obj: &Gdef1_2<'_>, _offset_data: &[u8]) -> Option<Self> {
        let offset_data = obj.bytes();
        compile::Gdef::new(
            offset_data,
            obj.glyph_class_def_offset(),
            obj.attach_list_offset(),
            obj.lig_caret_list_offset(),
            obj.mark_attach_class_def_offset(),
            Some(obj.mark_glyph_sets_def_offset()),
            None,
        )
    }
}

#[cfg(feature = "compile")]
impl FromObjRef<Gdef1_3<'_>> for compile::Gdef {
    fn from_obj(obj: &Gdef1_3, _offset_data: &[u8]) -> Option<Self> {
        let offset_data = obj.bytes();
        compile::Gdef::new(
            offset_data,
            obj.glyph_class_def_offset(),
            obj.attach_list_offset(),
            obj.lig_caret_list_offset(),
            obj.mark_attach_class_def_offset(),
            Some(obj.mark_glyph_sets_def_offset()),
            Some(obj.item_var_store_offset()),
        )
    }
}

#[cfg(feature = "compile")]
impl FromObjRef<Gdef<'_>> for compile::Gdef {
    fn from_obj(obj: &Gdef<'_>, offset_data: &[u8]) -> Option<Self> {
        match obj {
            Gdef::Gdef1_0(t) => Self::from_obj(t, offset_data),
            Gdef::Gdef1_2(t) => Self::from_obj(t, offset_data),
            Gdef::Gdef1_3(t) => Self::from_obj(t, offset_data),
        }
    }
}

#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_0<'_> {
    type Owned = compile::Gdef;
}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_2<'_> {
    type Owned = compile::Gdef;
}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef1_3<'_> {
    type Owned = compile::Gdef;
}
#[cfg(feature = "compile")]
impl ToOwnedTable for Gdef<'_> {
    type Owned = compile::Gdef;
}

#[cfg(feature = "compile")]
impl FromObjRef<AttachList<'_>> for compile::AttachList {
    fn from_obj(obj: &AttachList<'_>, _offset_data: &[u8]) -> Option<Self> {
        let offset_data = obj.bytes();
        let coverage = obj.coverage_offset().read::<CoverageTable>(offset_data)?;

        let attach_points = obj.attach_point_offsets().iter().map(|off| {
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
impl ToOwnedTable for AttachList<'_> {
    type Owned = compile::AttachList;
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
impl FromObjRef<LigCaretList<'_>> for compile::LigCaretList {
    fn from_obj(obj: &LigCaretList<'_>, _offset_data: &[u8]) -> Option<Self> {
        let offset_data = obj.bytes();
        let coverage = obj.coverage_offset().read::<CoverageTable>(offset_data)?;

        let attach_points = obj.lig_glyph_offsets().iter().map(|off| {
            off.get()
                .read::<LigGlyph>(offset_data)
                .map(|x| {
                    x.caret_values()
                        .flat_map(|car| FromObjRef::from_obj(&car, x.bytes()))
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
impl ToOwnedTable for LigCaretList<'_> {
    type Owned = compile::LigCaretList;
}

#[cfg(feature = "compile")]
impl FromObjRef<CaretValue<'_>> for compile::CaretValue {
    fn from_obj(obj: &CaretValue<'_>, _offset_data: &[u8]) -> Option<Self> {
        let caret = match obj {
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
impl FromObjRef<MarkGlyphSets<'_>> for compile::MarkGlyphSets {
    fn from_obj(obj: &MarkGlyphSets<'_>, _offset_data: &[u8]) -> Option<Self> {
        let tables = obj
            .coverage_offsets()
            .iter()
            .flat_map(|off| {
                off.get()
                    .read::<CoverageTable>(obj.bytes())
                    .and_then(|cov| FromObjRef::from_obj(&cov, obj.bytes()))
            })
            .collect::<Vec<_>>();
        Some(compile::MarkGlyphSets { tables })
    }
}

#[cfg(feature = "compile")]
impl ToOwnedTable for MarkGlyphSets<'_> {
    type Owned = compile::MarkGlyphSets;
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

    use crate::compile::{FontWrite, FromObjRef, OffsetMarker32, TableWriter};
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
            let glyph_class_def = resolve_owned::<_, super::ClassDef, _>(glyph_class_off, bytes);
            let attach_list = resolve_owned::<_, super::AttachList, _>(attach_list_off, bytes);
            let lig_caret_list = resolve_owned::<_, super::LigCaretList, _>(lig_caret_off, bytes);
            let mark_attach_class_def =
                resolve_owned::<_, super::ClassDef, _>(mark_attach_off, bytes);
            let mark_glyph_sets_def = mark_glyph_sets_off
                .and_then(|off| resolve_owned::<_, super::MarkGlyphSets, _>(off, bytes));

            Some(Gdef {
                glyph_class_def,
                attach_list,
                lig_caret_list,
                mark_attach_class_def,
                mark_glyph_sets_def,
            })
        }
    }

    fn resolve_owned<'a, O, A, B>(off: O, bytes: &'a [u8]) -> Option<B>
    where
        O: Offset,
        A: FontRead<'a>,
        B: FromObjRef<A>,
    {
        let obj: A = off.read(bytes)?;
        B::from_obj(&obj, bytes)
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

    impl FontWrite for Gdef {
        fn write_into(&self, writer: &mut TableWriter) {
            // version
            let minor_version = if self.mark_glyph_sets_def.is_some() {
                2u16
            } else {
                0
            };
            1u16.write_into(writer);
            minor_version.write_into(writer);
            match &self.glyph_class_def {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => 0u16.write_into(writer),
            }

            match &self.attach_list {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => 0u16.write_into(writer),
            }

            match &self.lig_caret_list {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => 0u16.write_into(writer),
            }

            match &self.mark_attach_class_def {
                Some(obj) => writer.write_offset::<Offset16>(obj),
                None => 0u16.write_into(writer),
            }

            if minor_version >= 2 {
                match &self.mark_glyph_sets_def {
                    Some(obj) => writer.write_offset::<Offset16>(obj),
                    None => 0u16.write_into(writer),
                }
            }
        }
    }

    pub struct AttachList {
        pub items: BTreeMap<GlyphId, Vec<u16>>,
    }

    impl FontWrite for AttachList {
        fn write_into(&self, writer: &mut TableWriter) {
            let coverage = self.items.keys().copied().collect::<CoverageTable>();
            writer.write_offset::<Offset16>(&coverage);
            (self.items.len() as u16).write_into(writer);
            for points in self.items.values() {
                writer.write_offset::<Offset16>(&AttachPointTemp { points })
            }
        }
    }

    struct AttachPointTemp<'a> {
        points: &'a [u16],
    }

    impl FontWrite for AttachPointTemp<'_> {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.points.len() as u16).write_into(writer);
            self.points.write_into(writer);
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

    impl FontWrite for LigCaretList {
        fn write_into(&self, writer: &mut TableWriter) {
            let coverage = self.items.keys().copied().collect::<CoverageTable>();
            writer.write_offset::<Offset16>(&coverage);
            (self.items.len() as u16).write_into(writer);
            for carets in self.items.values() {
                writer.write_offset::<Offset16>(&LigGlyphTemp { carets });
            }
        }
    }

    struct LigGlyphTemp<'a> {
        carets: &'a [CaretValue],
    }

    impl FontWrite for LigGlyphTemp<'_> {
        fn write_into(&self, writer: &mut TableWriter) {
            (self.carets.len() as u16).write_into(writer);
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

    impl FontWrite for MarkGlyphSets {
        fn write_into(&self, writer: &mut TableWriter) {
            1u16.write_into(writer);
            (self.tables.len() as u16).write_into(writer);
            for table in &self.tables {
                writer.write_offset::<Offset32>(table);
            }
        }
    }

    pub struct MarkGlyphSetsNEXT {
        pub tables: Vec<OffsetMarker32<CoverageTable>>,
    }

    impl FontWrite for MarkGlyphSetsNEXT {
        fn write_into(&self, writer: &mut TableWriter) {
            1u16.write_into(writer);
            (self.tables.len() as u16).write_into(writer);
            self.tables.write_into(writer);
        }
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
            device: Box<dyn FontWrite>,
        },
    }

    impl FontWrite for CaretValue {
        fn write_into(&self, writer: &mut TableWriter) {
            match self {
                Self::Format1 { coordinate } => {
                    1u16.write_into(writer);
                    coordinate.write_into(writer);
                }
                Self::Format2 {
                    caret_value_point_index,
                } => {
                    2u16.write_into(writer);
                    caret_value_point_index.write_into(writer);
                }
                Self::Format3 { coordinate, device } => {
                    3u16.write_into(writer);
                    coordinate.write_into(writer);
                    writer.write_offset::<Offset16>(device);
                }
            }
        }
    }
}
