//! test data shared between various fontations crates.

pub mod gdef;
pub mod gpos;
pub mod gsub;
pub mod layout;

pub static CMAP12_FONT1: &[u8] = include_bytes!("../test_data/ttf/cmap12_font1.ttf");

pub static CMAP14_FONT1: &[u8] = include_bytes!("../test_data/ttf/cmap14_font1.ttf");

pub static CMAP4_SYMBOL_PUA: &[u8] = include_bytes!("../test_data/ttf/cmap4_symbol_pua.ttf");

pub static COLR_GRADIENT_RECT: &[u8] =
    include_bytes!("../test_data/ttf/linear_gradient_rect_colr_1.ttf");

pub static VAZIRMATN_VAR: &[u8] = include_bytes!("../test_data/ttf/vazirmatn_var_trimmed.ttf");

pub static NAMES_ONLY: &[u8] = include_bytes!("../test_data/ttf/names_only.ttf");

pub static VAZIRMATN_VAR_GLYPHS: &str =
    include_str!("../test_data/extracted/vazirmatn_var_trimmed-glyphs.txt");

pub static SIMPLE_GLYF: &[u8] = include_bytes!("../test_data/ttf/simple_glyf.ttf");

pub static NOTO_SERIF_DISPLAY_TRIMMED: &[u8] =
    include_bytes!("../test_data/ttf/noto_serif_display_trimmed.ttf");

pub static CANTARELL_VF_TRIMMED: &[u8] =
    include_bytes!("../test_data/ttf/cantarell_vf_trimmed.ttf");

pub mod post {

    #[rustfmt::skip]
    pub static SIMPLE: &[u8] = &[
        0x00, 0x02, 0x00, 0x00, // version 2.0
        0x00, 0x00, 0x00, 0x00, // italic angle
        0xFF, 0xb5,             // underlinePosition -75
        0x00, 0x32,             // underlineThickness 50
        0x00, 0x00, 0x00, 0x00, // fixedpitch
        0x00, 0x00, 0x00, 0x00, // min/max mem:
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
        0x00, 0x0A,             // numGlyphs 10
                                // glyph name index:
        0x00, 0x00,              // glyph 0 -> name 0
        0x00, 0x00,             // glyph 1 -> name 0
        0x00, 0x03,              // glyph 2 -> name 3 ('space')
        0x00, 0x04,              // glyph 3 -> name 4 ('exclam')
        0x00, 0x06,
        0x00, 0x07,
        0x00, 0x08,
        0x01, 0x02,             // glyph 7 -> name 258 first custom
        0x01, 0x03,             // glyph 8 -> name 258 first custom
        0x01, 0x04,             // glyph 9 -> name 258 first custom
        0x05, 0x68, 0x65, 0x6c, 0x6c, 0x6f, // 5, h e l l o
        0x02, 0x68, 0x69, // 2, h i
        0x4, 0x68, 0x6f, 0x6c, 0x61, // 4, h o l a
    ];
}
