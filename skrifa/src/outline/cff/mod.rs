//! Support for scaling CFF outlines.

mod hint;

use super::{GlyphHMetrics, OutlinePen};
use hint::{HintParams, HintState, HintingSink};
use raw::FontRef;
use read_fonts::{
    tables::{
        postscript::{
            charstring::{self, CommandSink},
            dict, BlendState, Error, FdSelect, Index,
        },
        variations::ItemVariationStore,
    },
    types::{F2Dot14, Fixed, GlyphId},
    FontData, FontRead, ReadError, TableProvider,
};
use std::ops::Range;

/// Type for loading, scaling and hinting outlines in CFF/CFF2 tables.
///
/// The skrifa crate provides a higher level interface for this that handles
/// caching and abstracting over the different outline formats. Consider using
/// that if detailed control over resources is not required.
///
/// # Subfonts
///
/// CFF tables can contain multiple logical "subfonts" which determine the
/// state required for processing some subset of glyphs. This state is
/// accessed using the [`FDArray and FDSelect`](https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=28)
/// operators to select an appropriate subfont for any given glyph identifier.
/// This process is exposed on this type with the
/// [`subfont_index`](Self::subfont_index) method to retrieve the subfont
/// index for the requested glyph followed by using the
/// [`subfont`](Self::subfont) method to create an appropriately configured
/// subfont for that glyph.
#[derive(Clone)]
pub(crate) struct Outlines<'a> {
    pub(crate) font: FontRef<'a>,
    pub(crate) glyph_metrics: GlyphHMetrics<'a>,
    offset_data: FontData<'a>,
    global_subrs: Index<'a>,
    top_dict: TopDict<'a>,
    version: u16,
    units_per_em: u16,
}

impl<'a> Outlines<'a> {
    /// Creates a new scaler for the given font.
    ///
    /// This will choose an underlying CFF2 or CFF table from the font, in that
    /// order.
    pub fn new(font: &FontRef<'a>) -> Option<Self> {
        let units_per_em = font.head().ok()?.units_per_em();
        Self::from_cff2(font, units_per_em).or_else(|| Self::from_cff(font, units_per_em))
    }

    pub fn from_cff(font: &FontRef<'a>, units_per_em: u16) -> Option<Self> {
        let cff1 = font.cff().ok()?;
        let glyph_metrics = GlyphHMetrics::new(font)?;
        // "The Name INDEX in the CFF data must contain only one entry;
        // that is, there must be only one font in the CFF FontSet"
        // So we always pass 0 for Top DICT index when reading from an
        // OpenType font.
        // <https://learn.microsoft.com/en-us/typography/opentype/spec/cff>
        let top_dict_data = cff1.top_dicts().get(0).ok()?;
        let top_dict = TopDict::new(cff1.offset_data().as_bytes(), top_dict_data, false).ok()?;
        Some(Self {
            font: font.clone(),
            glyph_metrics,
            offset_data: cff1.offset_data(),
            global_subrs: cff1.global_subrs().into(),
            top_dict,
            version: 1,
            units_per_em,
        })
    }

    pub fn from_cff2(font: &FontRef<'a>, units_per_em: u16) -> Option<Self> {
        let cff2 = font.cff2().ok()?;
        let glyph_metrics = GlyphHMetrics::new(font)?;
        let table_data = cff2.offset_data().as_bytes();
        let top_dict = TopDict::new(table_data, cff2.top_dict_data(), true).ok()?;
        Some(Self {
            font: font.clone(),
            glyph_metrics,
            offset_data: cff2.offset_data(),
            global_subrs: cff2.global_subrs().into(),
            top_dict,
            version: 2,
            units_per_em,
        })
    }

    pub fn is_cff2(&self) -> bool {
        self.version == 2
    }

    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Returns the number of available glyphs.
    pub fn glyph_count(&self) -> usize {
        self.top_dict.charstrings.count() as usize
    }

    /// Returns the number of available subfonts.
    pub fn subfont_count(&self) -> u32 {
        // All CFF fonts have at least one logical subfont.
        self.top_dict.font_dicts.count().max(1)
    }

    /// Returns the subfont (or Font DICT) index for the given glyph
    /// identifier.
    pub fn subfont_index(&self, glyph_id: GlyphId) -> u32 {
        // For CFF tables, an FDSelect index will be present for CID-keyed
        // fonts. Otherwise, the Top DICT will contain an entry for the
        // "global" Private DICT.
        // See <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=27>
        //
        // CFF2 tables always contain a Font DICT and an FDSelect is only
        // present if the size of the DICT is greater than 1.
        // See <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#10-font-dict-index-font-dicts-and-fdselect>
        //
        // In both cases, we return a subfont index of 0 when FDSelect is missing.
        self.top_dict
            .fd_select
            .as_ref()
            .and_then(|select| select.font_index(glyph_id))
            .unwrap_or(0) as u32
    }

    /// Creates a new subfont for the given index, size, normalized
    /// variation coordinates and hinting state.
    ///
    /// The index of a subfont for a particular glyph can be retrieved with
    /// the [`subfont_index`](Self::subfont_index) method.
    pub fn subfont(
        &self,
        index: u32,
        size: Option<f32>,
        coords: &[F2Dot14],
    ) -> Result<Subfont, Error> {
        let private_dict_range = self.private_dict_range(index)?;
        let blend_state = self
            .top_dict
            .var_store
            .clone()
            .map(|store| BlendState::new(store, coords, 0))
            .transpose()?;
        let private_dict = PrivateDict::new(self.offset_data, private_dict_range, blend_state)?;
        let scale = match size {
            Some(ppem) if self.units_per_em > 0 => {
                // Note: we do an intermediate scale to 26.6 to ensure we
                // match FreeType
                Some(
                    Fixed::from_bits((ppem * 64.) as i32)
                        / Fixed::from_bits(self.units_per_em as i32),
                )
            }
            _ => None,
        };
        // When hinting, use a modified scale factor
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psft.c#L279>
        let hint_scale = Fixed::from_bits((scale.unwrap_or(Fixed::ONE).to_bits() + 32) / 64);
        let hint_state = HintState::new(&private_dict.hint_params, hint_scale);
        Ok(Subfont {
            is_cff2: self.is_cff2(),
            scale,
            subrs_offset: private_dict.subrs_offset,
            hint_state,
            store_index: private_dict.store_index,
        })
    }

    /// Loads and scales an outline for the given subfont instance, glyph
    /// identifier and normalized variation coordinates.
    ///
    /// Before calling this method, use [`subfont_index`](Self::subfont_index)
    /// to retrieve the subfont index for the desired glyph and then
    /// [`subfont`](Self::subfont) to create an instance of the subfont for a
    /// particular size and location in variation space.
    /// Creating subfont instances is not free, so this process is exposed in
    /// discrete steps to allow for caching.
    ///
    /// The result is emitted to the specified pen.
    pub fn draw(
        &self,
        subfont: &Subfont,
        glyph_id: GlyphId,
        coords: &[F2Dot14],
        hint: bool,
        pen: &mut impl OutlinePen,
    ) -> Result<(), Error> {
        let charstring_data = self.top_dict.charstrings.get(glyph_id.to_u32() as usize)?;
        let subrs = subfont.subrs(self)?;
        let blend_state = subfont.blend_state(self, coords)?;
        let mut pen_sink = PenSink::new(pen);
        let mut simplifying_adapter = NopFilteringSink::new(&mut pen_sink);
        // Only apply hinting if we have a scale
        if hint && subfont.scale.is_some() {
            let mut hinting_adapter =
                HintingSink::new(&subfont.hint_state, &mut simplifying_adapter);
            charstring::evaluate(
                charstring_data,
                self.global_subrs.clone(),
                subrs,
                blend_state,
                &mut hinting_adapter,
            )?;
            hinting_adapter.finish();
        } else {
            let mut scaling_adapter =
                ScalingSink26Dot6::new(&mut simplifying_adapter, subfont.scale);
            charstring::evaluate(
                charstring_data,
                self.global_subrs.clone(),
                subrs,
                blend_state,
                &mut scaling_adapter,
            )?;
        }
        simplifying_adapter.finish();
        Ok(())
    }

    fn private_dict_range(&self, subfont_index: u32) -> Result<Range<usize>, Error> {
        if self.top_dict.font_dicts.count() != 0 {
            // If we have a font dict array, extract the private dict range
            // from the font dict at the given index.
            let font_dict_data = self.top_dict.font_dicts.get(subfont_index as usize)?;
            let mut range = None;
            for entry in dict::entries(font_dict_data, None) {
                if let dict::Entry::PrivateDictRange(r) = entry? {
                    range = Some(r);
                    break;
                }
            }
            range
        } else {
            // Use the private dict range from the top dict.
            // Note: "A Private DICT is required but may be specified as having
            // a length of 0 if there are no non-default values to be stored."
            // <https://adobe-type-tools.github.io/font-tech-notes/pdfs/5176.CFF.pdf#page=25>
            let range = self.top_dict.private_dict_range.clone();
            Some(range.start as usize..range.end as usize)
        }
        .ok_or(Error::MissingPrivateDict)
    }
}

/// Specifies local subroutines and hinting parameters for some subset of
/// glyphs in a CFF or CFF2 table.
///
/// This type is designed to be cacheable to avoid re-evaluating the private
/// dict every time a charstring is processed.
///
/// For variable fonts, this is dependent on a location in variation space.
#[derive(Clone)]
pub(crate) struct Subfont {
    is_cff2: bool,
    scale: Option<Fixed>,
    subrs_offset: Option<usize>,
    pub(crate) hint_state: HintState,
    store_index: u16,
}

impl Subfont {
    /// Returns the local subroutine index.
    pub fn subrs<'a>(&self, scaler: &Outlines<'a>) -> Result<Option<Index<'a>>, Error> {
        if let Some(subrs_offset) = self.subrs_offset {
            let offset_data = scaler.offset_data.as_bytes();
            let index_data = offset_data.get(subrs_offset..).unwrap_or_default();
            Ok(Some(Index::new(index_data, self.is_cff2)?))
        } else {
            Ok(None)
        }
    }

    /// Creates a new blend state for the given normalized variation
    /// coordinates.
    pub fn blend_state<'a>(
        &self,
        scaler: &Outlines<'a>,
        coords: &'a [F2Dot14],
    ) -> Result<Option<BlendState<'a>>, Error> {
        if let Some(var_store) = scaler.top_dict.var_store.clone() {
            Ok(Some(BlendState::new(var_store, coords, self.store_index)?))
        } else {
            Ok(None)
        }
    }
}

/// Entries that we parse from the Private DICT to support charstring
/// evaluation.
#[derive(Default)]
struct PrivateDict {
    hint_params: HintParams,
    subrs_offset: Option<usize>,
    store_index: u16,
}

impl PrivateDict {
    fn new(
        data: FontData,
        range: Range<usize>,
        blend_state: Option<BlendState<'_>>,
    ) -> Result<Self, Error> {
        let private_dict_data = data.read_array(range.clone())?;
        let mut dict = Self::default();
        for entry in dict::entries(private_dict_data, blend_state) {
            use dict::Entry::*;
            match entry? {
                BlueValues(values) => dict.hint_params.blues = values,
                FamilyBlues(values) => dict.hint_params.family_blues = values,
                OtherBlues(values) => dict.hint_params.other_blues = values,
                FamilyOtherBlues(values) => dict.hint_params.family_other_blues = values,
                BlueScale(value) => dict.hint_params.blue_scale = value,
                BlueShift(value) => dict.hint_params.blue_shift = value,
                BlueFuzz(value) => dict.hint_params.blue_fuzz = value,
                LanguageGroup(group) => dict.hint_params.language_group = group,
                // Subrs offset is relative to the private DICT
                SubrsOffset(offset) => {
                    dict.subrs_offset = Some(
                        range
                            .start
                            .checked_add(offset)
                            .ok_or(ReadError::OutOfBounds)?,
                    )
                }
                VariationStoreIndex(index) => dict.store_index = index,
                _ => {}
            }
        }
        Ok(dict)
    }
}

/// Entries that we parse from the Top DICT that are required to support
/// charstring evaluation.
#[derive(Clone, Default)]
struct TopDict<'a> {
    charstrings: Index<'a>,
    font_dicts: Index<'a>,
    fd_select: Option<FdSelect<'a>>,
    private_dict_range: Range<u32>,
    var_store: Option<ItemVariationStore<'a>>,
}

impl<'a> TopDict<'a> {
    fn new(table_data: &'a [u8], top_dict_data: &'a [u8], is_cff2: bool) -> Result<Self, Error> {
        let mut items = TopDict::default();
        for entry in dict::entries(top_dict_data, None) {
            match entry? {
                dict::Entry::CharstringsOffset(offset) => {
                    items.charstrings =
                        Index::new(table_data.get(offset..).unwrap_or_default(), is_cff2)?;
                }
                dict::Entry::FdArrayOffset(offset) => {
                    items.font_dicts =
                        Index::new(table_data.get(offset..).unwrap_or_default(), is_cff2)?;
                }
                dict::Entry::FdSelectOffset(offset) => {
                    items.fd_select = Some(FdSelect::read(FontData::new(
                        table_data.get(offset..).unwrap_or_default(),
                    ))?);
                }
                dict::Entry::PrivateDictRange(range) => {
                    items.private_dict_range = range.start as u32..range.end as u32;
                }
                dict::Entry::VariationStoreOffset(offset) if is_cff2 => {
                    // IVS is preceded by a 2 byte length, but ensure that
                    // we don't overflow
                    // See <https://github.com/googlefonts/fontations/issues/1223>
                    let offset = offset.checked_add(2).ok_or(ReadError::OutOfBounds)?;
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

/// Command sink that sends the results of charstring evaluation to
/// an [OutlinePen].
struct PenSink<'a, P>(&'a mut P);

impl<'a, P> PenSink<'a, P> {
    fn new(pen: &'a mut P) -> Self {
        Self(pen)
    }
}

impl<P> CommandSink for PenSink<'_, P>
where
    P: OutlinePen,
{
    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.0.move_to(x.to_f32(), y.to_f32());
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.0.line_to(x.to_f32(), y.to_f32());
    }

    fn curve_to(&mut self, cx0: Fixed, cy0: Fixed, cx1: Fixed, cy1: Fixed, x: Fixed, y: Fixed) {
        self.0.curve_to(
            cx0.to_f32(),
            cy0.to_f32(),
            cx1.to_f32(),
            cy1.to_f32(),
            x.to_f32(),
            y.to_f32(),
        );
    }

    fn close(&mut self) {
        self.0.close();
    }
}

/// Command sink adapter that applies a scaling factor.
///
/// This assumes a 26.6 scaling factor packed into a Fixed and thus,
/// this is not public and exists only to match FreeType's exact
/// scaling process.
struct ScalingSink26Dot6<'a, S> {
    inner: &'a mut S,
    scale: Option<Fixed>,
}

impl<'a, S> ScalingSink26Dot6<'a, S> {
    fn new(sink: &'a mut S, scale: Option<Fixed>) -> Self {
        Self { scale, inner: sink }
    }

    fn scale(&self, coord: Fixed) -> Fixed {
        // The following dance is necessary to exactly match FreeType's
        // application of scaling factors. This seems to be the result
        // of merging the contributed Adobe code while not breaking the
        // FreeType public API.
        //
        // The first two steps apply to both scaled and unscaled outlines:
        //
        // 1. Multiply by 1/64
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psft.c#L284>
        let a = coord * Fixed::from_bits(0x0400);
        // 2. Truncate the bottom 10 bits. Combined with the division by 64,
        // converts to font units.
        // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/psobjs.c#L2219>
        let b = Fixed::from_bits(a.to_bits() >> 10);
        if let Some(scale) = self.scale {
            // Scaled case:
            // 3. Multiply by the original scale factor (to 26.6)
            // <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/cff/cffgload.c#L721>
            let c = b * scale;
            // 4. Convert from 26.6 to 16.16
            Fixed::from_bits(c.to_bits() << 10)
        } else {
            // Unscaled case:
            // 3. Convert from integer to 16.16
            Fixed::from_bits(b.to_bits() << 16)
        }
    }
}

impl<S: CommandSink> CommandSink for ScalingSink26Dot6<'_, S> {
    fn hstem(&mut self, y: Fixed, dy: Fixed) {
        self.inner.hstem(y, dy);
    }

    fn vstem(&mut self, x: Fixed, dx: Fixed) {
        self.inner.vstem(x, dx);
    }

    fn hint_mask(&mut self, mask: &[u8]) {
        self.inner.hint_mask(mask);
    }

    fn counter_mask(&mut self, mask: &[u8]) {
        self.inner.counter_mask(mask);
    }

    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.inner.move_to(self.scale(x), self.scale(y));
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        self.inner.line_to(self.scale(x), self.scale(y));
    }

    fn curve_to(&mut self, cx1: Fixed, cy1: Fixed, cx2: Fixed, cy2: Fixed, x: Fixed, y: Fixed) {
        self.inner.curve_to(
            self.scale(cx1),
            self.scale(cy1),
            self.scale(cx2),
            self.scale(cy2),
            self.scale(x),
            self.scale(y),
        );
    }

    fn close(&mut self) {
        self.inner.close();
    }
}

/// Command sink adapter that suppresses degenerate move and line commands.
///
/// FreeType avoids emitting empty contours and zero length lines to prevent
/// artifacts when stem darkening is enabled. We don't support stem darkening
/// because it's not enabled by any of our clients but we remove the degenerate
/// elements regardless to match the output.
///
/// See <https://gitlab.freedesktop.org/freetype/freetype/-/blob/80a507a6b8e3d2906ad2c8ba69329bd2fb2a85ef/src/psaux/pshints.c#L1786>
struct NopFilteringSink<'a, S> {
    start: Option<(Fixed, Fixed)>,
    last: Option<(Fixed, Fixed)>,
    pending_move: Option<(Fixed, Fixed)>,
    inner: &'a mut S,
}

impl<'a, S> NopFilteringSink<'a, S>
where
    S: CommandSink,
{
    fn new(inner: &'a mut S) -> Self {
        Self {
            start: None,
            last: None,
            pending_move: None,
            inner,
        }
    }

    fn flush_pending_move(&mut self) {
        if let Some((x, y)) = self.pending_move.take() {
            if let Some((last_x, last_y)) = self.start {
                if self.last != self.start {
                    self.inner.line_to(last_x, last_y);
                }
            }
            self.start = Some((x, y));
            self.last = None;
            self.inner.move_to(x, y);
        }
    }

    pub fn finish(&mut self) {
        if let Some((x, y)) = self.start {
            if self.last != self.start {
                self.inner.line_to(x, y);
            }
            self.inner.close();
        }
    }
}

impl<S> CommandSink for NopFilteringSink<'_, S>
where
    S: CommandSink,
{
    fn hstem(&mut self, y: Fixed, dy: Fixed) {
        self.inner.hstem(y, dy);
    }

    fn vstem(&mut self, x: Fixed, dx: Fixed) {
        self.inner.vstem(x, dx);
    }

    fn hint_mask(&mut self, mask: &[u8]) {
        self.inner.hint_mask(mask);
    }

    fn counter_mask(&mut self, mask: &[u8]) {
        self.inner.counter_mask(mask);
    }

    fn move_to(&mut self, x: Fixed, y: Fixed) {
        self.pending_move = Some((x, y));
    }

    fn line_to(&mut self, x: Fixed, y: Fixed) {
        if self.pending_move == Some((x, y)) {
            return;
        }
        self.flush_pending_move();
        if self.last == Some((x, y)) || (self.last.is_none() && self.start == Some((x, y))) {
            return;
        }
        self.inner.line_to(x, y);
        self.last = Some((x, y));
    }

    fn curve_to(&mut self, cx1: Fixed, cy1: Fixed, cx2: Fixed, cy2: Fixed, x: Fixed, y: Fixed) {
        self.flush_pending_move();
        self.last = Some((x, y));
        self.inner.curve_to(cx1, cy1, cx2, cy2, x, y);
    }

    fn close(&mut self) {
        if self.pending_move.is_none() {
            self.inner.close();
            self.start = None;
            self.last = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{super::pen::SvgPen, *};
    use crate::{
        outline::{HintingInstance, HintingOptions},
        prelude::{LocationRef, Size},
        MetadataProvider,
    };
    use dict::Blues;
    use font_test_data::bebuffer::BeBuffer;
    use raw::tables::cff2::Cff2;
    use read_fonts::FontRef;

    #[test]
    fn unscaled_scaling_sink_produces_integers() {
        let nothing = &mut ();
        let sink = ScalingSink26Dot6::new(nothing, None);
        for coord in [50.0, 50.1, 50.125, 50.5, 50.9] {
            assert_eq!(sink.scale(Fixed::from_f64(coord)).to_f32(), 50.0);
        }
    }

    #[test]
    fn scaled_scaling_sink() {
        let ppem = 20.0;
        let upem = 1000.0;
        // match FreeType scaling with intermediate conversion to 26.6
        let scale = Fixed::from_bits((ppem * 64.) as i32) / Fixed::from_bits(upem as i32);
        let nothing = &mut ();
        let sink = ScalingSink26Dot6::new(nothing, Some(scale));
        let inputs = [
            // input coord, expected scaled output
            (0.0, 0.0),
            (8.0, 0.15625),
            (16.0, 0.3125),
            (32.0, 0.640625),
            (72.0, 1.4375),
            (128.0, 2.5625),
        ];
        for (coord, expected) in inputs {
            assert_eq!(
                sink.scale(Fixed::from_f64(coord)).to_f32(),
                expected,
                "scaling coord {coord}"
            );
        }
    }

    #[test]
    fn read_cff_static() {
        let font = FontRef::new(font_test_data::NOTO_SERIF_DISPLAY_TRIMMED).unwrap();
        let cff = Outlines::new(&font).unwrap();
        assert!(!cff.is_cff2());
        assert!(cff.top_dict.var_store.is_none());
        assert!(cff.top_dict.font_dicts.count() == 0);
        assert!(!cff.top_dict.private_dict_range.is_empty());
        assert!(cff.top_dict.fd_select.is_none());
        assert_eq!(cff.subfont_count(), 1);
        assert_eq!(cff.subfont_index(GlyphId::new(1)), 0);
        assert_eq!(cff.global_subrs.count(), 17);
    }

    #[test]
    fn read_cff2_static() {
        let font = FontRef::new(font_test_data::CANTARELL_VF_TRIMMED).unwrap();
        let cff = Outlines::new(&font).unwrap();
        assert!(cff.is_cff2());
        assert!(cff.top_dict.var_store.is_some());
        assert!(cff.top_dict.font_dicts.count() != 0);
        assert!(cff.top_dict.private_dict_range.is_empty());
        assert!(cff.top_dict.fd_select.is_none());
        assert_eq!(cff.subfont_count(), 1);
        assert_eq!(cff.subfont_index(GlyphId::new(1)), 0);
        assert_eq!(cff.global_subrs.count(), 0);
    }

    #[test]
    fn read_example_cff2_table() {
        let cff2 = Cff2::read(FontData::new(font_test_data::cff2::EXAMPLE)).unwrap();
        let top_dict =
            TopDict::new(cff2.offset_data().as_bytes(), cff2.top_dict_data(), true).unwrap();
        assert!(top_dict.var_store.is_some());
        assert!(top_dict.font_dicts.count() != 0);
        assert!(top_dict.private_dict_range.is_empty());
        assert!(top_dict.fd_select.is_none());
        assert_eq!(cff2.global_subrs().count(), 0);
    }

    #[test]
    fn cff2_variable_outlines_match_freetype() {
        compare_glyphs(
            font_test_data::CANTARELL_VF_TRIMMED,
            font_test_data::CANTARELL_VF_TRIMMED_GLYPHS,
        );
    }

    #[test]
    fn cff_static_outlines_match_freetype() {
        compare_glyphs(
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED,
            font_test_data::NOTO_SERIF_DISPLAY_TRIMMED_GLYPHS,
        );
    }

    #[test]
    fn unhinted_ends_with_close() {
        let font = FontRef::new(font_test_data::CANTARELL_VF_TRIMMED).unwrap();
        let glyph = font.outline_glyphs().get(GlyphId::new(1)).unwrap();
        let mut svg = SvgPen::default();
        glyph.draw(Size::unscaled(), &mut svg).unwrap();
        assert!(svg.to_string().ends_with('Z'));
    }

    #[test]
    fn hinted_ends_with_close() {
        let font = FontRef::new(font_test_data::CANTARELL_VF_TRIMMED).unwrap();
        let glyphs = font.outline_glyphs();
        let hinter = HintingInstance::new(
            &glyphs,
            Size::unscaled(),
            LocationRef::default(),
            HintingOptions::default(),
        )
        .unwrap();
        let glyph = glyphs.get(GlyphId::new(1)).unwrap();
        let mut svg = SvgPen::default();
        glyph.draw(&hinter, &mut svg).unwrap();
        assert!(svg.to_string().ends_with('Z'));
    }

    /// Ensure we don't reject an empty Private DICT
    #[test]
    fn empty_private_dict() {
        let font = FontRef::new(font_test_data::MATERIAL_ICONS_SUBSET).unwrap();
        let outlines = super::Outlines::new(&font).unwrap();
        assert!(outlines.top_dict.private_dict_range.is_empty());
        assert!(outlines.private_dict_range(0).unwrap().is_empty());
    }

    /// Fuzzer caught add with overflow when computing subrs offset.
    /// See <https://issues.oss-fuzz.com/issues/377965575>
    #[test]
    fn subrs_offset_overflow() {
        // A private DICT with an overflowing subrs offset
        let private_dict = BeBuffer::new()
            .push(0u32) // pad so that range doesn't start with 0 and we overflow
            .push(29u8) // integer operator
            .push(-1i32) // integer value
            .push(19u8) // subrs offset operator
            .to_vec();
        // Just don't panic with overflow
        assert!(
            PrivateDict::new(FontData::new(&private_dict), 4..private_dict.len(), None).is_err()
        );
    }

    // Fuzzer caught add with overflow when computing offset to
    // var store.
    // See <https://issues.oss-fuzz.com/issues/377574377>
    #[test]
    fn top_dict_ivs_offset_overflow() {
        // A top DICT with a var store offset of -1 which will cause an
        // overflow
        let top_dict = BeBuffer::new()
            .push(29u8) // integer operator
            .push(-1i32) // integer value
            .push(24u8) // var store offset operator
            .to_vec();
        // Just don't panic with overflow
        assert!(TopDict::new(&[], &top_dict, true).is_err());
    }

    /// Actually apply a scale when the computed scale factor is
    /// equal to Fixed::ONE.
    ///
    /// Specifically, when upem = 512 and ppem = 8, this results in
    /// a scale factor of 65536 which was being interpreted as an
    /// unscaled draw request.
    #[test]
    fn proper_scaling_when_factor_equals_fixed_one() {
        let font = FontRef::new(font_test_data::MATERIAL_ICONS_SUBSET).unwrap();
        assert_eq!(font.head().unwrap().units_per_em(), 512);
        let glyphs = font.outline_glyphs();
        let glyph = glyphs.get(GlyphId::new(1)).unwrap();
        let mut svg = SvgPen::with_precision(6);
        glyph
            .draw((Size::new(8.0), LocationRef::default()), &mut svg)
            .unwrap();
        // This was initially producing unscaled values like M405.000...
        assert!(svg.starts_with("M6.328125,7.000000 L1.671875,7.000000"));
    }

    /// For the given font data and extracted outlines, parse the extracted
    /// outline data into a set of expected values and compare these with the
    /// results generated by the scaler.
    ///
    /// This will compare all outlines at various sizes and (for variable
    /// fonts), locations in variation space.
    fn compare_glyphs(font_data: &[u8], expected_outlines: &str) {
        use super::super::testing;
        let font = FontRef::new(font_data).unwrap();
        let expected_outlines = testing::parse_glyph_outlines(expected_outlines);
        let outlines = super::Outlines::new(&font).unwrap();
        let mut path = testing::Path::default();
        for expected_outline in &expected_outlines {
            if expected_outline.size == 0.0 && !expected_outline.coords.is_empty() {
                continue;
            }
            let size = (expected_outline.size != 0.0).then_some(expected_outline.size);
            path.elements.clear();
            let subfont = outlines
                .subfont(
                    outlines.subfont_index(expected_outline.glyph_id),
                    size,
                    &expected_outline.coords,
                )
                .unwrap();
            outlines
                .draw(
                    &subfont,
                    expected_outline.glyph_id,
                    &expected_outline.coords,
                    false,
                    &mut path,
                )
                .unwrap();
            if path.elements != expected_outline.path {
                panic!(
                    "mismatch in glyph path for id {} (size: {}, coords: {:?}): path: {:?} expected_path: {:?}",
                    expected_outline.glyph_id,
                    expected_outline.size,
                    expected_outline.coords,
                    &path.elements,
                    &expected_outline.path
                );
            }
        }
    }

    // We were overwriting family_other_blues with family_blues.
    #[test]
    fn capture_family_other_blues() {
        let private_dict_data = &font_test_data::cff2::EXAMPLE[0x4f..=0xc0];
        let store =
            ItemVariationStore::read(FontData::new(&font_test_data::cff2::EXAMPLE[18..])).unwrap();
        let coords = &[F2Dot14::from_f32(0.0)];
        let blend_state = BlendState::new(store, coords, 0).unwrap();
        let private_dict = PrivateDict::new(
            FontData::new(private_dict_data),
            0..private_dict_data.len(),
            Some(blend_state),
        )
        .unwrap();
        assert_eq!(
            private_dict.hint_params.family_other_blues,
            Blues::new([-249.0, -239.0].map(Fixed::from_f64).into_iter())
        )
    }
}
