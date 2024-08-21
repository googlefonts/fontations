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

pub static CUBIC_GLYF: &[u8] = include_bytes!("../test_data/ttf/cubic_glyf.ttf");

pub static NOTO_SERIF_DISPLAY_TRIMMED: &[u8] =
    include_bytes!("../test_data/ttf/noto_serif_display_trimmed.ttf");

pub static NOTO_SERIF_DISPLAY_TRIMMED_GLYPHS: &str =
    include_str!("../test_data/extracted/noto_serif_display_trimmed-glyphs.txt");

pub static CANTARELL_VF_TRIMMED: &[u8] =
    include_bytes!("../test_data/ttf/cantarell_vf_trimmed.ttf");

pub static CANTARELL_VF_TRIMMED_GLYPHS: &str =
    include_str!("../test_data/extracted/cantarell_vf_trimmed-glyphs.txt");

pub static CHARSTRING_PATH_OPS: &[u8] = include_bytes!("../test_data/ttf/charstring_path_ops.ttf");

pub static EMBEDDED_BITMAPS: &[u8] = include_bytes!("../test_data/ttf/embedded_bitmaps.ttf");
pub static CBDT: &[u8] = include_bytes!("../test_data/ttf/cbdt.ttf");

pub static HVAR_WITH_TRUNCATED_ADVANCE_INDEX_MAP: &[u8] =
    include_bytes!("../test_data/ttf/hvar_with_truncated_adv_index_map.ttf");

pub static COLRV0V1: &[u8] = include_bytes!("../test_data/ttf/test_glyphs-glyf_colr_1.ttf");

pub static COLRV0V1_VARIABLE: &[u8] =
    include_bytes!("../test_data/ttf/test_glyphs-glyf_colr_1_variable.ttf");

pub static COLRV1_NO_CLIPLIST: &[u8] =
    include_bytes!("../test_data/ttf/test_glyphs-glyf_colr_1_no_cliplist.subset.ttf");

pub static CVAR: &[u8] = include_bytes!("../test_data/ttf/cvar.ttf");

pub static STARTING_OFF_CURVE: &[u8] = include_bytes!("../test_data/ttf/starts_off_curve.ttf");

pub static MOSTLY_OFF_CURVE: &[u8] = include_bytes!("../test_data/ttf/mostly_off_curve.ttf");

pub static INTERPOLATE_THIS: &[u8] = include_bytes!("../test_data/ttf/interpolate_this.ttf");

pub static MATERIAL_SYMBOLS_SUBSET: &[u8] =
    include_bytes!("../test_data/ttf/material_symbols_subset.ttf");

pub static GLYF_COMPONENTS: &[u8] = include_bytes!("../test_data/ttf/glyf_components.ttf");

pub static AUTOHINT_CMAP: &[u8] = include_bytes!("../test_data/ttf/autohint_cmap.ttf");

pub static NOTOSERIFHEBREW_AUTOHINT_METRICS: &[u8] =
    include_bytes!("../test_data/ttf/notoserifhebrew_autohint_metrics.ttf");

pub static TTHINT_SUBSET: &[u8] = include_bytes!("../test_data/ttf/tthint_subset.ttf");

pub static VORG: &[u8] = include_bytes!("../test_data/ttf/vorg.ttf");

pub static AHEM: &[u8] = include_bytes!("../test_data/ttf/ahem.ttf");

pub mod varc {
    pub static CJK_6868: &[u8] = include_bytes!("../test_data/ttf/varc-6868.ttf");
    pub static CONDITIONALS: &[u8] = include_bytes!("../test_data/ttf/varc-ac01-conditional.ttf");
}

pub mod closure {
    pub static SIMPLE: &[u8] = include_bytes!("../test_data/ttf/simple_closure.ttf");
    pub static SIMPLE_GLYPHS: &str = include_str!("../test_data/fea/simple_closure_glyphs.txt");
    pub static RECURSIVE: &[u8] = include_bytes!("../test_data/ttf/recursive_closure.ttf");
    pub static RECURSIVE_GLYPHS: &str =
        include_str!("../test_data/fea/recursive_closure_glyphs.txt");
    pub static CONTEXTUAL: &[u8] = include_bytes!("../test_data/ttf/context_closure.ttf");
    pub static CONTEXTUAL_GLYPHS: &str =
        include_str!("../test_data/fea/context_closure_glyphs.txt");
    pub static RECURSIVE_CONTEXTUAL: &[u8] =
        include_bytes!("../test_data/ttf/recursive_context_closure.ttf");
    pub static RECURSIVE_CONTEXTUAL_GLYPHS: &str =
        include_str!("../test_data/fea/recursive_context_closure_glyphs.txt");
    pub static VARIATIONS_CLOSURE: &[u8] =
        include_bytes!("../test_data/ttf/variations_closure.ttf");
    pub static VARIATIONS_GLYPHS: &str =
        include_str!("../test_data/fea/variations_closure_glyphs.txt");
}

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

pub mod cff2 {
    /// CFF2 example table
    /// <https://learn.microsoft.com/en-us/typography/opentype/spec/cff2#appendix-a-example-cff2-font>
    pub static EXAMPLE: &[u8] = &[
        0x02, 0x00, 0x05, 0x00, 0x07, 0xCF, 0x0C, 0x24, 0xC3, 0x11, 0x9B, 0x18, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x26, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0C, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1C,
        0x00, 0x01, 0x00, 0x02, 0xC0, 0x00, 0xE0, 0x00, 0x00, 0x00, 0xC0, 0x00, 0xC0, 0x00, 0xE0,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02,
        0x01, 0x01, 0x03, 0x05, 0x20, 0x0A, 0x20, 0x0A, 0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x05,
        0xF7, 0x06, 0xDA, 0x12, 0x77, 0x9F, 0xF8, 0x6C, 0x9D, 0xAE, 0x9A, 0xF4, 0x9A, 0x95, 0x9F,
        0xB3, 0x9F, 0x8B, 0x8B, 0x8B, 0x8B, 0x85, 0x9A, 0x8B, 0x8B, 0x97, 0x73, 0x8B, 0x8B, 0x8C,
        0x80, 0x8B, 0x8B, 0x8B, 0x8D, 0x8B, 0x8B, 0x8C, 0x8A, 0x8B, 0x8B, 0x97, 0x17, 0x06, 0xFB,
        0x8E, 0x95, 0x86, 0x9D, 0x8B, 0x8B, 0x8D, 0x17, 0x07, 0x77, 0x9F, 0xF8, 0x6D, 0x9D, 0xAD,
        0x9A, 0xF3, 0x9A, 0x95, 0x9F, 0xB3, 0x9F, 0x08, 0xFB, 0x8D, 0x95, 0x09, 0x1E, 0xA0, 0x37,
        0x5F, 0x0C, 0x09, 0x8B, 0x0C, 0x0B, 0xC2, 0x6E, 0x9E, 0x8C, 0x17, 0x0A, 0xDB, 0x57, 0xF7,
        0x02, 0x8C, 0x17, 0x0B, 0xB3, 0x9A, 0x77, 0x9F, 0x82, 0x8A, 0x8D, 0x17, 0x0C, 0x0C, 0xDB,
        0x95, 0x57, 0xF7, 0x02, 0x85, 0x8B, 0x8D, 0x17, 0x0C, 0x0D, 0xF7, 0x06, 0x13, 0x00, 0x00,
        0x00, 0x01, 0x01, 0x01, 0x1B, 0xBD, 0xBD, 0xEF, 0x8C, 0x10, 0x8B, 0x15, 0xF8, 0x88, 0x27,
        0xFB, 0x5C, 0x8C, 0x10, 0x06, 0xF8, 0x88, 0x07, 0xFC, 0x88, 0xEF, 0xF7, 0x5C, 0x8C, 0x10,
        0x06,
    ];
}

/// This setup to avoid cross-crate path construction and build.rs because both caused problems for google3
pub mod colrv1_json {
    /// Gets the expected value for a colrv1 json test
    pub fn expected(set_name: &str, settings: &[(&str, f32)]) -> &'static str {
        let mut key = Vec::with_capacity(1 + settings.len());
        key.push("colrv1_".to_string() + &set_name.to_ascii_lowercase());
        key.extend(settings.iter().map(|(t, v)| format!("{t}_{v}")));
        let key = key.join("_");
        // you could generate the cases in bash using something like:
        // for f in $(ls font-test-data/test_data/colrv1_json); do echo "\"$f\" => include_str!(\"../test_data/colrv1_json/$f\"),"; done
        match key.as_str() {
            "colrv1_clipbox" => include_str!("../test_data/colrv1_json/colrv1_clipbox"),
            "colrv1_clipbox_CLIO_200" => include_str!("../test_data/colrv1_json/colrv1_clipbox_CLIO_200"),
            "colrv1_colored_circles_v0" => include_str!("../test_data/colrv1_json/colrv1_colored_circles_v0"),
            "colrv1_composite_mode" => include_str!("../test_data/colrv1_json/colrv1_composite_mode"),
            "colrv1_extend_mode" => include_str!("../test_data/colrv1_json/colrv1_extend_mode"),
            "colrv1_extend_mode_COL1_-0.25_COL3_0.25" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL1_-0.25_COL3_0.25"),
            "colrv1_extend_mode_COL1_0.5_COL3_-0.5" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL1_0.5_COL3_-0.5"),
            "colrv1_extend_mode_COL1_-1.5" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL1_-1.5"),
            "colrv1_extend_mode_COL2_-0.3" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL2_-0.3"),
            "colrv1_extend_mode_COL3_0.5" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL3_0.5"),
            "colrv1_extend_mode_COL3_1" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL3_1"),
            "colrv1_extend_mode_COL3_1_COL2_1.5_COL1_2" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_COL3_1_COL2_1.5_COL1_2"),
            "colrv1_extend_mode_GRR0_-200_GRR1_-300" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRR0_-200_GRR1_-300"),
            "colrv1_extend_mode_GRR0_430_GRR1_40" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRR0_430_GRR1_40"),
            "colrv1_extend_mode_GRR0_-50_COL3_-2_COL2_-2_COL1_-0.9" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRR0_-50_COL3_-2_COL2_-2_COL1_-0.9"),
            "colrv1_extend_mode_GRR0_-50_COL3_-2_COL2_-2_COL1_-1.1" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRR0_-50_COL3_-2_COL2_-2_COL1_-1.1"),
            "colrv1_extend_mode_GRX0_1000_GRX1_-1000_GRR0_-1000_GRR1_200" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRX0_1000_GRX1_-1000_GRR0_-1000_GRR1_200"),
            "colrv1_extend_mode_GRX0_-1000_GRX1_-1000_GRR0_-1000_GRR1_-900" => include_str!("../test_data/colrv1_json/colrv1_extend_mode_GRX0_-1000_GRX1_-1000_GRR0_-1000_GRR1_-900"),
            "colrv1_foreground_color" => include_str!("../test_data/colrv1_json/colrv1_foreground_color"),
            "colrv1_gradient_p2_skewed" => include_str!("../test_data/colrv1_json/colrv1_gradient_p2_skewed"),
            "colrv1_gradient_stops_repeat" => include_str!("../test_data/colrv1_json/colrv1_gradient_stops_repeat"),
            "colrv1_no_cycle_multi_colrglyph" => include_str!("../test_data/colrv1_json/colrv1_no_cycle_multi_colrglyph"),
            "colrv1_paint_glyph_nested" => include_str!("../test_data/colrv1_json/colrv1_paint_glyph_nested"),
            "colrv1_paint_rotate" => include_str!("../test_data/colrv1_json/colrv1_paint_rotate"),
            "colrv1_paint_rotate_ROTA_40" => include_str!("../test_data/colrv1_json/colrv1_paint_rotate_ROTA_40"),
            "colrv1_paint_rotate_ROTX_-250_ROTY_-250" => include_str!("../test_data/colrv1_json/colrv1_paint_rotate_ROTX_-250_ROTY_-250"),
            "colrv1_paint_scale" => include_str!("../test_data/colrv1_json/colrv1_paint_scale"),
            "colrv1_paint_scale_SCOX_200_SCOY_200" => include_str!("../test_data/colrv1_json/colrv1_paint_scale_SCOX_200_SCOY_200"),
            "colrv1_paint_scale_SCSX_0.25_SCOY_0.25" => include_str!("../test_data/colrv1_json/colrv1_paint_scale_SCSX_0.25_SCOY_0.25"),
            "colrv1_paint_scale_SCSX_-1_SCOY_-1" => include_str!("../test_data/colrv1_json/colrv1_paint_scale_SCSX_-1_SCOY_-1"),
            "colrv1_paint_skew" => include_str!("../test_data/colrv1_json/colrv1_paint_skew"),
            "colrv1_paint_skew_SKCX_200_SKCY_200" => include_str!("../test_data/colrv1_json/colrv1_paint_skew_SKCX_200_SKCY_200"),
            "colrv1_paint_skew_SKXA_20" => include_str!("../test_data/colrv1_json/colrv1_paint_skew_SKXA_20"),
            "colrv1_paint_skew_SKYA_20" => include_str!("../test_data/colrv1_json/colrv1_paint_skew_SKYA_20"),
            "colrv1_paint_transform" => include_str!("../test_data/colrv1_json/colrv1_paint_transform"),
            "colrv1_paint_translate" => include_str!("../test_data/colrv1_json/colrv1_paint_translate"),
            "colrv1_paint_translate_TLDX_100_TLDY_100" => include_str!("../test_data/colrv1_json/colrv1_paint_translate_TLDX_100_TLDY_100"),
            "colrv1_sweep_coincident" => include_str!("../test_data/colrv1_json/colrv1_sweep_coincident"),
            "colrv1_sweep_varsweep" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep"),
            "colrv1_sweep_varsweep_SWC1_-0.25_SWC2_0.083333336_SWC3_0.083333336_SWC4_0.25" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWC1_-0.25_SWC2_0.083333336_SWC3_0.083333336_SWC4_0.25"),
            "colrv1_sweep_varsweep_SWPE_-45" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPE_-45"),
            "colrv1_sweep_varsweep_SWPE_-90" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPE_-90"),
            "colrv1_sweep_varsweep_SWPS_0" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPS_0"),
            "colrv1_sweep_varsweep_SWPS_-45_SWPE_45" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPS_-45_SWPE_45"),
            "colrv1_sweep_varsweep_SWPS_45_SWPE_-45_SWC1_-0.25_SWC2_-0.416687_SWC3_-0.583313_SWC4_-0.75" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPS_45_SWPE_-45_SWC1_-0.25_SWC2_-0.416687_SWC3_-0.583313_SWC4_-0.75"),
            "colrv1_sweep_varsweep_SWPS_90" => include_str!("../test_data/colrv1_json/colrv1_sweep_varsweep_SWPS_90"),
            "colrv1_variable_alpha" => include_str!("../test_data/colrv1_json/colrv1_variable_alpha"),
            "colrv1_variable_alpha_APH1_-0.7" => include_str!("../test_data/colrv1_json/colrv1_variable_alpha_APH1_-0.7"),
            "colrv1_variable_alpha_APH2_-0.7_APH3_-0.2" => include_str!("../test_data/colrv1_json/colrv1_variable_alpha_APH2_-0.7_APH3_-0.2"),
            _ => panic!("No entry for {key}, if this is a new test you might need to add a case"),
        }
    }
}

pub mod ift {
    use read_fonts::{be_buffer, be_buffer_add, test_helpers::BeBuffer, types::Uint24};

    pub static IFT_BASE: &[u8] = include_bytes!("../test_data/ttf/ift_base.ttf");

    // Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-1
    pub fn simple_format1() -> BeBuffer {
        let mut buffer = be_buffer! {
            /* ### Header ### */
            1u8,                    // format
            0u32,                   // reserved
            [1u32, 2, 3, 4],        // compat id
            2u16,                   // max entry id
            {2u16: "max_glyph_map_entry_id"},
            (Uint24::new(7)),       // glyph count
            {0u32: "glyph_map_offset"},
            0u32,                   // feature map offset
            0b00000010u8,           // applied entry bitmap (entry 1)

            8u16,                   // uri template length
            {b'A': "uri_template[0]"},
            {b'B': "uri_template[1]"},
            [b'C', b'D', b'E', b'F', 0xc9, 0xa4], // uri_template[2..7]

            {4u8: "patch_encoding"}, // = glyph keyed

            /* ### Glyph Map ### */
            {1u16: "glyph_map"},     // first mapped glyph
            {2u8: "entry_index[1]"},
            [1u8, 0, 1, 0, 0]        // entry index[2..6]
        };

        let offset = buffer.offset_for("glyph_map") as u32;
        buffer.write_at("glyph_map_offset", offset);

        buffer
    }

    #[rustfmt::skip]
    pub static U16_ENTRIES_FORMAT1: &[u8] = &[
        0x01,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x01, 0x2C,              // 21: max entry id = 300
        0x01, 0x2C,              // 23: max glyph map entry id = 300

              0x00, 0x00, 0x07,  // 25: glyph count
        0x00, 0x00, 0x00, 0x55,  // 28: glyph map offset (0x55 = 85)
        0x00, 0x00, 0x00, 0x00,  // 32: feature map offset

        // 36: applied entry bitmap (38 bytes)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,

        0x00, 0x08,              // 74: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 76: uriTemplate[8]

        0x04,                    // 84: patch encoding = glyph keyed

        // ## Glyph Map ##
        0x00, 0x02,              // 85: first mapped glyph
        // 88: entryIndex[2..6]
        0x00, 0x50, // gid 2
        0x00, 0x51, // gid 3
        0x01, 0x2c, // gid 4
        0x01, 0x2c, // gid 5
        0x00, 0x50, // gid 6
    ];

    #[rustfmt::skip]
    pub static FEATURE_MAP_FORMAT1: &[u8] = &[
        0x01,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x01, 0x90,              // 21: max entry id = 0x190
        0x01, 0x2c,              // 23: max glyph map entry id = 0x12C
        0x00, 0x00, 0x07,        // 25: glyph count
        0x00, 0x00, 0x00, 0x62,  // 28: glyph map offset (0x62 = 98)
        0x00, 0x00, 0x00, 0x6E,  // 32: feature map offset (0x6E = 110)

        // 36: applied entry bitmap (51 bytes) - 0x12b is applied
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // [0, 64)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // [64, 128)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // [128, 192)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // [192, 256)
        0x00, 0x00, 0x00, 0x00, 0x00, 0b00001000, 0x00, 0x00, // [256, 320)
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,       // [320, 384)
        0x00, 0x00, 0x00,                                     // [384, 400)

        0x00, 0x08,              // 87: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 89: uriTemplate[8]

        0x04,                    // 97: patch encoding = glyph keyed

        // ## Glyph Map ##
        0x00, 0x02,              // 98: first mapped glyph
        // 101: entryIndex[2..6]
        0x00, 0x50, // gid 2
        0x00, 0x51, // gid 3
        0x01, 0x2c, // gid 4
        0x01, 0x2b, // gid 5
        0x00, 0x50, // gid 6

        // ## Feature Map ##
        0x00, 0x03,               // 110: feature count (3)

        // FeatureRecord[0]
        b'd', b'l', b'i', b'g',   // 112: feature tag
        0x01, 0x90,               // 116: first new entry index (0x190 = 400)
        0x00, 0x01,               // 118: entry map count

        // FeatureRecord[1]
        b'l', b'i', b'g', b'a',   // 120: feature tag
        0x01, 0x80,               // 124: first new entry index
        0x00, 0x02,               // 126: entry map count

        // FeatureRecord[2]
        b'n', b'u', b'l', b'l',   // 120: feature tag
        0x01, 0x2D,               // 122: first new entry index
        0x00, 0x01,               // 124: entry map count

        // EntryMapRecord[0]: "dlig" + entry 0x51 => entry 0x190
        0x00, 0x51,               // first_entry_index
        0x00, 0x51,               // last_entry_index

        // EntryMapRecord[1]: "liga" + entry [0x50, 0x51] => entry 0x180
        0x00, 0x50,               // first_entry_index
        0x00, 0x51,               // last_entry_index

        // EntryMapRecord[2]: "liga" + entry 0x12c => entry 0x181
        0x01, 0x2b,               // first_entry_index
        0x01, 0x2c,               // last_entry_index

        // EntryMapRecord[3]: "null" + entry 0x0 => entry 0x12D
        0x00, 0x00,               // first_entry_index
        0x00, 0x00,               // last_entry_index
    ];

    // Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
    #[rustfmt::skip]
    pub static CODEPOINTS_ONLY_FORMAT2: &[u8] = &[
        0x02,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x04,                    // 21: default patch encoding = glyph keyed
        0x00, 0x00, 0x03,        // 22: entry count
        0x00, 0x00, 0x00, 0x2b,  // 25: entries offset (0x2b = 43)
        0x00, 0x00, 0x00, 0x00,  // 29: entry id string data = null

        0x00, 0x08,              // 33: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 35: uriTemplate[8]

        // Entries Array
        // Entry id = 1
        0b00010000,                         // 43: format = CODEPOINT_BIT_1
        0b00001101, 0b00000011, 0b00110001, // 44: codepoints = [0..17]

        // Entry id = 2
        0b01100000,                         // 47: format = CODEPOINT_BIT_2
        0x00, 0x05,                         // 48: bias = 6
        0b00001101, 0b00000011, 0b00110001, // 50: codepoints = [5..22]

        // Entry id = 3
        0b00100000,                         // 47: format = CODEPOINT_BIT_2
        0x00, 0x05,                         // 48: bias = 5
        0b00001101, 0b00000011, 0b00110001, // 50: codepoints = [5..22]
    ];

    #[rustfmt::skip]
    pub static FEATURES_AND_DESIGN_SPACE_FORMAT2: &[u8] = &[
        0x02,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x04,                    // 21: default patch encoding = glyph keyed
        0x00, 0x00, 0x03,        // 22: entry count
        0x00, 0x00, 0x00, 0x2b,  // 25: entries offset (0x2b = 43)
        0x00, 0x00, 0x00, 0x00,  // 29: entry id string data = null

        0x00, 0x08,              // 33: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 35: uriTemplate[8]

        // Entries Array
        // Entry id = 1
        0b00010001,                         // 43: format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

        0x02,                               // 44: feature count = 2
        b'l', b'i', b'g', b'a',             // 45: feature[0] = liga
        b's', b'm', b'c', b'p',             // 49: feature[0] = smcp

        0x00, 0x01,                         // 53: design space count = 1
        b'w', b'd', b't', b'h',             // 55: tag = wdth
        0x00, 0x00, 0x80, 0x00,             // 59: start = 0.5
        0x00, 0x01, 0x00, 0x00,             // 63: end = 1.0

        0b00001101, 0b00000011, 0b00110001, // 67: codepoints = [0..17]

        // Entries Array
        // Entry id = 2
        0b00010001,                         // 70: format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

        0x01,                               // 71: feature count = 1
        b'r', b'l', b'i', b'g',             // 72: feature[0] = rlig

        0x00, 0x00,                         // 76: design space count = 1

        0b00001101, 0b00000011, 0b00110001, // 78: codepoints = [0..17]

        // Entry id = 3
        0b000100001,                         // 81: format = CODEPOINT_BIT_2 | FEATURES_AND_DESIGN_SPACE

        0x01,                               // 82: feature count = 1
        b's', b'm', b'c', b'p',             // 83: feature[0] = smcp

        0x00, 0x03,                         // 87: design space count = 2
        b'w', b'g', b'h', b't',             // 89: tag = wght
        0x00, 0xC8, 0x00, 0x00,             // 93: start = 200
        0x02, 0xBC, 0x00, 0x00,             // 97: end = 700

        b'w', b'd', b't', b'h',             // 101: tag = wdth
        0x00, 0x00, 0x00, 0x00,             // 105: start = 0
        0x00, 0x00, 0x80, 0x00,             // 109: end = 0.5

        b'w', b'd', b't', b'h',             // 114: tag = wdth
        0x00, 0x02, 0x00, 0x00,             // 119: start = 2.0
        0x00, 0x02, 0x80, 0x00,             // 124: end = 2.5

        0x00, 0x05,                         // 128: bias = 5
        0b00001101, 0b00000011, 0b00110001, // 130: codepoints = [5..22]
    ];

    #[rustfmt::skip]
    pub static COPY_INDICES_FORMAT2: &[u8] = &[
        0x02,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x04,                    // 21: default patch encoding = glyph keyed
        0x00, 0x00, 0x09,        // 22: entry count 9
        0x00, 0x00, 0x00, 0x2b,  // 25: entries offset (0x2b = 43)
        0x00, 0x00, 0x00, 0x00,  // 29: entry id string data = null

        0x00, 0x08,              // 33: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 35: uriTemplate[8]

        // Entries Array

        // Entry id = 1
        0b00100000,                         // : format = CODEPOINT_BIT_2
        0x00, 0x05,                         // : bias = 5
        0b00001101, 0b00000011, 0b00110001, // : codepoints = [5..22]

        // Entry id = 2
        0b00100000,                         // : format = CODEPOINT_BIT_2
        0x00, 0x32,                         // : bias = 50
        0b00001101, 0b00000011, 0b00110001, // : codepoints = [50..67]

        // Entry id = 3
        0b00000001,                         //   : format = FEATURES_AND_DESIGN_SPACE

        0x01,                               //   : feature count = 1
        b'r', b'l', b'i', b'g',             //   : feature[0] = rlig

        0x00, 0x01,                         //   : design space count = 1
        b'w', b'g', b'h', b't',             //   : tag = wght
        0x00, 0xC8, 0x00, 0x00,             //   : start = 200
        0x02, 0xBC, 0x00, 0x00,             //   : end = 700

        // Entry id = 4
        0b00000001,                         //   : format = FEATURES_AND_DESIGN_SPACE

        0x01,                               //   : feature count = 1
        b'l', b'i', b'g', b'a',             //   : feature[0] = liga

        0x00, 0x01,                         //   : design space count = 1
        b'w', b'g', b'h', b't',             //   : tag = wght
        0x00, 0x32, 0x00, 0x00,             //   : start = 50
        0x00, 0x64, 0x00, 0x00,             //   : end = 100

        // Entry id = 5
        0b00000010,                         //   : format = COPY_INDICES
        0x01,                               //   : copy count = 1
        0x00, 0x00, 0x00,                   //   : copy 0

        // Entry id = 6
        0b00000010,                         //   : format = COPY_INDICES
        0x01,                               //   : copy count = 1
        0x00, 0x00, 0x02,                   //   : copy 2

        // Entry id = 7
        0b00000010,                         //   : format = COPY_INDICES
        0x04,                               //   : copy count = 4
        0x00, 0x00, 0x03,                   //   : copy 3
        0x00, 0x00, 0x02,                   //   : copy 2
        0x00, 0x00, 0x01,                   //   : copy 1
        0x00, 0x00, 0x00,                   //   : copy 0

        // Entry id = 8
        0b00000010,                         //   : format = COPY_INDICES
        0x02,                               //   : copy count = 2
        0x00, 0x00, 0x04,                   //   : copy 4
        0x00, 0x00, 0x05,                   //   : copy 5

        // Entry id = 9
        0b00100010,                         // : format = CODEPOINT_BIT_2 | COPY_INDICES
        0x01,                               // : copy count = 1
        0x00, 0x00, 0x00,                   // : copy 0
        0x00, 0x64,                         // : bias = 100
        0b00001101, 0b00000011, 0b00110001, // : codepoints = [100..117]
    ];

    // Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
    #[rustfmt::skip]
    pub static CUSTOM_IDS_FORMAT_2: &[u8] = &[
        0x02,                    // 0: format

        0x00, 0x00, 0x00, 0x00,  // 1: reserved

        0x00, 0x00, 0x00, 0x01,  // 5: compat id [0]
        0x00, 0x00, 0x00, 0x02,  // 9: compat id [1]
        0x00, 0x00, 0x00, 0x03,  // 13: compat id [2]
        0x00, 0x00, 0x00, 0x04,  // 17: compat id [3]

        0x04,                    // 21: default patch encoding = glyph keyed
        0x00, 0x00, 0x04,        // 22: entry count
        0x00, 0x00, 0x00, 0x2b,  // 25: entries offset (0x2b = 43)
        0x00, 0x00, 0x00, 0x00,  // 29: entry id string data = null

        0x00, 0x08,              // 33: uriTemplateLength
        b'A', b'B', b'C', b'D',
        b'E', b'F', 0xc9, 0xa4,  // 35: uriTemplate[8]

        // Entries Array
        // Entry id = 0
        0b00010100,                         // 43: format = CODEPOINT_BIT_1 | ID_DELTA
        0xFF, 0xFF, 0xFF,                   // 44: id delta = -1
        0b00001101, 0b00000011, 0b00110001, // 47: codepoints = [0..17]

        // Entry id = 6
        0b00100100,                         // 50: format = CODEPOINT_BIT_2 | ID_DELTA
        0x00, 0x00, 0x05,                   // 51: id delta 5
        0x00, 0x05,                         // 54: bias = 6
        0b00001101, 0b00000011, 0b00110001, // 56: codepoints = [5..22]

        // Entry id = 14
        0b01000100,                         // 59: format = ID_DELTA | IGNORED
        0x00, 0x00, 0x07,                   // 60: id delta 7

        // Entry id = 15
        0b00101000,                         // 63: format = CODEPOINT_BIT_2 | PATCH_ENCODING
        0x04,                               // 64: patch encoding = Glyph Keyed
        0x00, 0x0A,                         // 65: bias = 10
        0b00001101, 0b00000011, 0b00110001, // 67: codepoints = [10..27]
    ];
}
