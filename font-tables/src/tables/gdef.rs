//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

#[path = "../../generated/generated_gdef.rs"]
mod generated;

pub use generated::*;

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
pub mod compile {
    use font_types::{GlyphId, Offset, Offset16, OffsetHost};
    use std::collections::BTreeMap;

    use crate::{
        compile::{FontWrite, OffsetMarker16, TableWriter, ToOwnedObj},
        layout::{compile::CoverageTable, CoverageTable as CoverageTableRef},
    };

    pub use super::generated::compile::*;

    // a more ergonimic representation
    #[derive(Debug, Default, PartialEq)]
    pub struct AttachList {
        pub items: BTreeMap<GlyphId, Vec<u16>>,
    }

    impl ToOwnedObj for super::AttachList<'_> {
        type Owned = AttachList;

        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            let offset_data = self.bytes();
            let coverage = self
                .coverage_offset()
                .read::<CoverageTableRef>(offset_data)?;

            let attach_points = self.attach_point_offsets().iter().map(|off| {
                off.get()
                    .read::<super::AttachPoint>(offset_data)
                    .map(|x| {
                        x.point_indices()
                            .iter()
                            .map(|pt| pt.get())
                            .collect::<Vec<_>>()
                    })
                    .expect("invalid offset in AttachList")
            });
            Some(AttachList {
                items: coverage.iter().zip(attach_points).collect(),
            })
        }
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
    #[derive(Debug, Default, PartialEq)]
    pub struct CaretValueFormat3 {
        pub coordinate: i16,
        pub device_offset: OffsetMarker16<Box<dyn FontWrite>>,
    }

    impl ToOwnedObj for super::CaretValueFormat3<'_> {
        type Owned = CaretValueFormat3;
        fn to_owned_obj(&self, _offset_data: &[u8]) -> Option<Self::Owned> {
            Some(CaretValueFormat3 {
                coordinate: self.coordinate(),
                device_offset: Default::default(),
            })
        }
    }

    impl FontWrite for CaretValueFormat3 {
        fn write_into(&self, writer: &mut crate::compile::TableWriter) {
            (3 as u16).write_into(writer);
            self.coordinate.write_into(writer);
            self.device_offset.write_into(writer);
        }
    }
}
