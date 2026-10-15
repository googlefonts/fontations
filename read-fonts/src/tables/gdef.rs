//! the [GDEF] table
//!
//! [GDEF]: https://docs.microsoft.com/en-us/typography/opentype/spec/gdef

pub use super::layout::{ClassDef, CoverageTable, DeviceOrVariationIndex};

use super::layout::extended::HeaderOffset;
use super::variations::ItemVariationStore;

#[cfg(test)]
#[path = "../tests/test_gdef.rs"]
mod tests;

include!("../../generated/generated_gdef.rs");

impl<'a> Gdef<'a> {
    fn select_offset(
        &self,
        original: Nullable<Offset16>,
        extended: Option<Nullable<Offset32>>,
    ) -> Result<HeaderOffset, ReadError> {
        HeaderOffset::select(original, extended, self.version() >= MajorMinor::new(1, 4))
    }

    fn resolve_selected<T: FontRead<'a, Args = ()>>(
        &self,
        original: Nullable<Offset16>,
        extended: Option<Nullable<Offset32>>,
    ) -> Option<Result<T, ReadError>> {
        match self.select_offset(original, extended) {
            Ok(offset) => offset.resolve(self.offset_data()),
            Err(error) => Some(Err(error)),
        }
    }

    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn glyph_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.resolve_selected(
            self.glyph_class_def_offset(),
            self.glyph_class_def2_offset(),
        )
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_glyph_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.glyph_class_def_offset().resolve(self.offset_data())
    }

    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn attach_list(&self) -> Option<Result<AttachList<'a>, ReadError>> {
        self.resolve_selected(self.attach_list_offset(), self.attach_list2_offset())
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_attach_list(&self) -> Option<Result<AttachList<'a>, ReadError>> {
        self.attach_list_offset().resolve(self.offset_data())
    }

    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn mark_attach_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.resolve_selected(
            self.mark_attach_class_def_offset(),
            self.mark_attach_class_def2_offset(),
        )
    }

    /// Resolves only the original 16-bit offset.
    pub fn legacy_mark_attach_class_def(&self) -> Option<Result<ClassDef<'a>, ReadError>> {
        self.mark_attach_class_def_offset()
            .resolve(self.offset_data())
    }

    /// Resolves the table, preferring its 32-bit offset when nonzero.
    pub fn mark_glyph_sets_def(&self) -> Option<Result<MarkGlyphSets<'a>, ReadError>> {
        self.resolve_selected(
            self.mark_glyph_sets_def_offset().unwrap_or_default(),
            self.mark_glyph_sets_def2_offset(),
        )
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
        let offset =
            match self.select_offset(self.lig_caret_list_offset(), self.lig_caret_list2_offset()) {
                Ok(offset) => offset,
                Err(error) => return Some(Err(error)),
            };
        if offset.is_null() {
            return None;
        }
        let data = self.offset_data();
        Some(match offset {
            HeaderOffset::Original(offset) => offset.resolve(data).map(LigCaretListTable::Offset16),
            HeaderOffset::Extended(offset) => offset.resolve(data).map(LigCaretListTable::Offset24),
        })
    }

    pub fn legacy_lig_caret_list(&self) -> Option<Result<LigCaretList<'a>, ReadError>> {
        self.lig_caret_list_offset().resolve(self.offset_data())
    }
}
