//! the [cmap] table
//!
//! [cmap]: https://docs.microsoft.com/en-us/typography/opentype/spec/cmap

include!("../../generated/generated_cmap.rs");

#[cfg(test)]
mod tests {
    use font_types::{BigEndian, Scalar};
    use read::{
        tables::cmap::{Cmap, CmapSubtable, PlatformId},
        FontData, FontRead,
    };

    use crate::{dump_table, tables::cmap as write};

    // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#unicode-platform-platform-id--0
    const UNICODE_BMP_ENCODING: u16 = 3;

    // I feel like i'm doing something wrong working this out this way...
    fn size_of_cmap4(seg_count: u16, gid_count: u16) -> u16 {
        8 * 2  // 8 uint16's
        + 2 * seg_count * 4  // 4 parallel arrays of len seg_count, 2 bytes per entry
        + 2 * gid_count // 2 bytes per gid in glyphIdArray
    }

    fn to_vec<T: Scalar>(bees: &[BigEndian<T>]) -> Vec<T> {
        bees.iter().map(|be| be.get()).collect()
    }

    // https://learn.microsoft.com/en-us/typography/opentype/spec/cmap#format-4-segment-mapping-to-delta-values
    #[test]
    fn generate_simple_cmap4() {
        // building the example given in the spec
        let seg_count: u16 = 4;
        let search_range: u16 = 8;
        let entry_selector: u16 = 4;
        let range_shift: u16 = 0;
        let end_code = vec![20u16, 90u16, 480u16, 0xffffu16];
        let start_code = vec![10u16, 30u16, 153u16, 0xffffu16];
        let id_delta = vec![-9i16, -18i16, -80i16, 1i16];
        let id_range_offsets = vec![0u16, 0u16, 0u16, 0u16];

        let cmap = write::Cmap::new(vec![write::EncodingRecord::new(
            PlatformId::Unicode,
            UNICODE_BMP_ENCODING,
            write::CmapSubtable::Format4(write::Cmap4::new(
                size_of_cmap4(seg_count, 0),
                0, // The language field must be set to zero for all 'cmap' subtables whose platform IDs are other than Macintosh
                seg_count * 2,
                search_range,
                entry_selector,
                range_shift,
                end_code.clone(),
                start_code.clone(),
                id_delta.clone(),
                id_range_offsets.clone(),
                vec![], // becauseour idRangeOffset's are 0 glyphIdArray is unused
            )),
        )]);

        let bytes = dump_table(&cmap).unwrap();
        let font_data = FontData::new(&bytes);
        let cmap = Cmap::read(font_data).unwrap();

        assert_eq!(1, cmap.encoding_records().len(), "{cmap:?}");
        let encoding_record = &cmap.encoding_records()[0];
        assert_eq!(
            (PlatformId::Unicode, UNICODE_BMP_ENCODING),
            (encoding_record.platform_id(), encoding_record.encoding_id())
        );

        let CmapSubtable::Format4(cmap4) = encoding_record.subtable(font_data).unwrap() else {
            panic!("Expected a cmap4 in {encoding_record:?}");
        };

        assert_eq!(start_code, to_vec(cmap4.start_code()));
        assert_eq!(end_code, to_vec(cmap4.end_code()));
        assert_eq!(id_delta, to_vec(cmap4.id_delta()));
        assert_eq!(id_range_offsets, to_vec(cmap4.id_range_offsets()));
    }
}
