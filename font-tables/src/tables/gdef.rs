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
    use crate::compile::{FontWrite, ToOwnedObj};

    pub use super::generated::compile::*;

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
