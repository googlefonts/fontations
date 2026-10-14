//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

pub use super::layout::{ClassDef, CoverageTable, DeviceOrVariationIndex};

use super::variations::ItemVariationStore;

#[cfg(test)]
#[path = "../tests/test_gdef.rs"]
mod tests;

include!("../../generated/generated_gdef.rs");

impl<'a> Gdef<'a> {
    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn glyph_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        let offset = match super::layout::extended::preferred_offset(
            self.glyph_class_def_offset(),
            self.glyph_class_def2_offset(),
            self.version() >= MajorMinor::new(1, 4),
        ) {
            Ok(offset) => offset,
            Err(error) => return Some(Err(error)),
        };
        if offset.is_null() {
            None
        } else {
            Some(offset.resolve(self.offset_data()))
        }
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_glyph_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.glyph_class_def_offset().resolve(self.offset_data())
    }
    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn attach_list(&self) -> Option<Result<AttachList<'a>, ReadError>> {
        let offset = match super::layout::extended::preferred_offset(
            self.attach_list_offset(),
            self.attach_list2_offset(),
            self.version() >= MajorMinor::new(1, 4),
        ) {
            Ok(offset) => offset,
            Err(error) => return Some(Err(error)),
        };
        if offset.is_null() {
            None
        } else {
            Some(offset.resolve(self.offset_data()))
        }
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_attach_list(&self) -> Option<Result<AttachList<'a>, ReadError>> {
        self.attach_list_offset().resolve(self.offset_data())
    }
    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn mark_attach_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        let offset = match super::layout::extended::preferred_offset(
            self.mark_attach_class_def_offset(),
            self.mark_attach_class_def2_offset(),
            self.version() >= MajorMinor::new(1, 4),
        ) {
            Ok(offset) => offset,
            Err(error) => return Some(Err(error)),
        };
        if offset.is_null() {
            None
        } else {
            Some(offset.resolve(self.offset_data()))
        }
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_mark_attach_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.mark_attach_class_def_offset()
            .resolve(self.offset_data())
    }
    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn mark_glyph_sets_def(&self) -> Option<Result<MarkGlyphSets<'a>, ReadError>> {
        let offset = match super::layout::extended::preferred_offset(
            self.mark_glyph_sets_def_offset().unwrap_or_default(),
            self.mark_glyph_sets_def2_offset(),
            self.version() >= MajorMinor::new(1, 4),
        ) {
            Ok(offset) => offset,
            Err(error) => return Some(Err(error)),
        };
        if offset.is_null() {
            None
        } else {
            Some(offset.resolve(self.offset_data()))
        }
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_mark_glyph_sets_def(&self) -> Option<Result<MarkGlyphSets<'a>, ReadError>> {
        self.mark_glyph_sets_def_offset()
            .unwrap_or_default()
            .resolve(self.offset_data())
    }
}

/// LigCaretList or LigCaretList2, selected by the GDEF header.
#[derive(Clone)]
pub enum LigCaretListTable<'a> {
    Offset16(LigCaretList<'a>),
    Offset24(LigCaretList2<'a>),
}

impl<'a> LigCaretListTable<'a> {
    pub fn coverage(&self) -> Result<CoverageTable<'a>, ReadError> {
        match self {
            Self::Offset16(t) => t.coverage(),
            Self::Offset24(t) => t.coverage(),
        }
    }

    pub fn lig_glyph_count(&self) -> u32 {
        match self {
            Self::Offset16(t) => u32::from(t.lig_glyph_count()),
            Self::Offset24(t) => t.lig_glyph_count().to_u32(),
        }
    }

    pub fn lig_glyphs(&self) -> super::layout::LayoutOffsetArray<'a, LigGlyph<'a>> {
        match self {
            Self::Offset16(t) => super::layout::LayoutOffsetArray::Offset16(t.lig_glyphs()),
            Self::Offset24(t) => super::layout::LayoutOffsetArray::Offset24(t.lig_glyphs()),
        }
    }
}

impl<'a> Gdef<'a> {
    pub fn lig_caret_list(&self) -> Option<Result<LigCaretListTable<'a>, ReadError>> {
        let offset = match super::layout::extended::preferred_offset(
            self.lig_caret_list_offset(),
            self.lig_caret_list2_offset(),
            self.version() >= MajorMinor::new(1, 4),
        ) {
            Ok(offset) => offset,
            Err(error) => return Some(Err(error)),
        };
        if offset.is_null() {
            return None;
        }
        if self
            .lig_caret_list2_offset()
            .is_some_and(|o| !o.offset().is_null())
        {
            Some(
                offset
                    .resolve(self.offset_data())
                    .map(LigCaretListTable::Offset24),
            )
        } else {
            Some(
                offset
                    .resolve(self.offset_data())
                    .map(LigCaretListTable::Offset16),
            )
        }
    }

    pub fn legacy_lig_caret_list(&self) -> Option<Result<LigCaretList<'a>, ReadError>> {
        self.lig_caret_list_offset().resolve(self.offset_data())
    }
}
