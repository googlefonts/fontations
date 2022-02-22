use font_types::{BigEndian, Fixed, LongDateTime};

font_types::tables! {
    Head {
        /// Major version number of the font header table — set to 1.
        major_version: BigEndian<u16>,
        /// Minor version number of the font header table — set to 0.
        minor_version: BigEndian<u16>,
        /// Set by font manufacturer.
        font_revision: BigEndian<Fixed>,
        /// To compute: set it to 0, sum the entire font as uint32, then
        /// store 0xB1B0AFBA - sum. If the font is used as a component in a
        /// font collection file, the value of this field will be
        /// invalidated by changes to the file structure and font table
        /// directory, and must be ignored.
        checksum_adjustment: BigEndian<u32>,
        /// Set to 0x5F0F3CF5.
        magic_number: BigEndian<u32>,
        /// See the flags enum
        flags: BigEndian<u16>,
        /// Set to a value from 16 to 16384. Any value in this range is
        /// valid. In fonts that have TrueType outlines, a power of 2 is
        /// recommended as this allows performance optimizations in some
        /// rasterizers.
        units_per_em: BigEndian<u16>,
        /// Number of seconds since 12:00 midnight that started January 1st
        /// 1904 in GMT/UTC time zone.
        created: BigEndian<LongDateTime>,
        /// Number of seconds since 12:00 midnight that started January 1st
        /// 1904 in GMT/UTC time zone.
        modified: BigEndian<LongDateTime>,
        /// Minimum x coordinate across all glyph bounding boxes.
        x_min: BigEndian<i16>,
        /// Minimum y coordinate across all glyph bounding boxes.
        y_min: BigEndian<i16>,
        /// Maximum x coordinate across all glyph bounding boxes.
        x_max: BigEndian<i16>,
        /// Maximum y coordinate across all glyph bounding boxes.
        y_max: BigEndian<i16>,
        /// see somewhere else
        mac_style: BigEndian<u16>,
        /// Smallest readable size in pixels.
        lowest_rec_ppem: BigEndian<u16>,
        /// Deprecated (Set to 2).
        font_direction_hint: BigEndian<i16>,
        /// 0 for short offsets (Offset16), 1 for long (Offset32).
        index_to_loc_format: BigEndian<i16>,
        /// 0 for current format.
        glyph_data_format: BigEndian<i16>,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use font_types::{test_helpers::BeBuffer, FontRead};

    #[test]
    fn smoke_text() {
        let mut buf = BeBuffer::new();
        buf.extend([1u16, 0u16]);
        buf.push(Fixed::from_f64(2.8));
        buf.extend([42u32, 0x5f0f3cf5]);
        buf.extend([16u16, 4096]); // flags, upm
        buf.extend([LongDateTime::new(-500), LongDateTime::new(101)]);
        buf.extend([-100i16, -50, 400, 711]);
        buf.extend([0u16, 12]); // mac_style / ppem
        buf.extend([2i16, 1, 0]);

        let head = Head::read(&buf).unwrap();
        assert_eq!(head.major_version(), 1);
        assert_eq!(head.minor_version(), 0);
        assert_eq!(head.font_revision(), Fixed::from_f64(2.8));
        assert_eq!(head.units_per_em(), 4096);
        assert_eq!(head.created().as_secs(), -500);
        assert_eq!(head.y_min(), -50);
    }
}
