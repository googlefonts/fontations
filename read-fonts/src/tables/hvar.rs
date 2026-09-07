//! The [HVAR (Horizontal Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table

use super::variations::{self, DeltaSetIndexMap, ItemVariationStore};

include!("../../generated/generated_hvar.rs");

impl Hvar<'_> {
    /// Returns the change a location makes to the advance width of a glyph.
    ///
    /// The value carries every bit the item variation store computed. It is
    /// a caller that decides how to round it into a whole design unit, and
    /// implementations differ on that.
    ///
    /// Returns `None` where the table says nothing readable about the glyph.
    pub fn advance_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Option<F48Dot16> {
        variations::advance_delta(
            self.advance_width_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }

    /// Returns the change a location makes to the left side bearing of a
    /// glyph.
    ///
    /// The value carries every bit the item variation store computed. It is
    /// a caller that decides how to round it into a whole design unit, and
    /// implementations differ on that.
    ///
    /// Returns `None` where the table says nothing readable about the glyph.
    pub fn lsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Option<F48Dot16> {
        variations::item_delta(
            self.lsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }

    /// Returns the change a location makes to the right side bearing of a
    /// glyph.
    ///
    /// The value carries every bit the item variation store computed. It is
    /// a caller that decides how to round it into a whole design unit, and
    /// implementations differ on that.
    ///
    /// Returns `None` where the table says nothing readable about the glyph.
    pub fn rsb_delta(&self, glyph_id: GlyphId, coords: &[F2Dot14]) -> Option<F48Dot16> {
        variations::item_delta(
            self.rsb_mapping(),
            self.item_variation_store(),
            glyph_id,
            coords,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{tables::variations::DeltaSetIndexMap, FontRef, TableProvider};
    use types::{F2Dot14, F48Dot16, GlyphId};

    #[test]
    fn a_delta_keeps_the_fraction_the_variation_store_computed() {
        // These locations land between whole design units. Rounding is left
        // to a caller, and implementations disagree on how, so the value
        // reported here keeps what the store worked out.
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().unwrap();
        let gid = GlyphId::new(1);
        for (coord, expected) in [(-1.0, -113.0), (-0.75, -84.75), (-0.5, -56.5)] {
            let coords = [F2Dot14::from_f32(coord)];
            assert_eq!(
                hvar.advance_delta(gid, &coords),
                Some(F48Dot16::from_f64(expected)),
                "at {coord}"
            );
        }
    }

    #[test]
    fn a_default_location_moves_nothing() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().unwrap();
        let gid = GlyphId::new(1);
        assert_eq!(hvar.advance_delta(gid, &[]), Some(F48Dot16::ZERO));
        assert_eq!(hvar.lsb_delta(gid, &[]), Some(F48Dot16::ZERO));
        assert_eq!(hvar.rsb_delta(gid, &[]), Some(F48Dot16::ZERO));
    }

    #[test]
    fn a_mapping_the_font_does_not_state_is_absent() {
        // This font maps advances but neither side bearing.
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().unwrap();
        let gid = GlyphId::new(1);
        let coords = [F2Dot14::from_f32(-0.75)];
        assert!(hvar.advance_delta(gid, &coords).is_some());
        assert_eq!(hvar.lsb_delta(gid, &coords), None);
        assert_eq!(hvar.rsb_delta(gid, &coords), None);
    }

    #[test]
    fn advance_deltas() {
        let font = FontRef::new(font_test_data::VAZIRMATN_VAR).unwrap();
        let hvar = font.hvar().unwrap();
        let gid_a = GlyphId::new(1);
        // The odd quarters are what the store computed; the rounded form of
        // this accessor reported -85 and -56 for them.
        for (coord, expected) in [
            (-1.0, -113.0),
            (-0.75, -84.75),
            (-0.5, -56.5),
            (0.0, 0.0),
            (0.5, 29.5),
            (1.0, 59.0),
        ] {
            assert_eq!(
                hvar.advance_delta(gid_a, &[F2Dot14::from_f32(coord)]),
                Some(F48Dot16::from_f64(expected)),
                "at {coord}"
            );
        }
    }

    #[test]
    fn advance_deltas_from_hvar_with_truncated_adv_index_map() {
        let font = FontRef::new(font_test_data::HVAR_WITH_TRUNCATED_ADVANCE_INDEX_MAP).unwrap();
        let maxp = font.maxp().unwrap();
        let num_glyphs = maxp.num_glyphs();
        let hvar = font.hvar().unwrap();
        let Ok(DeltaSetIndexMap::Format0(adv_index_map)) = hvar.advance_width_mapping().unwrap()
        else {
            panic!("Expected DeltaSetIndexMap::Format0 for hvar.advance_width_mapping()");
        };
        assert!(adv_index_map.map_count() < num_glyphs);
        assert_eq!(num_glyphs, 24);
        assert_eq!(adv_index_map.map_count(), 15);
        let last_mapped_gid = adv_index_map.map_count() - 1;
        // We expect the last 10 glyphs to have the same advance width delta as
        // the last mapped glyph. Crucially, the accessor should answer for
        // them rather than reporting the glyph as out of bounds.
        for idx in last_mapped_gid..num_glyphs {
            let gid = GlyphId::new(idx as _);
            assert_eq!(
                hvar.advance_delta(gid, &[F2Dot14::from_f32(1.0)]),
                Some(F48Dot16::from_f64(100.0))
            );
        }
    }
}
