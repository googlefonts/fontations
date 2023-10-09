//! The [CBDT (Color Bitmap Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/cbdt) table

use super::bitmap::{BitmapData, BitmapLocation};

include!("../../generated/generated_cbdt.rs");

impl<'a> Cbdt<'a> {
    pub fn data(&self, location: &BitmapLocation) -> Result<BitmapData<'a>, ReadError> {
        super::bitmap::bitmap_data(self.offset_data(), location, true)
    }
}

#[cfg(test)]
mod tests {
    use super::super::bitmap::{BitmapDataFormat, SmallGlyphMetrics};
    use crate::{types::GlyphId, FontRef, TableProvider};

    #[test]
    fn read_cblc_1_cbdt_17() {
        let font = FontRef::new(font_test_data::EMBEDDED_BITMAPS).unwrap();
        let cblc = font.cblc().unwrap();
        let cbdt = font.cbdt().unwrap();
        let size = &cblc.bitmap_sizes()[0];
        // Metrics for size at index 0
        assert_eq!(size.hori.ascender(), 101);
        assert_eq!(size.hori.descender(), -27);
        assert_eq!(size.hori.width_max(), 136);
        assert_eq!(size.vert.ascender(), 101);
        assert_eq!(size.vert.descender(), -27);
        assert_eq!(size.vert.width_max(), 136);
        assert_eq!(size.start_glyph_index(), GlyphId::new(4));
        assert_eq!(size.end_glyph_index(), GlyphId::new(4));
        assert_eq!(size.ppem_x(), 109);
        assert_eq!(size.ppem_y(), 109);
        assert_eq!(size.bit_depth(), 32);
        let expected: &[(GlyphId, &[u8], SmallGlyphMetrics)] = &[(
            GlyphId::new(4),
            &raw_image_data(
                r#"89504e47 0d0a1a0a 0000000d 49484452
            00000088 00000080 08030000 00e737d1
            0d000000 8a504c54 4547704c 5c5c5c75
            75756d6d 6d727272 7474746a 69696c6c
            6c696969 6f6f6f6b 6b6b6968 68737272
            56555570 70706d6d 6d6b6b6b 61606068
            68686666 66646363 64636354 53535b59
            59616060 52515154 5353605f 5f5e5e5e
            5d5d5d5b 5b5b5150 505a5959 51505052
            51515756 56515050 51505052 51515453
            534f4e4e 4f4e4e51 50504f4e 4e4f4e4e
            4f4e4eb5 c8e4e900 00002e74 524e5300
            208040eb ff511070 a3c38fff 30ffffff
            80ffffea ff60bfff af9fffff ffff70ff
            10cfff40 ef8fff80 bfff50ff df66dbac
            80000002 6b494441 547801ed d8878eea
            400c85e1 13c2d27b 872dd909 1d26efff
            78b76107 4b9adb22 e2a0953f b5e8df36
            5e4c8579 06c61863 8c31c618 13d57e89
            21c5b718 414f547f b96988d8 a0566f42
            4deb85b4 45ec708c a1a6db23 3511eb1c
            63a8e9f4 4803b966 8f414f8f 45c8c5dc
            3a50d31f 30116b94 7a6da869 0c881cbe
            cdb10635 b5e1cda0 2bf76648 34777548
            1a220e07 149b5033 1cddc8e1 e311a943
            4d73c444 6c71eb40 4d1cfa9b 5d8e63a8
            194f485b ee0dc706 d4b42744 0e3f6111
            d474a624 46ae3f65 d033654d e41adc34
            77754a66 726f3876 a1a63527 72f80eb5
            e9026aba 73227775 c631869a e59cc817
            23730635 fdd57c75 b346aeb5 620b94a8
            31beebce 56643e5f 72dcac72 f30db5b8
            8473bc0a 6f6ff7eb 557ec157 f2ba8147
            1bbf1532 fec20779 2fa48483 7c14f285
            0f927c16 f2f883b8 e56701a9 4389a274
            cb960e64 b7cf63e2 a064b765 1172074a
            9f29a07e 903de441 c8016a92 23598ab8
            e4b8809a cd919c44 dc73dc41 cd39307c
            7464d073 617de476 dc52a8e9 5f988827
            6e1ba8b9 7a22875f 733c41cd c913397c
            ca517357 3d4944f4 cc41cd3e 30fc8edb
            1e6a5c68 f884db19 4ac4f099 88872a77
            550e7fd6 3f88cb3c eb83f5bd ce4db338
            df653eb7 3f337f97 715b9470 0e5fc8a2
            84ad28e4 6407b183 a81f24f1 85247834
            b7f1059c 1d4a9478 9645206e edd9c641
            4b1af8c7 f79feec5 48f6ac2f 46ecc5c8
            156a36a1 e13d8ba0 260d0cdf f70c5ac2
            c35f2bd9 5516da9b 03d42cfe b8ab09d4
            1c42c367 4ff26ed3 79862779 804fa126
            fae3aeae 2bfe6464 53e503fc bae24f46
            d6cff100 1fbea3f6 b9ed2b7e 80df693e
            c0b373e0 6f467b8a 0be871c9 e9a7c441
            e89f7e59 c018638c 31c61863 4cf5be03
            d8291f21 ceb2e953 00000000 49454e44
            ae426082"#,
            ),
            SmallGlyphMetrics {
                height: 128,
                width: 136,
                bearing_x: 0.into(),
                bearing_y: 101.into(),
                advance: 136,
            },
        )];
        for (gid, data, metrics) in expected {
            let location = size.location(cblc.offset_data(), *gid).unwrap();
            // all glyphs have data format == 17
            assert_eq!(location.format, 17);
            let bitmap_data = cbdt.data(&location).unwrap();
            let (img_fmt, img_data) = bitmap_data.content.extract_data();
            // all glyphs are PNG
            assert_eq!(img_fmt, BitmapDataFormat::Png);
            assert_eq!(img_data, *data);
            assert_eq!(bitmap_data.extract_small_metrics(), metrics);
        }
    }

    fn raw_image_data(raw: &str) -> Vec<u8> {
        raw.replace([' ', '\n', '\r'], "")
            .as_bytes()
            .chunks(2)
            .map(|str_hex| u8::from_str_radix(std::str::from_utf8(str_hex).unwrap(), 16).unwrap())
            .collect()
    }
}
