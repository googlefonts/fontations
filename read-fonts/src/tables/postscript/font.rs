//! An embedded PostScript "font"

use std::ops::Range;

use super::{dict, Error, FdSelect, Index, Index1, Latin1String, StringId};
use crate::{
    tables::{cff::Cff, cff2::Cff2, variations::ItemVariationStore},
    types::{Fixed, GlyphId},
    FontData, FontRead, ReadError, TableProvider,
};

/// Reference to an embedded PostScript font.
pub struct Font<'a> {
    /// Root table data for accessing items by offset.
    pub table_data: &'a [u8],
    // TODO: add type for font matrix
    pub matrix: Option<[Fixed; 6]>,
    pub charstrings: Option<Index<'a>>,
    pub fd_array: Option<Index<'a>>,
    pub fd_select: Option<FdSelect<'a>>,
    pub global_subrs: Index<'a>,
    pub units_per_em: u16,
    pub kind: FontKind<'a>,
}

impl<'a> Font<'a> {
    pub fn new(font: &impl TableProvider<'a>) -> Result<Self, Error> {
        let units_per_em = font.head()?.units_per_em();
        if let Ok(cff2) = font.cff2() {
            Self::from_cff2(&cff2, units_per_em)
        } else {
            Self::from_cff(&font.cff()?, units_per_em)
        }
    }

    pub fn from_cff(cff: &Cff<'a>, units_per_em: u16) -> Result<Self, Error> {
        let table_data = cff.offset_data().as_bytes();
        let mut data = FontData::new(cff.trailing_data());
        let names = Index1::read(data)?;
        data = data
            .split_off(names.size_in_bytes()?)
            .ok_or(ReadError::OutOfBounds)?;
        let top_dicts = Index1::read(data)?;
        data = data
            .split_off(top_dicts.size_in_bytes()?)
            .ok_or(ReadError::OutOfBounds)?;
        let strings = Index1::read(data)?;
        data = data
            .split_off(strings.size_in_bytes()?)
            .ok_or(ReadError::OutOfBounds)?;
        let global_subrs = Index::new(data.as_bytes(), false)?;
        let top_dict_data = top_dicts.get(0)?;
        let top_items = TopDictItems::new(table_data, top_dict_data, false)?;
        let private_dict_range = top_items.private_dict_range.clone();
        Ok(Self::new_impl(
            table_data,
            units_per_em,
            top_items,
            global_subrs,
            FontKind::Cff(CffFont {
                names,
                top_dicts,
                strings,
                private_dict_range,
            }),
        ))
    }

    pub fn from_cff2(cff2: &Cff2<'a>, units_per_em: u16) -> Result<Self, Error> {
        let top_dict_data = cff2.top_dict_data();
        let global_subrs_data = cff2.trailing_data();
        let global_subrs = Index::new(global_subrs_data, true)?;
        let table_data = cff2.offset_data().as_bytes();
        let top_items = TopDictItems::new(table_data, top_dict_data, true)?;
        let var_store = top_items.var_store.clone();
        Ok(Self::new_impl(
            table_data,
            units_per_em,
            top_items,
            global_subrs,
            FontKind::Cff2(Cff2Font { var_store }),
        ))
    }

    pub fn string(&self, id: StringId) -> Option<Latin1String<'a>> {
        match id.standard_string() {
            Ok(name) => Some(name),
            Err(ix) => match &self.kind {
                FontKind::Cff(cff) => cff.strings.get(ix).ok().map(Latin1String::new),
                _ => None,
            },
        }
    }

    /// Returns the number of available subfonts.
    pub fn subfont_count(&self) -> u32 {
        self.fd_array
            .as_ref()
            .map(|fd_array| fd_array.count())
            .unwrap_or(1)
    }

    /// Returns the subfont (or FD) index for the given glyph identifier.
    pub fn subfont_index(&self, glyph_id: GlyphId) -> u32 {
        self.fd_select
            .as_ref()
            .and_then(|select| select.font_index(glyph_id))
            // Missing FDSelect assumes a single Font DICT at index 0.
            .unwrap_or(0) as u32
    }
}

impl<'a> Font<'a> {
    fn new_impl(
        table_data: &'a [u8],
        units_per_em: u16,
        top_items: TopDictItems<'a>,
        global_subrs: Index<'a>,
        kind: FontKind<'a>,
    ) -> Self {
        Self {
            table_data,
            units_per_em,
            matrix: top_items.matrix,
            charstrings: top_items.charstrings,
            fd_array: top_items.fd_array,
            fd_select: top_items.fd_select,
            global_subrs,
            kind,
        }
    }
}

/// Data that is specific to a `CFF` table.
#[derive(Clone)]
pub struct CffFont<'a> {
    pub names: Index1<'a>,
    pub top_dicts: Index1<'a>,
    pub strings: Index1<'a>,
    pub private_dict_range: Option<Range<usize>>,
}

/// Data that is specific to a `CFF2` table.
#[derive(Clone)]
pub struct Cff2Font<'a> {
    pub var_store: Option<ItemVariationStore<'a>>,
}

/// Data that is specific to the underlying type of font.
#[derive(Clone)]
pub enum FontKind<'a> {
    // TODO: support Type1 fonts
    Cff(CffFont<'a>),
    Cff2(Cff2Font<'a>),
}

/// Helper type for reading common items from a top DICT.
#[derive(Default)]
struct TopDictItems<'a> {
    pub matrix: Option<[Fixed; 6]>,
    pub charstrings: Option<Index<'a>>,
    pub fd_array: Option<Index<'a>>,
    pub fd_select: Option<FdSelect<'a>>,
    pub private_dict_range: Option<Range<usize>>,
    pub var_store: Option<ItemVariationStore<'a>>,
}

impl<'a> TopDictItems<'a> {
    fn new(table_data: &'a [u8], top_dict_data: &'a [u8], is_cff2: bool) -> Result<Self, Error> {
        let mut items = TopDictItems::default();
        for entry in dict::entries(top_dict_data, None) {
            match entry? {
                dict::Entry::FontMatrix(matrix) => {
                    items.matrix = Some(matrix);
                }
                dict::Entry::CharstringsOffset(offset) => {
                    items.charstrings = Some(Index::new(
                        table_data.get(offset..).unwrap_or_default(),
                        is_cff2,
                    )?);
                }
                dict::Entry::FdArrayOffset(offset) => {
                    items.fd_array = Some(Index::new(
                        table_data.get(offset..).unwrap_or_default(),
                        is_cff2,
                    )?);
                }
                dict::Entry::FdSelectOffset(offset) => {
                    items.fd_select = Some(FdSelect::read(FontData::new(
                        table_data.get(offset..).unwrap_or_default(),
                    ))?);
                }
                dict::Entry::PrivateDictRange(range) => {
                    items.private_dict_range = Some(range);
                }
                dict::Entry::VariationStoreOffset(offset) if is_cff2 => {
                    items.var_store = Some(ItemVariationStore::read(FontData::new(
                        table_data.get(offset..).unwrap_or_default(),
                    ))?);
                }
                _ => {}
            }
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FontRef;

    #[test]
    fn noto_serif_display_strings() {
        let font = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED).unwrap();
        let ps_font = Font::new(&font).unwrap();
        assert!(matches!(ps_font.kind, FontKind::Cff(_)));
        // Version
        assert_eq!(ps_font.string(StringId::new(391)).unwrap(), "2.9");
        // Notice
        assert_eq!(
            ps_font.string(StringId::new(392)).unwrap(),
            "Noto is a trademark of Google LLC."
        );
        // Copyright
        assert_eq!(
            ps_font.string(StringId::new(393)).unwrap(),
            "Copyright 2022 The Noto Project Authors https:github.comnotofontslatin-greek-cyrillic"
        );
        // FullName
        assert_eq!(
            ps_font.string(StringId::new(394)).unwrap(),
            "Noto Serif Display Regular"
        );
        // FamilyName
        assert_eq!(
            ps_font.string(StringId::new(395)).unwrap(),
            "Noto Serif Display"
        );
        assert_eq!(ps_font.subfont_count(), 1);
    }
}
