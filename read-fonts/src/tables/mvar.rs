//! The [MVAR (Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/mvar) table

use super::variations::{DeltaSetIndex, ItemVariationStore};
use types::F48Dot16;

/// Four-byte tags used to represent particular metric or other values.
pub mod tags {
    use font_types::Tag;

    /// Horizontal ascender.
    pub const HASC: Tag = Tag::new(b"hasc");
    /// Horizontal descender.
    pub const HDSC: Tag = Tag::new(b"hdsc");
    /// Horizontal line gap.
    pub const HLGP: Tag = Tag::new(b"hlgp");

    /// Horizontal clipping ascent.
    pub const HCLA: Tag = Tag::new(b"hcla");
    /// Horizontal clipping descent.
    pub const HCLD: Tag = Tag::new(b"hcld");

    /// Vertical ascender.
    pub const VASC: Tag = Tag::new(b"vasc");
    /// Vertical descender.
    pub const VDSC: Tag = Tag::new(b"vdsc");
    /// Vertical line gap.
    pub const VLGP: Tag = Tag::new(b"vlgp");

    /// Horizontal caret rise.
    pub const HCRS: Tag = Tag::new(b"hcrs");
    /// Horizontal caret run.
    pub const HCRN: Tag = Tag::new(b"hcrn");
    /// Horizontal caret offset.
    pub const HCOF: Tag = Tag::new(b"hcof");

    /// Vertical caret rise.
    pub const VCRS: Tag = Tag::new(b"vcrs");
    /// Vertical caret run.
    pub const VCRN: Tag = Tag::new(b"vcrn");
    /// Vertical caret offset.
    pub const VCOF: Tag = Tag::new(b"vcof");

    /// X-height.
    pub const XHGT: Tag = Tag::new(b"xhgt");
    /// Cap height.
    pub const CPHT: Tag = Tag::new(b"cpht");

    /// Subscript em x-offset.
    pub const SBXO: Tag = Tag::new(b"sbxo");
    /// Subscript em y-offset.
    pub const SBYO: Tag = Tag::new(b"sbyo");
    /// Subscript em x-size.
    pub const SBXS: Tag = Tag::new(b"sbxs");
    /// Subscript em y-size.
    pub const SBYS: Tag = Tag::new(b"sbys");

    /// Superscript em x-offset.
    pub const SPXO: Tag = Tag::new(b"spxo");
    /// Superscript em y-offset.
    pub const SPYO: Tag = Tag::new(b"spyo");
    /// Superscript em x-size.
    pub const SPXS: Tag = Tag::new(b"spxs");
    /// Superscript em y-size.
    pub const SPYS: Tag = Tag::new(b"spys");

    /// Strikeout size.
    pub const STRS: Tag = Tag::new(b"strs");
    /// Strikeout offset.
    pub const STRO: Tag = Tag::new(b"stro");

    /// Underline size.
    pub const UNDS: Tag = Tag::new(b"unds");
    /// Underline offset.
    pub const UNDO: Tag = Tag::new(b"undo");

    /// GaspRange\[0\]
    pub const GSP0: Tag = Tag::new(b"gsp0");
    /// GaspRange\[1\]
    pub const GSP1: Tag = Tag::new(b"gsp1");
    /// GaspRange\[2\]
    pub const GSP2: Tag = Tag::new(b"gsp2");
    /// GaspRange\[3\]
    pub const GSP3: Tag = Tag::new(b"gsp3");
    /// GaspRange\[4\]
    pub const GSP4: Tag = Tag::new(b"gsp4");
    /// GaspRange\[5\]
    pub const GSP5: Tag = Tag::new(b"gsp5");
    /// GaspRange\[6\]
    pub const GSP6: Tag = Tag::new(b"gsp6");
    /// GaspRange\[7\]
    pub const GSP7: Tag = Tag::new(b"gsp7");
    /// GaspRange\[8\]
    pub const GSP8: Tag = Tag::new(b"gsp8");
    /// GaspRange\[9\]
    pub const GSP9: Tag = Tag::new(b"gsp9");
}

include!("../../generated/generated_mvar.rs");

impl<'a> Mvar<'a> {
    /// The stride between value records.
    ///
    /// `valueRecordSize` may be larger than the record's own size, reserving
    /// trailing bytes per record; the stride is that field, not the fixed
    /// record size. `None` when it is smaller than the record, which is
    /// malformed.
    fn value_record_stride(&self) -> Option<usize> {
        let stride = self.value_record_size() as usize;
        (stride >= ValueRecord::RAW_BYTE_LEN).then_some(stride)
    }

    /// The value record at `index`, read at the `valueRecordSize` stride.
    ///
    /// The generated [`value_records`](Self::value_records) accessor assumes
    /// the fixed record size, so it reads the wrong bytes whenever
    /// `valueRecordSize` is larger; lookups go through this instead.
    fn value_record(&self, index: usize) -> Option<&'a ValueRecord> {
        let stride = self.value_record_stride()?;
        let offset = self
            .value_records_byte_range()
            .start
            .checked_add(index.checked_mul(stride)?)?;
        self.offset_data().read_ref_at::<ValueRecord>(offset).ok()
    }

    /// Binary searches the value records for `tag`, honoring the record
    /// stride. Records are in binary order of their tag.
    fn value_record_for_tag(&self, tag: Tag) -> Option<&'a ValueRecord> {
        use core::cmp::Ordering;
        let mut lo = 0usize;
        let mut hi = self.value_record_count() as usize;
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let record = self.value_record(mid)?;
            match tag.cmp(&record.value_tag()) {
                Ordering::Less => hi = mid,
                Ordering::Greater => lo = mid + 1,
                Ordering::Equal => return Some(record),
            }
        }
        None
    }
}

/// `MVAR` at one location, with the variation store resolved once.
///
/// Prefer this over [`Mvar::metric_delta`] when reading more than one metric;
/// that resolves the store on every lookup.
pub struct MvarInstance<'a> {
    mvar: Mvar<'a>,
    ivs: ItemVariationStore<'a>,
    coords: &'a [F2Dot14],
}

impl<'a> Mvar<'a> {
    /// Returns the table at a location.
    ///
    /// `None` at the default location, where every delta is zero, and for a
    /// font whose variation store cannot be read.
    pub fn at(&self, coords: &'a [F2Dot14]) -> Option<MvarInstance<'a>> {
        if coords.is_empty() {
            return None;
        }
        Some(MvarInstance {
            mvar: self.clone(),
            ivs: self.item_variation_store()?.ok()?,
            coords,
        })
    }
}

impl MvarInstance<'_> {
    /// Returns the delta for a metric, or `None` for one the font does not
    /// vary.
    ///
    /// Tags are in the [`tags`] module. The delta is in design units, added
    /// to the value the metric's own table holds, and left unrounded so a
    /// caller can round it its own way.
    pub fn get(&self, tag: Tag) -> Option<F48Dot16> {
        let record = self.mvar.value_record_for_tag(tag)?;
        self.ivs.compute_delta(
            DeltaSetIndex {
                outer: record.delta_set_outer_index(),
                inner: record.delta_set_inner_index(),
            },
            self.coords,
        )
    }
}

impl Mvar<'_> {
    /// Returns the metric delta for the specified tag and normalized
    /// variation coordinates. Possible tags are found in the [tags]
    /// module.
    ///
    /// `None` for a metric the font does not vary, and for one it states
    /// unreadably.
    pub fn metric_delta(&self, tag: Tag, coords: &[F2Dot14]) -> Option<Fixed> {
        let record = self.value_record_for_tag(tag)?;
        let ivs = self.item_variation_store()?.ok()?;
        Some(Fixed::from_i32(
            ivs.compute_delta(
                DeltaSetIndex {
                    outer: record.delta_set_outer_index(),
                    inner: record.delta_set_inner_index(),
                },
                coords,
            )?
            .to_i32(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FontData, FontRef, TableProvider};
    use font_test_data::bebuffer::BeBuffer;
    use types::F2Dot14;

    /// Twelve axes, and an `MVAR` varying several metrics.
    const VAR: &[u8] = font_test_data::AMSTELVAR_AVAR2_A;

    fn far() -> [F2Dot14; 12] {
        [F2Dot14::from_f32(1.0); 12]
    }

    #[test]
    fn the_default_location_has_no_instance() {
        // Every delta is zero there, so there is nothing to resolve.
        let font = FontRef::new(VAR).unwrap();
        assert!(font.mvar().unwrap().at(&[]).is_none());
    }

    #[test]
    fn an_instance_agrees_with_the_single_metric_accessor() {
        let font = FontRef::new(VAR).unwrap();
        let mvar = font.mvar().unwrap();
        let coords = far();
        let instance = mvar.at(&coords).expect("a location away from the default");
        let mut varied = 0;
        for record in mvar.value_records() {
            let tag = record.value_tag();
            let exact = instance.get(tag).expect("a tag the table states");
            let rounded = mvar.metric_delta(tag, &coords).unwrap();
            assert_eq!(exact.to_i32(), rounded.to_i32(), "{tag}");
            varied += (exact != F48Dot16::ZERO) as u32;
        }
        assert!(varied > 0, "no metric moved at the far end of every axis");
    }

    #[test]
    fn a_tag_the_font_does_not_state_is_absent() {
        let font = FontRef::new(VAR).unwrap();
        let mvar = font.mvar().unwrap();
        let coords = far();
        let instance = mvar.at(&coords).unwrap();
        assert!(instance.get(Tag::new(b"zzzz")).is_none());
    }

    #[test]
    fn an_instance_keeps_a_fraction_the_single_accessor_drops() {
        // `metric_delta` reports whole design units; the instance reports
        // what the variation store computed.
        let font = FontRef::new(VAR).unwrap();
        let mvar = font.mvar().unwrap();
        let coords = [F2Dot14::from_f32(0.37); 12];
        let instance = mvar.at(&coords).unwrap();
        let fractional = mvar
            .value_records()
            .iter()
            .filter_map(|record| instance.get(record.value_tag()))
            .any(|delta| delta.to_bits() & 0xFFFF != 0);
        assert!(fractional, "no metric delta carried a fraction");
    }

    // Builds an `MVAR` with three value records (`hasc`, `hdsc`, `hlgp`, in
    // binary tag order) whose stride is `value_record_size`. The spec lets
    // that exceed the eight bytes the record defines; the trailing bytes here
    // are a nonzero sentinel, so a read that strides by the fixed record size
    // instead of `valueRecordSize` lands on them and sees an invalid tag.
    //
    // The item variation store has one axis and one region peaking at 1.0,
    // with one delta per record: 100, -50 and 25. At the far end of the axis
    // each metric's delta is therefore its own stored value.
    fn mvar_with_stride(value_record_size: u16) -> BeBuffer {
        let stride = value_record_size as usize;
        assert!(stride >= ValueRecord::RAW_BYTE_LEN);
        let pad = stride - ValueRecord::RAW_BYTE_LEN;
        // The fixed header is 12 bytes; the store follows the record array.
        let store_offset = 12 + 3 * stride;

        let mut buf = BeBuffer::new()
            .push(MajorMinor::VERSION_1_0)
            .push(0u16) // reserved
            .push(value_record_size)
            .push(3u16) // value record count
            .push(store_offset as u16); // item variation store offset

        for (tag, inner) in [(b"hasc", 0u16), (b"hdsc", 1u16), (b"hlgp", 2u16)] {
            buf = buf
                .push(Tag::new(tag))
                .push(0u16) // delta-set outer index
                .push(inner) // delta-set inner index
                .extend(std::iter::repeat_n(0xFFu8, pad));
        }

        buf.push(1u16) // item variation store format
            .push(12u32) // region list offset
            .push(1u16) // variation data count
            .push(22u32) // variation data offset
            .push(1u16) // axis count
            .push(1u16) // region count
            .extend([F2Dot14::ZERO, F2Dot14::ONE, F2Dot14::ONE]) // start/peak/end
            .push(3u16) // item count
            .push(1u16) // word delta count
            .push(1u16) // region index count
            .push(0u16) // region index
            .extend([100i16, -50, 25])
    }

    #[test]
    fn a_wide_value_record_size_still_finds_every_record() {
        // Striding by the fixed record size rather than `valueRecordSize`
        // would read every record after the first at the wrong offset and
        // drop its delta.
        let bytes = mvar_with_stride(12);
        let mvar = Mvar::read(FontData::new(bytes.as_slice())).unwrap();
        assert_eq!(mvar.value_record_size(), 12);
        let coords = [F2Dot14::from_f32(1.0)];
        let instance = mvar.at(&coords).expect("a location away from the default");
        for (tag, expected) in [
            (Tag::new(b"hasc"), 100),
            (Tag::new(b"hdsc"), -50),
            (Tag::new(b"hlgp"), 25),
        ] {
            assert_eq!(
                mvar.metric_delta(tag, &coords).unwrap().to_i32(),
                expected,
                "metric_delta {tag}"
            );
            assert_eq!(
                instance.get(tag).unwrap().to_i32(),
                expected,
                "instance {tag}"
            );
        }
    }

    #[test]
    fn the_common_eight_byte_stride_agrees() {
        // The same records at the spec's usual stride, through the same code.
        let bytes = mvar_with_stride(8);
        let mvar = Mvar::read(FontData::new(bytes.as_slice())).unwrap();
        assert_eq!(mvar.value_record_size(), 8);
        let coords = [F2Dot14::from_f32(1.0)];
        let instance = mvar.at(&coords).unwrap();
        for (tag, expected) in [
            (Tag::new(b"hasc"), 100),
            (Tag::new(b"hdsc"), -50),
            (Tag::new(b"hlgp"), 25),
        ] {
            assert_eq!(mvar.metric_delta(tag, &coords).unwrap().to_i32(), expected);
            assert_eq!(instance.get(tag).unwrap().to_i32(), expected);
        }
    }

    #[test]
    fn a_value_record_size_below_the_record_is_malformed() {
        // `valueRecordSize` must be at least the defined record size; a smaller
        // one is rejected without reading past the array or panicking.
        let mut bytes = mvar_with_stride(12).as_slice().to_vec();
        bytes[6] = 0;
        bytes[7] = 4; // valueRecordSize = 4
        let mvar = Mvar::read(FontData::new(&bytes)).unwrap();
        assert_eq!(mvar.value_record_size(), 4);
        let coords = [F2Dot14::from_f32(1.0)];
        assert!(mvar.metric_delta(Tag::new(b"hasc"), &coords).is_none());
        let instance = mvar.at(&coords).unwrap();
        assert!(instance.get(Tag::new(b"hasc")).is_none());
    }
}
