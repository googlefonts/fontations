//! Test data for the IFT table
//!
//! Used for incremental font transfer. Specification:
//! <https://w3c.github.io/IFT/Overview.html>

use std::iter;

use font_types::{Int24, Tag, Uint24};

use crate::{be_buffer, bebuffer::BeBuffer};

pub static IFT_BASE: &[u8] = include_bytes!("../test_data/ttf/ift_base.ttf");

pub static ROBOTO_IFT: &[u8] = include_bytes!("../test_data/ift/roboto/Roboto-IFT.ttf");

pub static CFF_FONT: &[u8] = include_bytes!("../test_data/ttf/NotoSansJP-Regular.subset.otf");
pub static CFF2_FONT: &[u8] = include_bytes!("../test_data/ttf/NotoSansJP-VF.subset.otf");

pub const CFF_FONT_CHARSTRINGS_OFFSET: u32 = 0x1b9;
pub const CFF2_FONT_CHARSTRINGS_OFFSET: u32 = 0x8f;

// Using opcode format: https://w3c.github.io/IFT/Overview.html#url-templates
pub const RELATIVE_URL_TEMPLATE: &[u8] = b"\x04foo/\x80";
pub const ABSOLUTE_URL_TEMPLATE: &[u8] = b"\x0a//foo.bar/\x80";

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-1
pub fn simple_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        {1u8: "format"},        // format
        0u32,                   // reserved
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        6u16,                   // url template length
        4u8,
        {b'f': "url_template[1]"},
        {b'o': "url_template[2]"},
        [b'o', b'/', 128u8], // url_template[3..6]

        {3u8: "patch_format"}, // = glyph keyed

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn format1_with_dup_urls() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u32,                   // reserved
        [1u32, 2, 3, 4],        // compat id
        4u16,                   // max entry id
        {4u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        9u16,                   // url template length
        [8u8, b'f', b'o', b'o', b'/', b'b', b'a', b'a', b'r'], // url_template[9]

        {3u8: "patch_format"}, // = glyph keyed

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [3u8, 4, 0, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn simple_format1_with_one_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u8, 0u8, 0u8,          // reserved
        0b00000001u8,           // has charstrings offset
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        8u16,                   // url template length
        {b'A': "url_template[0]"},
        {b'B': "url_template[1]"},
        [b'C', b'D', b'E', b'F', 0xc9, 0xa4], // url_template[2..7]

        {3u8: "patch_format"}, // = glyph keyed

        456u32, // charstrings offset [0]

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn simple_format1_with_two_charstrings_offsets() -> BeBuffer {
    let mut buffer = be_buffer! {
        /* ### Header ### */
        1u8,                    // format
        0u8, 0u8, 0u8,          // reserved
        0b00000011u8,           // has cff and cff2 charstrings offset
        [1u32, 2, 3, 4],        // compat id
        2u16,                   // max entry id
        {2u16: "max_glyph_map_entry_id"},
        (Uint24::new(7)),       // glyph count
        {0u32: "glyph_map_offset"},
        0u32,                   // feature map offset
        0b00000010u8,           // applied entry bitmap (entry 1)

        8u16,                   // url template length
        {b'A': "url_template[0]"},
        {b'B': "url_template[1]"},
        [b'C', b'D', b'E', b'F', 0xc9, 0xa4], // url_template[2..7]

        {3u8: "patch_format"}, // = glyph keyed

        456u32, // charstrings offset [0]
        789u32, // charstrings offset [1]

        /* ### Glyph Map ### */
        {1u16: "glyph_map"},     // first mapped glyph
        {2u8: "entry_index[1]"},
        [1u8, 0, 1, 0, 0]        // entry index[2..6]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn u16_entries_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
      1u8,              // format
      0u32,             // reserved
      [1, 2, 3, 4u32],  // compat id

      300u16,           // max entry id
      300u16,           // max glyph map entry id

      (Uint24::new(7)), // glyph count

      {0u32: "glyph_map_offset"},
      {0u32: "feature_map_offset"},

      // applied entry bitmap (38 bytes)
      {0u8: "applied_entry_bitmap"},
      [
        0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0u8
      ],

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      3u8,                 // patch encoding = glyph keyed

      /* ### Glyph Map ### */
      {2u16: "glyph_map"}, // first mapped glyph

      // entryIndex[2..6]
      [80, 81, 300, 300, 80u16]
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    buffer
}

pub fn feature_map_format1() -> BeBuffer {
    let mut buffer = be_buffer! {
      /* ### Header ### */
      1u8,                    // format

      0u32,  // reserved

      [1u32, 2u32, 3u32, 4u32], // compat id

      400u16, // max entry id
      300u16, // max glyph map entry id
      (Uint24::new(7)), // glyph count

      {0u32: "glyph_map_offset"},
      {0u32: "feature_map_offset"},

      // applied entry bitmap (51 bytes) - 299 is applied
      {0u8: "applied_entries"},
      [
        0, 0, 0, 0, 0, 0, 0,           // [0, 64)
        0, 0, 0, 0, 0, 0, 0, 0,           // [64, 128)
        0, 0, 0, 0, 0, 0, 0, 0,           // [128, 192)
        0, 0, 0, 0, 0, 0, 0, 0,           // [192, 256)
        0, 0, 0, 0, 0u8
      ],
      {0b00001000u8: "applied_entries_296"},
      [
        0, 0,  // [256, 320)
        0, 0, 0, 0, 0, 0, 0, 0,           // [320, 384)
        0, 0, 0u8                         // [384, 400)
      ],

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      {3u8: "patch_format"},            // patch encoding = glyph keyed

      /* ### Glyph Map ### */
      {2u16: "glyph_map"}, // first mapped glyph

      // entryIndex[2..6]
      [
        80,     // gid 2
        81,     // gid 3
        300u16  // gid 4
      ],
      {299u16: "gid5_entry"},  // gid 5
      {80u16:  "gid6_entry"},  // gid 6

      // ## Feature Map ##
      {3u16: "feature_map"}, // feature count

      // FeatureRecord[0]
      {(Tag::new(b"dlig")): "FeatureRecord[0]"}, // feature tag
      400u16,                   // first new entry index
      1u16,                     // entry map count

      // FeatureRecord[1]
      {(Tag::new(b"liga")): "FeatureRecord[1]"}, // feature tag
      384u16,                   // first new entry index
      2u16,                     // entry map count

      // FeatureRecord[2]
      [b'n', b'u', b'l', b'l'], // feature tag
      301u16,                   // first new entry index
      1u16,                     // entry map count

      // EntryMapRecord[0]: "dlig" + entry 81 => entry 400
      81u16,                    // first_entry_index
      81u16,                    // last_entry_index

      // EntryMapRecord[1]: "liga" + entry [80, 81] => entry 384
      80u16,                    // first_entry_index
      81u16,                    // last_entry_index

      // EntryMapRecord[2]: "liga" + entry [299, 300] => entry 385
      299u16,                   // first_entry_index
      300u16,                   // last_entry_index

      // EntryMapRecord[3]: "null" + entry 0 => entry 301
      0u16,                     // first_entry_index
      0u16                      // last_entry_index
    };

    let offset = buffer.offset_for("glyph_map") as u32;
    buffer.write_at("glyph_map_offset", offset);

    let offset = buffer.offset_for("feature_map") as u32;
    buffer.write_at("feature_map_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn codepoints_only_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      {2u8: "format"},    // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      {(Uint24::new(4)): "entry_count"},
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 2
      {0b01100000u8: "entries[1]"},           // format = IGNORED | CODEPOINT_BIT_2
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 3
      {0b00100000u8: "entries[2]"},            // format = CODEPOINT_BIT_2
      5u16,                                    // bias
      [0b00001101, 0b00000011, 0b00110001u8],  // codepoints = [5..22]

      // Entry id = 4
      {0b00110000u8: "entries[3]"},            // format = CODEPOINT_BIT_1 | CODEPOINT_BIT_2
      (Uint24::new(80_000)),                   // bias
      [0b00001101, 0b00000011, 0b00110001u8]   // codepoints = [80_005..80_022]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn format2_with_one_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u8, 0u8, 0u8,                 // reserved
      {0b00000001u8: "field_flags"}, // has charstrings offset

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      (Uint24::new(1)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      8u16, // urlTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // urlTemplate[8]

      {456u32: "charstrings_offset"}, // charstrings offset [0]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn format2_with_two_charstrings_offset() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u8, 0u8, 0u8,          // reserved
      0b00000011u8,           // has cff and cff2 charstrings offset

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                // default patch encoding
      (Uint24::new(1)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      8u16, // urlTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // urlTemplate[8]

      456u32, // charstrings offset [0]
      789u32, // charstrings offset [1]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn features_and_design_space_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8, // format

      0u32, // reserved

      [1, 2, 3, 4u32], // compat id

      {3u8: "patch_format"}, // default patch encoding
      (Uint24::new(3)), // entry count
      {0u32: "entries_offset"},
      0u32, // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010001u8: "entries[0]"},          // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      2u8,                                // feature count = 2
      (Tag::new(b"liga")),                // feature[0] = liga
      (Tag::new(b"smcp")),                // feature[1] = smcp

      1u16,                               // design space count
      (Tag::new(b"wdth")),                // tag = wdth
      {0x8000u32: "wdth start"},          // start = 0.5
      0x10000u32,                         // end = 1.0

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 2
      {0b00010001u8: "entries[1]"},       // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count
      (Tag::new(b"rlig")),                // feature[0]

      0u16,                               // design space count

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 3
      {0b000100001u8: "entries[2]"},      // format = CODEPOINT_BIT_2 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count = 1
      (Tag::new(b"smcp")),                // feature[0] = smcp

      3u16,                               // design space count
      (Tag::new(b"wght")),                // tag = wght
      0x00C8_0000u32,                     // start = 200
      0x02BC_0000u32,                     // end = 700

      (Tag::new(b"wdth")),                // tag = wdth
      0x0u32,                             // start = 0.0
      0x8000u32,                          // end = 0.5

      (Tag::new(b"wdth")),                // tag = wdth
      0x0002_0000u32,                     // start = 2.0
      0x0002_8000u32,                     // end = 2.5

      5u16,                               // bias = 5
      [0b00001101, 0b00000011, 0b00110001u8] // codepoints = [5..22]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn child_indices_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      {3u8: "encoding"},          // default patch encoding = glyph keyed
      (Uint24::new(9)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      0u32,                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      // Entries Array

      // Entry id = 1
      {0b01100000u8: "entries[0]"},           // format = CODEPOINT_BIT_2 | IGNORED
      5u16,                                   // bias = 5
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 2
      {0b00100000u8: "entries[1]"},           // format = CODEPOINT_BIT_2
      50u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [50..67]

      // Entry id = 3
      {0b00000001u8: "entries[2]"},           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count = 1
      (Tag::new(b"rlig")),                    // feature[0] = rlig

      1u16,                                   // design space count = 1
      (Tag::new(b"wght")),                    // tag = wght
      0x00C8_0000u32,                         // start = 200
      0x02BC_0000u32,                         // end = 700

      // Entry id = 4
      {0b00000001u8: "entries[3]"},           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count
      (Tag::new(b"liga")),                    // feature[0] = liga

      1u16,                                   // design space count
      (Tag::new(b"wght")),                    // tag = wght
      0x0032_0000,                            // start = 50
      0x0064_0000,                            // end = 100

      // Entry id = 5
      {0b00000010u8: "entries[4]"},           // format = CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(0)),                       // child[0] = 0

      // Entry id = 6
      {0b00000010u8: "entries[5]"},           // format = CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(2)),                       // child

      // Entry id = 7
      {0b00000010u8: "entries[6]"},           // format = CHILD_INDICES
      {4u8: "entries[6]_child_count"},        // child count
      (Uint24::new(3)),                       // child[0] = 3
      {(Uint24::new(2)): "entries[6]_child"}, // child[1] = 2
      (Uint24::new(1)),                       // child[2] = 1
      (Uint24::new(0)),                       // child[3] = 0

      // Entry id = 8
      {0b00000010u8: "entries[7]"},           // format = CHILD_INDICES
      2u8,                                    // child count
      (Uint24::new(4)),                       // child[0] = 4
      (Uint24::new(5)),                       // child[1] = 5

      // Entry id = 9
      {0b00100010u8: "entries[8]"},           // format = CODEPOINT_BIT_2 | CHILD_INDICES
      1u8,                                    // child count
      (Uint24::new(0)),                       // chil[0] = 0
      100u16,                                 // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [100..117]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn custom_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                               // format

      0u32,                              // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      3u8,                               // default patch encoding = glyph keyed
      {(Uint24::new(4)): "entry_count"}, // entry count
      {0u32: "entries_offset"},          // entries offset
      0u32,                              // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      // Entries Array
      // Entry id = 0
      {0b00010100u8: "entries[0]"},           // format = CODEPOINT_BIT_1 | ID_DELTA
      {(Int24::new(-2)): "entries[0].id_delta"}, // id delta = -1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 6
      {0b00100100u8: "entries[1]"},            // format = CODEPOINT_BIT_2 | ID_DELTA
      {(Int24::new(10)): "entries[1].id_delta"},           // id delta = 5
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 14
      {0b01000100u8: "entries[2]"},                  // format = ID_DELTA | IGNORED
      {(Int24::new(14)): "id delta - ignored entry"}, // id delta = 7

      // Entry id = 15
      {0b00101000u8: "entries[3]"},           // format = CODEPOINT_BIT_2 | PATCH_FORMAT
      {3u8: "entry[4] encoding"},             // patch encoding = Glyph Keyed
      10u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [10..27]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn string_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      3u8,                      // default patch encoding = glyph keyed
      (Uint24::new(6)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      {0u32: "string_data_offset"},                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entry Data ### */

      // Entry id = ""
      {0b00000000u8: "entries"},              // format = {}

      // Entry id = abc
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(3)),                       // id length

      // Entry id = defg
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(4)),                       // id length

      // Entry id = defg
      0b00000000u8,                           // format = {}

      // Entry id = hij
      0b00000100u8,                           // format = ID_DELTA
      {(Uint24::new(3)): "entry[4] id length"},           // id length

      // Entry id = ""
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(0)),                                   // id length

      /* ### String Data ### */
      {b'a': "string_data"},
      [b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j']
    };

    let offset = buffer.offset_for("string_data") as u32;
    buffer.write_at("string_data_offset", offset);

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn string_ids_format2_with_preloads() -> BeBuffer {
    const CONTINUE_MASK: u32 = 1 << 23;
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      3u8,                      // default patch encoding = glyph keyed
      (Uint24::new(5)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      {0u32: "string_data_offset"},                     // entry id string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entry Data ### */

      // Entry id = ""
      {0b00000000u8: "entries"},              // format = {}

      // Entry id = {abc, "", defg}
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(CONTINUE_MASK | 3)),       // id length 3
      (Uint24::new(CONTINUE_MASK)),           // id length 0
      (Uint24::new(4)),                       // id length 4

      // Entry id = defg
      0b00000000u8,                           // format = {}

      // Entry id = hij
      0b00000100u8,                           // format = ID_DELTA
      {(Uint24::new(3)): "entry[4] id length"},           // id length

      // Entry id = ""
      0b00000100u8,                           // format = ID_DELTA
      (Uint24::new(0)),                                   // id length

      /* ### String Data ### */
      {b'a': "string_data"},
      [b'b', b'c', b'd', b'e', b'f', b'g', b'h', b'i', b'j']
    };

    let offset = buffer.offset_for("string_data") as u32;
    buffer.write_at("string_data_offset", offset);

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn table_keyed_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      {1u8: "encoding"},  // default patch encoding
      {(Uint24::new(1)): "entry_count"},   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4u8, b'f', b'o', b'o', b'/'],
      {128u8: "url_template_var_end"}, // urlTemplate[6]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00100100u8: "entries"},              // format = CODEPOINT_BIT_2
      {(Int24::new(0)): "id_delta"},
      {0u16: "bias"},                         // bias = 0
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn table_keyed_format2_with_preload_urls() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                // format

      0u32,               // reserved

      {1u32: "compat_id[0]"},
      {2u32: "compat_id[1]"},
      {3u32: "compat_id[2]"},
      {4u32: "compat_id[3]"},

      {3u8: "encoding"},  // default patch encoding = Glyph Keyed
      (Uint24::new(4)),   // entry count
      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      6u16, // urlTemplateLength
      [4, b'f', b'o', b'o', b'/', 128u8],  // urlTemplate[6]

      /* ### Entries Array ### */

      // Entry id = 1
      {0b00010000u8: "entries[0]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = {9, 10, 6}
      {0b00010100u8: "entries[1]"},           // format = CODEPOINT_BIT_1
      (Int24::new(15)),                       // delta +7
      (Int24::new(1)),                        // delta +0
      (Int24::new(-10)),                      // delta -5
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = {2, 3}
      {0b00010100u8: "entries[2]"},              // format = CODEPOINT_BIT_1
      (Int24::new(-11)),                      // delta -5
      (Int24::new(0)),                        // delta +0
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 4
      {0b00010000u8: "entries[3]"},           // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [0..17]
    };

    let offset = buffer.offset_for("entries[0]") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#table-keyed
pub fn table_keyed_patch() -> BeBuffer {
    let mut buffer = be_buffer! {
        {(Tag::new(b"iftk")): "tag"},
        0u32,                 // reserved
        {1u32: "compat_id"},
        [2, 3, 4u32],       // compat id
        3u16,                 // patch count

        // patch_offsets[3]
        {0u32: "patch_off[0]"},
        {0u32: "patch_off[1]"},
        {0u32: "patch_off[2]"},
        {0u32: "patch_off[3]"},

        // patch[0]
        {(Tag::new(b"tab1")): "patch[0]"},
        0u8,       // flags
        {29u32: "decompressed_len[0]"},     // max decompressed length
        // brotli stream (w/ shared dict)
        [0xa1, 0xe0, 0x00, 0xc0, 0x2f, 0x3a, 0x38, 0xf4, 0x01, 0xd1, 0xaf, 0x54, 0x84, 0x14, 0x71,
         0x2a, 0x80, 0x04, 0xa2, 0x1c, 0xd3, 0xdd, 0x07u8],

         // patch[1]
        {(Tag::new(b"tab2")): "patch[1]"},
        {1u8: "flags[1]"},  // flags (REPLACEMENT)
        30u32,              // max decompressed length
        // brotli stream (w/o shared dict)
        [0xa1, 0xe8, 0x00, 0xc0, 0xef, 0x48, 0x9d, 0xfa, 0xdc, 0xf1, 0xc2, 0xac, 0xc5, 0xde, 0xe4, 0xf4,
         0xb4, 0x02, 0x48, 0x98, 0x98, 0x52, 0x64, 0xa8, 0x50, 0x20, 0x29, 0x75, 0x0bu8],

         // patch[2]
        {(Tag::new(b"tab3")): "patch[2]"},
        {2u8: "flags[2]"}, // flags (DROP)
        {0u32: "end"}      // max decompressed length
    };

    let offset = buffer.offset_for("patch[0]") as u32;
    buffer.write_at("patch_off[0]", offset);

    let offset = buffer.offset_for("patch[1]") as u32;
    buffer.write_at("patch_off[1]", offset);

    let offset = buffer.offset_for("patch[2]") as u32;
    buffer.write_at("patch_off[2]", offset);

    let offset = (buffer.offset_for("end") + 4) as u32;
    buffer.write_at("patch_off[3]", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#table-keyed
pub fn noop_table_keyed_patch() -> BeBuffer {
    be_buffer! {
        {(Tag::new(b"iftk")): "tag"},
        0u32,                 // reserved
        [1, 2, 3, 4u32],       // compat id
        0u16,                 // patch count

        // patch_offsets[1]
        {0u32: "patch_off[0]"}
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyph-keyed
pub fn glyph_keyed_patch_header() -> BeBuffer {
    be_buffer! {
      {(Tag::new(b"ifgk")): "format"}, // format
      0u32,                // reserved
      0u8,                 // flags (0 = u16 gids)
      {6u32: "compatibility_id"},
      [7, 8, 9u32],     // compatibility id
      {0u32: "max_uncompressed_length"}
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn noop_glyf_glyph_patches() -> BeBuffer {
    be_buffer! {
      0u32,       // glyph count
      {1u8: "table_count"},        // table count

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 1
      0u32
    }
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      5u32,       // glyph count
      {1u8: "table_count"},        // table count

      // glyph ids * 5
      [2, 7u16],
      {8u16: "gid_8"},
      [9u16],
      {13u16: "gid_13"},

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_2_offset"},
      {0u32: "gid_7_offset"},
      {0u32: "gid_8_offset"},
      {0u32: "gid_9_offset"},
      {0u32: "gid_13_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_2_data"},
      [b'b', b'c'],

      {b'd': "gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_8_and_9_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_13_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_2_data") as u32;
    buffer.write_at("gid_2_offset", offset);

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_8_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_9_offset", offset);

    let offset = buffer.offset_for("gid_13_data") as u32;
    buffer.write_at("gid_13_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u16_glyph_patches_2() -> BeBuffer {
    let mut buffer = be_buffer! {
      3u32,       // glyph count
      {1u8: "table_count"},        // table count

      // glyph ids * 3
      7u16,
      12u16,
      14u16,

      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_7_offset"},
      {0u32: "gid_12_offset"},
      {0u32: "gid_14_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'q': "gid_7_data"},
      [b'r'],

      {b's': "gid_12_data"},
      [b't', b'u'],

      {b'v': "gid_14_data"}
    };

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_12_data") as u32;
    buffer.write_at("gid_12_offset", offset);

    let offset = buffer.offset_for("gid_14_data") as u32;
    buffer.write_at("gid_14_offset", offset);
    buffer.write_at("end_offset", offset + 1);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_u24_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      5u32,       // glyph count
      1u8,        // table count
      (Uint24::new(2)), (Uint24::new(7)), (Uint24::new(8)), (Uint24::new(9)), (Uint24::new(13)),   // glyph ids * 5
      (Tag::new(b"glyf")),   // tables * 1

      // glyph data offsets * 6
      {0u32: "gid_2_offset"},
      {0u32: "gid_7_offset"},
      {0u32: "gid_8_offset"},
      {0u32: "gid_9_offset"},
      {0u32: "gid_13_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_2_data"},
      [b'b', b'c'],

      {b'd': "gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_8_and_9_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_13_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_2_data") as u32;
    buffer.write_at("gid_2_offset", offset);

    let offset = buffer.offset_for("gid_7_data") as u32;
    buffer.write_at("gid_7_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_8_offset", offset);

    let offset = buffer.offset_for("gid_8_and_9_data") as u32;
    buffer.write_at("gid_9_offset", offset);

    let offset = buffer.offset_for("gid_13_data") as u32;
    buffer.write_at("gid_13_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#glyphpatches
pub fn glyf_and_gvar_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      3u32,       // glyph count
      2u8,        // table count
      [2, 7, 8u16],   // glyph ids * 3
      {(Tag::new(b"glyf")): "glyf_tag"}, // tables[0]
      {(Tag::new(b"gvar")): "gvar_tag"}, // tables[1]

      // glyph data offsets * 7
      {0u32: "glyf_gid_2_offset"},
      {0u32: "glyf_gid_7_offset"},
      {0u32: "glyf_gid_8_offset"},
      {0u32: "gvar_gid_2_offset"},
      {0u32: "gvar_gid_7_offset"},
      {0u32: "gvar_gid_8_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "glyf_gid_2_data"},
      [b'b', b'c'],

      {b'd': "glyf_gid_7_data"},
      [b'e', b'f', b'g'],

      {b'h': "glyf_gid_8_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gvar_gid_2_data"},
      [b'n'],

      {b'o': "gvar_gid_7_data"},
      [b'p', b'q'],

      {b'r': "gvar_gid_8_data"}
    };

    let offset = buffer.offset_for("glyf_gid_2_data") as u32;
    buffer.write_at("glyf_gid_2_offset", offset);
    let offset = buffer.offset_for("glyf_gid_7_data") as u32;
    buffer.write_at("glyf_gid_7_offset", offset);
    let offset = buffer.offset_for("glyf_gid_8_data") as u32;
    buffer.write_at("glyf_gid_8_offset", offset);

    let offset = buffer.offset_for("gvar_gid_2_data") as u32;
    buffer.write_at("gvar_gid_2_offset", offset);
    let offset = buffer.offset_for("gvar_gid_7_data") as u32;
    buffer.write_at("gvar_gid_7_offset", offset);
    let offset = buffer.offset_for("gvar_gid_8_data") as u32;
    buffer.write_at("gvar_gid_8_offset", offset);
    buffer.write_at("end_offset", offset + 1);

    buffer
}

pub fn cff_u16_glyph_patches() -> BeBuffer {
    let mut buffer = be_buffer! {
      4u32,       // glyph count
      {1u8: "table_count"},        // table count

      // 4 glyph ids
      [1,      // first gid
       38,
       47,
       59u16], // last gid

      {(Tag::new(b"CFF ")): "tag"},   // tables * 1

      // 5 glyph data offsets
      {0u32: "gid_1_offset"},
      {0u32: "gid_38_offset"},
      {0u32: "gid_47_offset"},
      {0u32: "gid_59_offset"},
      {0u32: "end_offset"},

      // data blocks
      {b'a': "gid_1_data"},
      [b'b', b'c'],

      {b'd': "gid_38_data"},
      [b'e', b'f', b'g'],

      {b'h': "gid_47_data"},
      [b'i', b'j', b'k', b'l'],

      {b'm': "gid_59_data"},
      [b'n']
    };

    let offset = buffer.offset_for("gid_1_data") as u32;
    buffer.write_at("gid_1_offset", offset);

    let offset = buffer.offset_for("gid_38_data") as u32;
    buffer.write_at("gid_38_offset", offset);

    let offset = buffer.offset_for("gid_47_data") as u32;
    buffer.write_at("gid_47_offset", offset);

    let offset = buffer.offset_for("gid_59_data") as u32;
    buffer.write_at("gid_59_offset", offset);
    buffer.write_at("end_offset", offset + 2);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn short_gvar_with_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16,

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn long_gvar_with_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0b00000000_00000001u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u32: "glyph_offset[0]"},
      {0u32: "glyph_offset[1]"},
      {0u32: "glyph_offset[2]"},
      {0u32: "glyph_offset[3]"},
      {0u32: "glyph_offset[4]"},
      {0u32: "glyph_offset[5]"},
      {0u32: "glyph_offset[6]"},
      {0u32: "glyph_offset[7]"},
      {0u32: "glyph_offset[8]"},
      {0u32: "glyph_offset[9]"},
      {0u32: "glyph_offset[10]"},
      {0u32: "glyph_offset[11]"},
      {0u32: "glyph_offset[12]"},
      {0u32: "glyph_offset[13]"},
      {0u32: "glyph_offset[14]"},
      {0u32: "glyph_offset[15]"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16,

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = (buffer.offset_for("glyph_0") - data_offset) as u32;
    let glyph8_offset = (buffer.offset_for("glyph_8") - data_offset) as u32;
    let end_offset = (buffer.offset_for("end") + 1 - data_offset) as u32;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

pub fn short_gvar_with_no_shared_tuples() -> BeBuffer {
    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let mut buffer = be_buffer! {
      // HEADER
      1u16,  // major version
      0u16,  // minor version
      1u16,  // axis count
      {0u16: "shared_tuple_count"},
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"}
    };

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("shared_tuples_offset", data_offset as u32);
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}

pub fn short_gvar_near_maximum_offset_size() -> BeBuffer {
    // This is a short offset gvar table whose glyph data is at the maximum representable size with short offsets

    // This gvar has the correct header and tuple structure but the per glyph variation data is not valid.
    // Meant for testing with IFT glyph keyed patching which treats the per glyph data as opaque blobs.
    let buffer = be_buffer! {
      // HEADER
      1u16,  // major version
      0u16,  // minor version
      1u16,  // axis count
      {0u16: "shared_tuple_count"},
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}
    };

    // Glyph 0
    let mut buffer = buffer.extend(iter::repeat(1u8).take(131065));

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("shared_tuples_offset", data_offset as u32);
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    buffer.write_at("glyph_offset[0]", 0u16);
    buffer.write_at("glyph_offset[1]", 65533u16);
    buffer.write_at("glyph_offset[2]", 65533u16);
    buffer.write_at("glyph_offset[3]", 65533u16);
    buffer.write_at("glyph_offset[4]", 65533u16);
    buffer.write_at("glyph_offset[5]", 65533u16);
    buffer.write_at("glyph_offset[6]", 65533u16);
    buffer.write_at("glyph_offset[7]", 65533u16);
    buffer.write_at("glyph_offset[8]", 65533u16);
    buffer.write_at("glyph_offset[9]", 65533u16);
    buffer.write_at("glyph_offset[10]", 65533u16);
    buffer.write_at("glyph_offset[11]", 65533u16);
    buffer.write_at("glyph_offset[12]", 65533u16);
    buffer.write_at("glyph_offset[13]", 65533u16);
    buffer.write_at("glyph_offset[14]", 65533u16);
    buffer.write_at("glyph_offset[15]", 65533u16);

    buffer
}

/// <https://learn.microsoft.com/en-us/typography/opentype/spec/gvar>
pub fn out_of_order_gvar_with_shared_tuples() -> BeBuffer {
    let mut buffer = be_buffer! {
      // HEADER
      1u16, // major version
      0u16, // minor version
      1u16, // axis count
      3u16, // sharedTupleCount
      {0u32: "shared_tuples_offset"},
      15u16, // glyph count
      0u16,  // flags
      {0u32: "glyph_variation_data_offset"},

      // OFFSETS
      {0u16: "glyph_offset[0]"},
      {0u16: "glyph_offset[1]"},
      {0u16: "glyph_offset[2]"},
      {0u16: "glyph_offset[3]"},
      {0u16: "glyph_offset[4]"},
      {0u16: "glyph_offset[5]"},
      {0u16: "glyph_offset[6]"},
      {0u16: "glyph_offset[7]"},
      {0u16: "glyph_offset[8]"},
      {0u16: "glyph_offset[9]"},
      {0u16: "glyph_offset[10]"},
      {0u16: "glyph_offset[11]"},
      {0u16: "glyph_offset[12]"},
      {0u16: "glyph_offset[13]"},
      {0u16: "glyph_offset[14]"},
      {0u16: "glyph_offset[15]"},

      // GLYPH VARIATION DATA
      {1u8: "glyph_0"}, [2, 3, 4u8],
      {5u8: "glyph_8"}, [6, 7, 8, 9u8], {10u8: "end"},

      // SHARED TUPLES
      {42u16: "sharedTuples[0]"},
      13u16,
      25u16
    };

    let offset = buffer.offset_for("sharedTuples[0]") as u32;
    buffer.write_at("shared_tuples_offset", offset);

    let data_offset = buffer.offset_for("glyph_0");
    buffer.write_at("glyph_variation_data_offset", data_offset as u32);

    let glyph0_offset = ((buffer.offset_for("glyph_0") - data_offset) / 2) as u16;
    let glyph8_offset = ((buffer.offset_for("glyph_8") - data_offset) / 2) as u16;
    let end_offset = ((buffer.offset_for("end") + 1 - data_offset) / 2) as u16;

    buffer.write_at("glyph_offset[0]", glyph0_offset);
    buffer.write_at("glyph_offset[1]", glyph8_offset);
    buffer.write_at("glyph_offset[2]", glyph8_offset);
    buffer.write_at("glyph_offset[3]", glyph8_offset);
    buffer.write_at("glyph_offset[4]", glyph8_offset);
    buffer.write_at("glyph_offset[5]", glyph8_offset);
    buffer.write_at("glyph_offset[6]", glyph8_offset);
    buffer.write_at("glyph_offset[7]", glyph8_offset);
    buffer.write_at("glyph_offset[8]", glyph8_offset);
    buffer.write_at("glyph_offset[9]", end_offset);
    buffer.write_at("glyph_offset[10]", end_offset);
    buffer.write_at("glyph_offset[11]", end_offset);
    buffer.write_at("glyph_offset[12]", end_offset);
    buffer.write_at("glyph_offset[13]", end_offset);
    buffer.write_at("glyph_offset[14]", end_offset);
    buffer.write_at("glyph_offset[15]", end_offset);

    buffer
}
pub const ROBOTO_PATCHES: &[(&str, &[u8])] = &[
    (
        "0400.ift_tk",
        include_bytes!("../test_data/ift/roboto/0400.ift_tk"),
    ),
    (
        "040G.ift_tk",
        include_bytes!("../test_data/ift/roboto/040G.ift_tk"),
    ),
    (
        "0410.ift_tk",
        include_bytes!("../test_data/ift/roboto/0410.ift_tk"),
    ),
    (
        "041G.ift_tk",
        include_bytes!("../test_data/ift/roboto/041G.ift_tk"),
    ),
    (
        "0420.ift_tk",
        include_bytes!("../test_data/ift/roboto/0420.ift_tk"),
    ),
    (
        "042G.ift_tk",
        include_bytes!("../test_data/ift/roboto/042G.ift_tk"),
    ),
    (
        "0430.ift_tk",
        include_bytes!("../test_data/ift/roboto/0430.ift_tk"),
    ),
    (
        "043G.ift_tk",
        include_bytes!("../test_data/ift/roboto/043G.ift_tk"),
    ),
    (
        "0440.ift_tk",
        include_bytes!("../test_data/ift/roboto/0440.ift_tk"),
    ),
    (
        "044G.ift_tk",
        include_bytes!("../test_data/ift/roboto/044G.ift_tk"),
    ),
    (
        "0450.ift_tk",
        include_bytes!("../test_data/ift/roboto/0450.ift_tk"),
    ),
    (
        "045G.ift_tk",
        include_bytes!("../test_data/ift/roboto/045G.ift_tk"),
    ),
    (
        "0460.ift_tk",
        include_bytes!("../test_data/ift/roboto/0460.ift_tk"),
    ),
    (
        "046G.ift_tk",
        include_bytes!("../test_data/ift/roboto/046G.ift_tk"),
    ),
    (
        "0470.ift_tk",
        include_bytes!("../test_data/ift/roboto/0470.ift_tk"),
    ),
    (
        "047G.ift_tk",
        include_bytes!("../test_data/ift/roboto/047G.ift_tk"),
    ),
    (
        "0480.ift_tk",
        include_bytes!("../test_data/ift/roboto/0480.ift_tk"),
    ),
    (
        "048G.ift_tk",
        include_bytes!("../test_data/ift/roboto/048G.ift_tk"),
    ),
    (
        "0490.ift_tk",
        include_bytes!("../test_data/ift/roboto/0490.ift_tk"),
    ),
    (
        "049G.ift_tk",
        include_bytes!("../test_data/ift/roboto/049G.ift_tk"),
    ),
    (
        "04A0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04A0.ift_tk"),
    ),
    (
        "04AG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04AG.ift_tk"),
    ),
    (
        "04B0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04B0.ift_tk"),
    ),
    (
        "04BG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04BG.ift_tk"),
    ),
    (
        "04C0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04C0.ift_tk"),
    ),
    (
        "04CG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04CG.ift_tk"),
    ),
    (
        "04D0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04D0.ift_tk"),
    ),
    (
        "04DG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04DG.ift_tk"),
    ),
    (
        "04E0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04E0.ift_tk"),
    ),
    (
        "04EG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04EG.ift_tk"),
    ),
    (
        "04F0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04F0.ift_tk"),
    ),
    (
        "04FG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04FG.ift_tk"),
    ),
    (
        "04G0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04G0.ift_tk"),
    ),
    (
        "04GG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04GG.ift_tk"),
    ),
    (
        "04H0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04H0.ift_tk"),
    ),
    (
        "04HG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04HG.ift_tk"),
    ),
    (
        "04I0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04I0.ift_tk"),
    ),
    (
        "04IG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04IG.ift_tk"),
    ),
    (
        "04J0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04J0.ift_tk"),
    ),
    (
        "04JG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04JG.ift_tk"),
    ),
    (
        "04K0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04K0.ift_tk"),
    ),
    (
        "04KG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04KG.ift_tk"),
    ),
    (
        "04L0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04L0.ift_tk"),
    ),
    (
        "04LG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04LG.ift_tk"),
    ),
    (
        "04M0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04M0.ift_tk"),
    ),
    (
        "04MG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04MG.ift_tk"),
    ),
    (
        "04N0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04N0.ift_tk"),
    ),
    (
        "04NG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04NG.ift_tk"),
    ),
    (
        "04O0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04O0.ift_tk"),
    ),
    (
        "04OG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04OG.ift_tk"),
    ),
    (
        "04P0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04P0.ift_tk"),
    ),
    (
        "04PG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04PG.ift_tk"),
    ),
    (
        "04Q0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04Q0.ift_tk"),
    ),
    (
        "04QG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04QG.ift_tk"),
    ),
    (
        "04R0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04R0.ift_tk"),
    ),
    (
        "04RG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04RG.ift_tk"),
    ),
    (
        "04S0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04S0.ift_tk"),
    ),
    (
        "04SG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04SG.ift_tk"),
    ),
    (
        "04T0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04T0.ift_tk"),
    ),
    (
        "04TG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04TG.ift_tk"),
    ),
    (
        "04U0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04U0.ift_tk"),
    ),
    (
        "04UG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04UG.ift_tk"),
    ),
    (
        "04V0.ift_tk",
        include_bytes!("../test_data/ift/roboto/04V0.ift_tk"),
    ),
    (
        "04VG.ift_tk",
        include_bytes!("../test_data/ift/roboto/04VG.ift_tk"),
    ),
    (
        "0500.ift_tk",
        include_bytes!("../test_data/ift/roboto/0500.ift_tk"),
    ),
    (
        "050G.ift_tk",
        include_bytes!("../test_data/ift/roboto/050G.ift_tk"),
    ),
    (
        "0510.ift_tk",
        include_bytes!("../test_data/ift/roboto/0510.ift_tk"),
    ),
    (
        "051G.ift_tk",
        include_bytes!("../test_data/ift/roboto/051G.ift_tk"),
    ),
    (
        "0520.ift_tk",
        include_bytes!("../test_data/ift/roboto/0520.ift_tk"),
    ),
    (
        "052G.ift_tk",
        include_bytes!("../test_data/ift/roboto/052G.ift_tk"),
    ),
    (
        "0530.ift_tk",
        include_bytes!("../test_data/ift/roboto/0530.ift_tk"),
    ),
    (
        "053G.ift_tk",
        include_bytes!("../test_data/ift/roboto/053G.ift_tk"),
    ),
    (
        "0540.ift_tk",
        include_bytes!("../test_data/ift/roboto/0540.ift_tk"),
    ),
    (
        "054G.ift_tk",
        include_bytes!("../test_data/ift/roboto/054G.ift_tk"),
    ),
    (
        "0550.ift_tk",
        include_bytes!("../test_data/ift/roboto/0550.ift_tk"),
    ),
    (
        "055G.ift_tk",
        include_bytes!("../test_data/ift/roboto/055G.ift_tk"),
    ),
    (
        "0560.ift_tk",
        include_bytes!("../test_data/ift/roboto/0560.ift_tk"),
    ),
    (
        "056G.ift_tk",
        include_bytes!("../test_data/ift/roboto/056G.ift_tk"),
    ),
    (
        "0570.ift_tk",
        include_bytes!("../test_data/ift/roboto/0570.ift_tk"),
    ),
    (
        "057G.ift_tk",
        include_bytes!("../test_data/ift/roboto/057G.ift_tk"),
    ),
    (
        "0580.ift_tk",
        include_bytes!("../test_data/ift/roboto/0580.ift_tk"),
    ),
    (
        "058G.ift_tk",
        include_bytes!("../test_data/ift/roboto/058G.ift_tk"),
    ),
    (
        "0590.ift_tk",
        include_bytes!("../test_data/ift/roboto/0590.ift_tk"),
    ),
    (
        "059G.ift_tk",
        include_bytes!("../test_data/ift/roboto/059G.ift_tk"),
    ),
    (
        "05A0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05A0.ift_tk"),
    ),
    (
        "05AG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05AG.ift_tk"),
    ),
    (
        "05B0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05B0.ift_tk"),
    ),
    (
        "05BG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05BG.ift_tk"),
    ),
    (
        "05C0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05C0.ift_tk"),
    ),
    (
        "05CG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05CG.ift_tk"),
    ),
    (
        "05D0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05D0.ift_tk"),
    ),
    (
        "05DG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05DG.ift_tk"),
    ),
    (
        "05E0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05E0.ift_tk"),
    ),
    (
        "05EG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05EG.ift_tk"),
    ),
    (
        "05F0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05F0.ift_tk"),
    ),
    (
        "05FG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05FG.ift_tk"),
    ),
    (
        "05G0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05G0.ift_tk"),
    ),
    (
        "05GG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05GG.ift_tk"),
    ),
    (
        "05H0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05H0.ift_tk"),
    ),
    (
        "05HG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05HG.ift_tk"),
    ),
    (
        "05I0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05I0.ift_tk"),
    ),
    (
        "05IG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05IG.ift_tk"),
    ),
    (
        "05J0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05J0.ift_tk"),
    ),
    (
        "05JG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05JG.ift_tk"),
    ),
    (
        "05K0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05K0.ift_tk"),
    ),
    (
        "05KG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05KG.ift_tk"),
    ),
    (
        "05L0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05L0.ift_tk"),
    ),
    (
        "05LG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05LG.ift_tk"),
    ),
    (
        "05M0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05M0.ift_tk"),
    ),
    (
        "05MG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05MG.ift_tk"),
    ),
    (
        "05N0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05N0.ift_tk"),
    ),
    (
        "05NG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05NG.ift_tk"),
    ),
    (
        "05O0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05O0.ift_tk"),
    ),
    (
        "05OG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05OG.ift_tk"),
    ),
    (
        "05P0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05P0.ift_tk"),
    ),
    (
        "05PG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05PG.ift_tk"),
    ),
    (
        "05Q0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05Q0.ift_tk"),
    ),
    (
        "05QG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05QG.ift_tk"),
    ),
    (
        "05R0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05R0.ift_tk"),
    ),
    (
        "05RG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05RG.ift_tk"),
    ),
    (
        "05S0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05S0.ift_tk"),
    ),
    (
        "05SG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05SG.ift_tk"),
    ),
    (
        "05T0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05T0.ift_tk"),
    ),
    (
        "05TG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05TG.ift_tk"),
    ),
    (
        "05U0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05U0.ift_tk"),
    ),
    (
        "05UG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05UG.ift_tk"),
    ),
    (
        "05V0.ift_tk",
        include_bytes!("../test_data/ift/roboto/05V0.ift_tk"),
    ),
    (
        "05VG.ift_tk",
        include_bytes!("../test_data/ift/roboto/05VG.ift_tk"),
    ),
    (
        "0600.ift_tk",
        include_bytes!("../test_data/ift/roboto/0600.ift_tk"),
    ),
    (
        "060G.ift_tk",
        include_bytes!("../test_data/ift/roboto/060G.ift_tk"),
    ),
    (
        "0610.ift_tk",
        include_bytes!("../test_data/ift/roboto/0610.ift_tk"),
    ),
    (
        "061G.ift_tk",
        include_bytes!("../test_data/ift/roboto/061G.ift_tk"),
    ),
    (
        "0620.ift_tk",
        include_bytes!("../test_data/ift/roboto/0620.ift_tk"),
    ),
    (
        "062G.ift_tk",
        include_bytes!("../test_data/ift/roboto/062G.ift_tk"),
    ),
    (
        "0630.ift_tk",
        include_bytes!("../test_data/ift/roboto/0630.ift_tk"),
    ),
    (
        "063G.ift_tk",
        include_bytes!("../test_data/ift/roboto/063G.ift_tk"),
    ),
    (
        "0640.ift_tk",
        include_bytes!("../test_data/ift/roboto/0640.ift_tk"),
    ),
    (
        "064G.ift_tk",
        include_bytes!("../test_data/ift/roboto/064G.ift_tk"),
    ),
    (
        "0650.ift_tk",
        include_bytes!("../test_data/ift/roboto/0650.ift_tk"),
    ),
    (
        "065G.ift_tk",
        include_bytes!("../test_data/ift/roboto/065G.ift_tk"),
    ),
    (
        "0660.ift_tk",
        include_bytes!("../test_data/ift/roboto/0660.ift_tk"),
    ),
    (
        "066G.ift_tk",
        include_bytes!("../test_data/ift/roboto/066G.ift_tk"),
    ),
    (
        "0670.ift_tk",
        include_bytes!("../test_data/ift/roboto/0670.ift_tk"),
    ),
    (
        "067G.ift_tk",
        include_bytes!("../test_data/ift/roboto/067G.ift_tk"),
    ),
    (
        "0680.ift_tk",
        include_bytes!("../test_data/ift/roboto/0680.ift_tk"),
    ),
    (
        "068G.ift_tk",
        include_bytes!("../test_data/ift/roboto/068G.ift_tk"),
    ),
    (
        "0690.ift_tk",
        include_bytes!("../test_data/ift/roboto/0690.ift_tk"),
    ),
    (
        "069G.ift_tk",
        include_bytes!("../test_data/ift/roboto/069G.ift_tk"),
    ),
    (
        "06A0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06A0.ift_tk"),
    ),
    (
        "06AG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06AG.ift_tk"),
    ),
    (
        "06B0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06B0.ift_tk"),
    ),
    (
        "06BG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06BG.ift_tk"),
    ),
    (
        "06C0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06C0.ift_tk"),
    ),
    (
        "06CG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06CG.ift_tk"),
    ),
    (
        "06D0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06D0.ift_tk"),
    ),
    (
        "06DG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06DG.ift_tk"),
    ),
    (
        "06E0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06E0.ift_tk"),
    ),
    (
        "06EG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06EG.ift_tk"),
    ),
    (
        "06F0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06F0.ift_tk"),
    ),
    (
        "06FG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06FG.ift_tk"),
    ),
    (
        "06G0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06G0.ift_tk"),
    ),
    (
        "06GG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06GG.ift_tk"),
    ),
    (
        "06H0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06H0.ift_tk"),
    ),
    (
        "06HG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06HG.ift_tk"),
    ),
    (
        "06I0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06I0.ift_tk"),
    ),
    (
        "06IG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06IG.ift_tk"),
    ),
    (
        "06J0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06J0.ift_tk"),
    ),
    (
        "06JG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06JG.ift_tk"),
    ),
    (
        "06K0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06K0.ift_tk"),
    ),
    (
        "06KG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06KG.ift_tk"),
    ),
    (
        "06L0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06L0.ift_tk"),
    ),
    (
        "06LG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06LG.ift_tk"),
    ),
    (
        "06M0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06M0.ift_tk"),
    ),
    (
        "06MG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06MG.ift_tk"),
    ),
    (
        "06N0.ift_tk",
        include_bytes!("../test_data/ift/roboto/06N0.ift_tk"),
    ),
    (
        "06NG.ift_tk",
        include_bytes!("../test_data/ift/roboto/06NG.ift_tk"),
    ),
    (
        "1_00.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_00.ift_gk"),
    ),
    (
        "1_04.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_04.ift_gk"),
    ),
    (
        "1_08.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_08.ift_gk"),
    ),
    (
        "1_0C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_0C.ift_gk"),
    ),
    (
        "1_0G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_0G.ift_gk"),
    ),
    (
        "1_0K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_0K.ift_gk"),
    ),
    (
        "1_0O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_0O.ift_gk"),
    ),
    (
        "1_0S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_0S.ift_gk"),
    ),
    (
        "1_10.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_10.ift_gk"),
    ),
    (
        "1_14.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_14.ift_gk"),
    ),
    (
        "1_18.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_18.ift_gk"),
    ),
    (
        "1_1C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_1C.ift_gk"),
    ),
    (
        "1_1G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_1G.ift_gk"),
    ),
    (
        "1_1K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_1K.ift_gk"),
    ),
    (
        "1_1O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_1O.ift_gk"),
    ),
    (
        "1_1S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_1S.ift_gk"),
    ),
    (
        "1_20.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_20.ift_gk"),
    ),
    (
        "1_24.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_24.ift_gk"),
    ),
    (
        "1_28.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_28.ift_gk"),
    ),
    (
        "1_2C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_2C.ift_gk"),
    ),
    (
        "1_2G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_2G.ift_gk"),
    ),
    (
        "1_2K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_2K.ift_gk"),
    ),
    (
        "1_2O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_2O.ift_gk"),
    ),
    (
        "1_2S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_2S.ift_gk"),
    ),
    (
        "1_30.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_30.ift_gk"),
    ),
    (
        "1_34.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_34.ift_gk"),
    ),
    (
        "1_38.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_38.ift_gk"),
    ),
    (
        "1_3C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_3C.ift_gk"),
    ),
    (
        "1_3G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_3G.ift_gk"),
    ),
    (
        "1_3K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_3K.ift_gk"),
    ),
    (
        "1_3O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_3O.ift_gk"),
    ),
    (
        "1_3S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_3S.ift_gk"),
    ),
    (
        "1_40.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_40.ift_gk"),
    ),
    (
        "1_44.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_44.ift_gk"),
    ),
    (
        "1_48.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_48.ift_gk"),
    ),
    (
        "1_4C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_4C.ift_gk"),
    ),
    (
        "1_4G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_4G.ift_gk"),
    ),
    (
        "1_4K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_4K.ift_gk"),
    ),
    (
        "1_4O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_4O.ift_gk"),
    ),
    (
        "1_4S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_4S.ift_gk"),
    ),
    (
        "1_50.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_50.ift_gk"),
    ),
    (
        "1_54.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_54.ift_gk"),
    ),
    (
        "1_58.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_58.ift_gk"),
    ),
    (
        "1_5C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_5C.ift_gk"),
    ),
    (
        "1_5G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_5G.ift_gk"),
    ),
    (
        "1_5K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_5K.ift_gk"),
    ),
    (
        "1_5O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_5O.ift_gk"),
    ),
    (
        "1_5S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_5S.ift_gk"),
    ),
    (
        "1_60.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_60.ift_gk"),
    ),
    (
        "1_64.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_64.ift_gk"),
    ),
    (
        "1_68.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_68.ift_gk"),
    ),
    (
        "1_6C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_6C.ift_gk"),
    ),
    (
        "1_6G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_6G.ift_gk"),
    ),
    (
        "1_6K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_6K.ift_gk"),
    ),
    (
        "1_6O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_6O.ift_gk"),
    ),
    (
        "1_6S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_6S.ift_gk"),
    ),
    (
        "1_70.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_70.ift_gk"),
    ),
    (
        "1_74.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_74.ift_gk"),
    ),
    (
        "1_78.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_78.ift_gk"),
    ),
    (
        "1_7C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_7C.ift_gk"),
    ),
    (
        "1_7G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_7G.ift_gk"),
    ),
    (
        "1_7K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_7K.ift_gk"),
    ),
    (
        "1_7O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_7O.ift_gk"),
    ),
    (
        "1_7S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_7S.ift_gk"),
    ),
    (
        "1_80.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_80.ift_gk"),
    ),
    (
        "1_84.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_84.ift_gk"),
    ),
    (
        "1_88.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_88.ift_gk"),
    ),
    (
        "1_8C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_8C.ift_gk"),
    ),
    (
        "1_8G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_8G.ift_gk"),
    ),
    (
        "1_8K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_8K.ift_gk"),
    ),
    (
        "1_8O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_8O.ift_gk"),
    ),
    (
        "1_8S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_8S.ift_gk"),
    ),
    (
        "1_90.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_90.ift_gk"),
    ),
    (
        "1_94.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_94.ift_gk"),
    ),
    (
        "1_98.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_98.ift_gk"),
    ),
    (
        "1_9C.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_9C.ift_gk"),
    ),
    (
        "1_9G.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_9G.ift_gk"),
    ),
    (
        "1_9K.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_9K.ift_gk"),
    ),
    (
        "1_9O.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_9O.ift_gk"),
    ),
    (
        "1_9S.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_9S.ift_gk"),
    ),
    (
        "1_A0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_A0.ift_gk"),
    ),
    (
        "1_A4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_A4.ift_gk"),
    ),
    (
        "1_A8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_A8.ift_gk"),
    ),
    (
        "1_AC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_AC.ift_gk"),
    ),
    (
        "1_AG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_AG.ift_gk"),
    ),
    (
        "1_AK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_AK.ift_gk"),
    ),
    (
        "1_AO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_AO.ift_gk"),
    ),
    (
        "1_AS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_AS.ift_gk"),
    ),
    (
        "1_B0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_B0.ift_gk"),
    ),
    (
        "1_B4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_B4.ift_gk"),
    ),
    (
        "1_B8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_B8.ift_gk"),
    ),
    (
        "1_BC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_BC.ift_gk"),
    ),
    (
        "1_BG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_BG.ift_gk"),
    ),
    (
        "1_BK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_BK.ift_gk"),
    ),
    (
        "1_BO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_BO.ift_gk"),
    ),
    (
        "1_BS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_BS.ift_gk"),
    ),
    (
        "1_C0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_C0.ift_gk"),
    ),
    (
        "1_C4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_C4.ift_gk"),
    ),
    (
        "1_C8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_C8.ift_gk"),
    ),
    (
        "1_CC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_CC.ift_gk"),
    ),
    (
        "1_CG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_CG.ift_gk"),
    ),
    (
        "1_CK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_CK.ift_gk"),
    ),
    (
        "1_CO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_CO.ift_gk"),
    ),
    (
        "1_CS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_CS.ift_gk"),
    ),
    (
        "1_D0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_D0.ift_gk"),
    ),
    (
        "1_D4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_D4.ift_gk"),
    ),
    (
        "1_D8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_D8.ift_gk"),
    ),
    (
        "1_DC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_DC.ift_gk"),
    ),
    (
        "1_DG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_DG.ift_gk"),
    ),
    (
        "1_DK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_DK.ift_gk"),
    ),
    (
        "1_DO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_DO.ift_gk"),
    ),
    (
        "1_DS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_DS.ift_gk"),
    ),
    (
        "1_E0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_E0.ift_gk"),
    ),
    (
        "1_E4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_E4.ift_gk"),
    ),
    (
        "1_E8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_E8.ift_gk"),
    ),
    (
        "1_EC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_EC.ift_gk"),
    ),
    (
        "1_EG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_EG.ift_gk"),
    ),
    (
        "1_EK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_EK.ift_gk"),
    ),
    (
        "1_EO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_EO.ift_gk"),
    ),
    (
        "1_ES.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_ES.ift_gk"),
    ),
    (
        "1_F0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_F0.ift_gk"),
    ),
    (
        "1_F4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_F4.ift_gk"),
    ),
    (
        "1_F8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_F8.ift_gk"),
    ),
    (
        "1_FC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_FC.ift_gk"),
    ),
    (
        "1_FG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_FG.ift_gk"),
    ),
    (
        "1_FK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_FK.ift_gk"),
    ),
    (
        "1_FO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_FO.ift_gk"),
    ),
    (
        "1_FS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_FS.ift_gk"),
    ),
    (
        "1_G0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_G0.ift_gk"),
    ),
    (
        "1_G4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_G4.ift_gk"),
    ),
    (
        "1_G8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_G8.ift_gk"),
    ),
    (
        "1_GC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_GC.ift_gk"),
    ),
    (
        "1_GG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_GG.ift_gk"),
    ),
    (
        "1_GK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_GK.ift_gk"),
    ),
    (
        "1_GO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_GO.ift_gk"),
    ),
    (
        "1_GS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_GS.ift_gk"),
    ),
    (
        "1_H0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_H0.ift_gk"),
    ),
    (
        "1_H4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_H4.ift_gk"),
    ),
    (
        "1_H8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_H8.ift_gk"),
    ),
    (
        "1_HC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_HC.ift_gk"),
    ),
    (
        "1_HG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_HG.ift_gk"),
    ),
    (
        "1_HK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_HK.ift_gk"),
    ),
    (
        "1_HO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_HO.ift_gk"),
    ),
    (
        "1_HS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_HS.ift_gk"),
    ),
    (
        "1_I0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_I0.ift_gk"),
    ),
    (
        "1_I4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_I4.ift_gk"),
    ),
    (
        "1_I8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_I8.ift_gk"),
    ),
    (
        "1_IC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_IC.ift_gk"),
    ),
    (
        "1_IG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_IG.ift_gk"),
    ),
    (
        "1_IK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_IK.ift_gk"),
    ),
    (
        "1_IO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_IO.ift_gk"),
    ),
    (
        "1_IS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_IS.ift_gk"),
    ),
    (
        "1_J0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_J0.ift_gk"),
    ),
    (
        "1_J4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_J4.ift_gk"),
    ),
    (
        "1_J8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_J8.ift_gk"),
    ),
    (
        "1_JC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_JC.ift_gk"),
    ),
    (
        "1_JG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_JG.ift_gk"),
    ),
    (
        "1_JK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_JK.ift_gk"),
    ),
    (
        "1_JO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_JO.ift_gk"),
    ),
    (
        "1_JS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_JS.ift_gk"),
    ),
    (
        "1_K0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_K0.ift_gk"),
    ),
    (
        "1_K4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_K4.ift_gk"),
    ),
    (
        "1_K8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_K8.ift_gk"),
    ),
    (
        "1_KC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_KC.ift_gk"),
    ),
    (
        "1_KG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_KG.ift_gk"),
    ),
    (
        "1_KK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_KK.ift_gk"),
    ),
    (
        "1_KO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_KO.ift_gk"),
    ),
    (
        "1_KS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_KS.ift_gk"),
    ),
    (
        "1_L0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_L0.ift_gk"),
    ),
    (
        "1_L4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_L4.ift_gk"),
    ),
    (
        "1_L8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_L8.ift_gk"),
    ),
    (
        "1_LC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_LC.ift_gk"),
    ),
    (
        "1_LG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_LG.ift_gk"),
    ),
    (
        "1_LK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_LK.ift_gk"),
    ),
    (
        "1_LO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_LO.ift_gk"),
    ),
    (
        "1_LS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_LS.ift_gk"),
    ),
    (
        "1_M0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_M0.ift_gk"),
    ),
    (
        "1_M4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_M4.ift_gk"),
    ),
    (
        "1_M8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_M8.ift_gk"),
    ),
    (
        "1_MC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_MC.ift_gk"),
    ),
    (
        "1_MG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_MG.ift_gk"),
    ),
    (
        "1_MK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_MK.ift_gk"),
    ),
    (
        "1_MO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_MO.ift_gk"),
    ),
    (
        "1_MS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_MS.ift_gk"),
    ),
    (
        "1_N0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_N0.ift_gk"),
    ),
    (
        "1_N4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_N4.ift_gk"),
    ),
    (
        "1_N8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_N8.ift_gk"),
    ),
    (
        "1_NC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_NC.ift_gk"),
    ),
    (
        "1_NG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_NG.ift_gk"),
    ),
    (
        "1_NK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_NK.ift_gk"),
    ),
    (
        "1_NO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_NO.ift_gk"),
    ),
    (
        "1_NS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_NS.ift_gk"),
    ),
    (
        "1_O0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_O0.ift_gk"),
    ),
    (
        "1_O4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_O4.ift_gk"),
    ),
    (
        "1_O8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_O8.ift_gk"),
    ),
    (
        "1_OC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_OC.ift_gk"),
    ),
    (
        "1_OG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_OG.ift_gk"),
    ),
    (
        "1_OK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_OK.ift_gk"),
    ),
    (
        "1_OO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_OO.ift_gk"),
    ),
    (
        "1_OS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_OS.ift_gk"),
    ),
    (
        "1_P0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_P0.ift_gk"),
    ),
    (
        "1_P4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_P4.ift_gk"),
    ),
    (
        "1_P8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_P8.ift_gk"),
    ),
    (
        "1_PC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_PC.ift_gk"),
    ),
    (
        "1_PG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_PG.ift_gk"),
    ),
    (
        "1_PK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_PK.ift_gk"),
    ),
    (
        "1_PO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_PO.ift_gk"),
    ),
    (
        "1_PS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_PS.ift_gk"),
    ),
    (
        "1_Q0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_Q0.ift_gk"),
    ),
    (
        "1_Q4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_Q4.ift_gk"),
    ),
    (
        "1_Q8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_Q8.ift_gk"),
    ),
    (
        "1_QC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_QC.ift_gk"),
    ),
    (
        "1_QG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_QG.ift_gk"),
    ),
    (
        "1_QK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_QK.ift_gk"),
    ),
    (
        "1_QO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_QO.ift_gk"),
    ),
    (
        "1_QS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_QS.ift_gk"),
    ),
    (
        "1_R0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_R0.ift_gk"),
    ),
    (
        "1_R4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_R4.ift_gk"),
    ),
    (
        "1_R8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_R8.ift_gk"),
    ),
    (
        "1_RC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_RC.ift_gk"),
    ),
    (
        "1_RG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_RG.ift_gk"),
    ),
    (
        "1_RK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_RK.ift_gk"),
    ),
    (
        "1_RO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_RO.ift_gk"),
    ),
    (
        "1_RS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_RS.ift_gk"),
    ),
    (
        "1_S0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_S0.ift_gk"),
    ),
    (
        "1_S4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_S4.ift_gk"),
    ),
    (
        "1_S8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_S8.ift_gk"),
    ),
    (
        "1_SC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_SC.ift_gk"),
    ),
    (
        "1_SG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_SG.ift_gk"),
    ),
    (
        "1_SK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_SK.ift_gk"),
    ),
    (
        "1_SO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_SO.ift_gk"),
    ),
    (
        "1_SS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_SS.ift_gk"),
    ),
    (
        "1_T0.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_T0.ift_gk"),
    ),
    (
        "1_T4.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_T4.ift_gk"),
    ),
    (
        "1_T8.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_T8.ift_gk"),
    ),
    (
        "1_TC.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_TC.ift_gk"),
    ),
    (
        "1_TG.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_TG.ift_gk"),
    ),
    (
        "1_TK.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_TK.ift_gk"),
    ),
    (
        "1_TO.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_TO.ift_gk"),
    ),
    (
        "1_TS.ift_gk",
        include_bytes!("../test_data/ift/roboto/1_TS.ift_gk"),
    ),
    (
        "2_00.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_00.ift_gk"),
    ),
    (
        "2_04.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_04.ift_gk"),
    ),
    (
        "2_08.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_08.ift_gk"),
    ),
    (
        "2_0C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_0C.ift_gk"),
    ),
    (
        "2_0G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_0G.ift_gk"),
    ),
    (
        "2_0K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_0K.ift_gk"),
    ),
    (
        "2_0O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_0O.ift_gk"),
    ),
    (
        "2_0S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_0S.ift_gk"),
    ),
    (
        "2_10.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_10.ift_gk"),
    ),
    (
        "2_14.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_14.ift_gk"),
    ),
    (
        "2_18.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_18.ift_gk"),
    ),
    (
        "2_1C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_1C.ift_gk"),
    ),
    (
        "2_1G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_1G.ift_gk"),
    ),
    (
        "2_1K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_1K.ift_gk"),
    ),
    (
        "2_1O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_1O.ift_gk"),
    ),
    (
        "2_1S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_1S.ift_gk"),
    ),
    (
        "2_20.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_20.ift_gk"),
    ),
    (
        "2_24.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_24.ift_gk"),
    ),
    (
        "2_28.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_28.ift_gk"),
    ),
    (
        "2_2C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_2C.ift_gk"),
    ),
    (
        "2_2G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_2G.ift_gk"),
    ),
    (
        "2_2K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_2K.ift_gk"),
    ),
    (
        "2_2O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_2O.ift_gk"),
    ),
    (
        "2_2S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_2S.ift_gk"),
    ),
    (
        "2_30.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_30.ift_gk"),
    ),
    (
        "2_34.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_34.ift_gk"),
    ),
    (
        "2_38.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_38.ift_gk"),
    ),
    (
        "2_3C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_3C.ift_gk"),
    ),
    (
        "2_3G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_3G.ift_gk"),
    ),
    (
        "2_3K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_3K.ift_gk"),
    ),
    (
        "2_3O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_3O.ift_gk"),
    ),
    (
        "2_3S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_3S.ift_gk"),
    ),
    (
        "2_40.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_40.ift_gk"),
    ),
    (
        "2_44.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_44.ift_gk"),
    ),
    (
        "2_48.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_48.ift_gk"),
    ),
    (
        "2_4C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_4C.ift_gk"),
    ),
    (
        "2_4G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_4G.ift_gk"),
    ),
    (
        "2_4K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_4K.ift_gk"),
    ),
    (
        "2_4O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_4O.ift_gk"),
    ),
    (
        "2_4S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_4S.ift_gk"),
    ),
    (
        "2_50.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_50.ift_gk"),
    ),
    (
        "2_54.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_54.ift_gk"),
    ),
    (
        "2_58.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_58.ift_gk"),
    ),
    (
        "2_5C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_5C.ift_gk"),
    ),
    (
        "2_5G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_5G.ift_gk"),
    ),
    (
        "2_5K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_5K.ift_gk"),
    ),
    (
        "2_5O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_5O.ift_gk"),
    ),
    (
        "2_5S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_5S.ift_gk"),
    ),
    (
        "2_60.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_60.ift_gk"),
    ),
    (
        "2_64.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_64.ift_gk"),
    ),
    (
        "2_68.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_68.ift_gk"),
    ),
    (
        "2_6C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_6C.ift_gk"),
    ),
    (
        "2_6G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_6G.ift_gk"),
    ),
    (
        "2_6K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_6K.ift_gk"),
    ),
    (
        "2_6O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_6O.ift_gk"),
    ),
    (
        "2_6S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_6S.ift_gk"),
    ),
    (
        "2_70.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_70.ift_gk"),
    ),
    (
        "2_74.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_74.ift_gk"),
    ),
    (
        "2_78.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_78.ift_gk"),
    ),
    (
        "2_7C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_7C.ift_gk"),
    ),
    (
        "2_7G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_7G.ift_gk"),
    ),
    (
        "2_7K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_7K.ift_gk"),
    ),
    (
        "2_7O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_7O.ift_gk"),
    ),
    (
        "2_7S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_7S.ift_gk"),
    ),
    (
        "2_80.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_80.ift_gk"),
    ),
    (
        "2_84.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_84.ift_gk"),
    ),
    (
        "2_88.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_88.ift_gk"),
    ),
    (
        "2_8C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_8C.ift_gk"),
    ),
    (
        "2_8G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_8G.ift_gk"),
    ),
    (
        "2_8K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_8K.ift_gk"),
    ),
    (
        "2_8O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_8O.ift_gk"),
    ),
    (
        "2_8S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_8S.ift_gk"),
    ),
    (
        "2_90.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_90.ift_gk"),
    ),
    (
        "2_94.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_94.ift_gk"),
    ),
    (
        "2_98.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_98.ift_gk"),
    ),
    (
        "2_9C.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_9C.ift_gk"),
    ),
    (
        "2_9G.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_9G.ift_gk"),
    ),
    (
        "2_9K.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_9K.ift_gk"),
    ),
    (
        "2_9O.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_9O.ift_gk"),
    ),
    (
        "2_9S.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_9S.ift_gk"),
    ),
    (
        "2_A0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_A0.ift_gk"),
    ),
    (
        "2_A4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_A4.ift_gk"),
    ),
    (
        "2_A8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_A8.ift_gk"),
    ),
    (
        "2_AC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_AC.ift_gk"),
    ),
    (
        "2_AG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_AG.ift_gk"),
    ),
    (
        "2_AK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_AK.ift_gk"),
    ),
    (
        "2_AO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_AO.ift_gk"),
    ),
    (
        "2_AS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_AS.ift_gk"),
    ),
    (
        "2_B0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_B0.ift_gk"),
    ),
    (
        "2_B4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_B4.ift_gk"),
    ),
    (
        "2_B8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_B8.ift_gk"),
    ),
    (
        "2_BC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_BC.ift_gk"),
    ),
    (
        "2_BG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_BG.ift_gk"),
    ),
    (
        "2_BK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_BK.ift_gk"),
    ),
    (
        "2_BO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_BO.ift_gk"),
    ),
    (
        "2_BS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_BS.ift_gk"),
    ),
    (
        "2_C0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_C0.ift_gk"),
    ),
    (
        "2_C4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_C4.ift_gk"),
    ),
    (
        "2_C8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_C8.ift_gk"),
    ),
    (
        "2_CC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_CC.ift_gk"),
    ),
    (
        "2_CG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_CG.ift_gk"),
    ),
    (
        "2_CK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_CK.ift_gk"),
    ),
    (
        "2_CO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_CO.ift_gk"),
    ),
    (
        "2_CS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_CS.ift_gk"),
    ),
    (
        "2_D0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_D0.ift_gk"),
    ),
    (
        "2_D4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_D4.ift_gk"),
    ),
    (
        "2_D8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_D8.ift_gk"),
    ),
    (
        "2_DC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_DC.ift_gk"),
    ),
    (
        "2_DG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_DG.ift_gk"),
    ),
    (
        "2_DK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_DK.ift_gk"),
    ),
    (
        "2_DO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_DO.ift_gk"),
    ),
    (
        "2_DS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_DS.ift_gk"),
    ),
    (
        "2_E0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_E0.ift_gk"),
    ),
    (
        "2_E4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_E4.ift_gk"),
    ),
    (
        "2_E8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_E8.ift_gk"),
    ),
    (
        "2_EC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_EC.ift_gk"),
    ),
    (
        "2_EG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_EG.ift_gk"),
    ),
    (
        "2_EK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_EK.ift_gk"),
    ),
    (
        "2_EO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_EO.ift_gk"),
    ),
    (
        "2_ES.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_ES.ift_gk"),
    ),
    (
        "2_F0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_F0.ift_gk"),
    ),
    (
        "2_F4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_F4.ift_gk"),
    ),
    (
        "2_F8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_F8.ift_gk"),
    ),
    (
        "2_FC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_FC.ift_gk"),
    ),
    (
        "2_FG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_FG.ift_gk"),
    ),
    (
        "2_FK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_FK.ift_gk"),
    ),
    (
        "2_FO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_FO.ift_gk"),
    ),
    (
        "2_FS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_FS.ift_gk"),
    ),
    (
        "2_G0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_G0.ift_gk"),
    ),
    (
        "2_G4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_G4.ift_gk"),
    ),
    (
        "2_G8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_G8.ift_gk"),
    ),
    (
        "2_GC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_GC.ift_gk"),
    ),
    (
        "2_GG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_GG.ift_gk"),
    ),
    (
        "2_GK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_GK.ift_gk"),
    ),
    (
        "2_GO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_GO.ift_gk"),
    ),
    (
        "2_GS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_GS.ift_gk"),
    ),
    (
        "2_H0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_H0.ift_gk"),
    ),
    (
        "2_H4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_H4.ift_gk"),
    ),
    (
        "2_H8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_H8.ift_gk"),
    ),
    (
        "2_HC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_HC.ift_gk"),
    ),
    (
        "2_HG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_HG.ift_gk"),
    ),
    (
        "2_HK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_HK.ift_gk"),
    ),
    (
        "2_HO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_HO.ift_gk"),
    ),
    (
        "2_HS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_HS.ift_gk"),
    ),
    (
        "2_I0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_I0.ift_gk"),
    ),
    (
        "2_I4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_I4.ift_gk"),
    ),
    (
        "2_I8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_I8.ift_gk"),
    ),
    (
        "2_IC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_IC.ift_gk"),
    ),
    (
        "2_IG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_IG.ift_gk"),
    ),
    (
        "2_IK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_IK.ift_gk"),
    ),
    (
        "2_IO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_IO.ift_gk"),
    ),
    (
        "2_IS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_IS.ift_gk"),
    ),
    (
        "2_J0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_J0.ift_gk"),
    ),
    (
        "2_J4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_J4.ift_gk"),
    ),
    (
        "2_J8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_J8.ift_gk"),
    ),
    (
        "2_JC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_JC.ift_gk"),
    ),
    (
        "2_JG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_JG.ift_gk"),
    ),
    (
        "2_JK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_JK.ift_gk"),
    ),
    (
        "2_JO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_JO.ift_gk"),
    ),
    (
        "2_JS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_JS.ift_gk"),
    ),
    (
        "2_K0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_K0.ift_gk"),
    ),
    (
        "2_K4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_K4.ift_gk"),
    ),
    (
        "2_K8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_K8.ift_gk"),
    ),
    (
        "2_KC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_KC.ift_gk"),
    ),
    (
        "2_KG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_KG.ift_gk"),
    ),
    (
        "2_KK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_KK.ift_gk"),
    ),
    (
        "2_KO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_KO.ift_gk"),
    ),
    (
        "2_KS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_KS.ift_gk"),
    ),
    (
        "2_L0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_L0.ift_gk"),
    ),
    (
        "2_L4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_L4.ift_gk"),
    ),
    (
        "2_L8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_L8.ift_gk"),
    ),
    (
        "2_LC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_LC.ift_gk"),
    ),
    (
        "2_LG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_LG.ift_gk"),
    ),
    (
        "2_LK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_LK.ift_gk"),
    ),
    (
        "2_LO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_LO.ift_gk"),
    ),
    (
        "2_LS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_LS.ift_gk"),
    ),
    (
        "2_M0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_M0.ift_gk"),
    ),
    (
        "2_M4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_M4.ift_gk"),
    ),
    (
        "2_M8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_M8.ift_gk"),
    ),
    (
        "2_MC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_MC.ift_gk"),
    ),
    (
        "2_MG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_MG.ift_gk"),
    ),
    (
        "2_MK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_MK.ift_gk"),
    ),
    (
        "2_MO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_MO.ift_gk"),
    ),
    (
        "2_MS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_MS.ift_gk"),
    ),
    (
        "2_N0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_N0.ift_gk"),
    ),
    (
        "2_N4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_N4.ift_gk"),
    ),
    (
        "2_N8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_N8.ift_gk"),
    ),
    (
        "2_NC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_NC.ift_gk"),
    ),
    (
        "2_NG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_NG.ift_gk"),
    ),
    (
        "2_NK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_NK.ift_gk"),
    ),
    (
        "2_NO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_NO.ift_gk"),
    ),
    (
        "2_NS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_NS.ift_gk"),
    ),
    (
        "2_O0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_O0.ift_gk"),
    ),
    (
        "2_O4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_O4.ift_gk"),
    ),
    (
        "2_O8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_O8.ift_gk"),
    ),
    (
        "2_OC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_OC.ift_gk"),
    ),
    (
        "2_OG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_OG.ift_gk"),
    ),
    (
        "2_OK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_OK.ift_gk"),
    ),
    (
        "2_OO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_OO.ift_gk"),
    ),
    (
        "2_OS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_OS.ift_gk"),
    ),
    (
        "2_P0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_P0.ift_gk"),
    ),
    (
        "2_P4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_P4.ift_gk"),
    ),
    (
        "2_P8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_P8.ift_gk"),
    ),
    (
        "2_PC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_PC.ift_gk"),
    ),
    (
        "2_PG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_PG.ift_gk"),
    ),
    (
        "2_PK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_PK.ift_gk"),
    ),
    (
        "2_PO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_PO.ift_gk"),
    ),
    (
        "2_PS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_PS.ift_gk"),
    ),
    (
        "2_Q0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_Q0.ift_gk"),
    ),
    (
        "2_Q4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_Q4.ift_gk"),
    ),
    (
        "2_Q8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_Q8.ift_gk"),
    ),
    (
        "2_QC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_QC.ift_gk"),
    ),
    (
        "2_QG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_QG.ift_gk"),
    ),
    (
        "2_QK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_QK.ift_gk"),
    ),
    (
        "2_QO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_QO.ift_gk"),
    ),
    (
        "2_QS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_QS.ift_gk"),
    ),
    (
        "2_R0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_R0.ift_gk"),
    ),
    (
        "2_R4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_R4.ift_gk"),
    ),
    (
        "2_R8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_R8.ift_gk"),
    ),
    (
        "2_RC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_RC.ift_gk"),
    ),
    (
        "2_RG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_RG.ift_gk"),
    ),
    (
        "2_RK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_RK.ift_gk"),
    ),
    (
        "2_RO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_RO.ift_gk"),
    ),
    (
        "2_RS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_RS.ift_gk"),
    ),
    (
        "2_S0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_S0.ift_gk"),
    ),
    (
        "2_S4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_S4.ift_gk"),
    ),
    (
        "2_S8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_S8.ift_gk"),
    ),
    (
        "2_SC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_SC.ift_gk"),
    ),
    (
        "2_SG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_SG.ift_gk"),
    ),
    (
        "2_SK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_SK.ift_gk"),
    ),
    (
        "2_SO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_SO.ift_gk"),
    ),
    (
        "2_SS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_SS.ift_gk"),
    ),
    (
        "2_T0.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_T0.ift_gk"),
    ),
    (
        "2_T4.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_T4.ift_gk"),
    ),
    (
        "2_T8.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_T8.ift_gk"),
    ),
    (
        "2_TC.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_TC.ift_gk"),
    ),
    (
        "2_TG.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_TG.ift_gk"),
    ),
    (
        "2_TK.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_TK.ift_gk"),
    ),
    (
        "2_TO.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_TO.ift_gk"),
    ),
    (
        "2_TS.ift_gk",
        include_bytes!("../test_data/ift/roboto/2_TS.ift_gk"),
    ),
    (
        "U0.ift_tk",
        include_bytes!("../test_data/ift/roboto/U0.ift_tk"),
    ),
    (
        "U4.ift_tk",
        include_bytes!("../test_data/ift/roboto/U4.ift_tk"),
    ),
    (
        "U8.ift_tk",
        include_bytes!("../test_data/ift/roboto/U8.ift_tk"),
    ),
    (
        "UC.ift_tk",
        include_bytes!("../test_data/ift/roboto/UC.ift_tk"),
    ),
    (
        "UG.ift_tk",
        include_bytes!("../test_data/ift/roboto/UG.ift_tk"),
    ),
    (
        "UK.ift_tk",
        include_bytes!("../test_data/ift/roboto/UK.ift_tk"),
    ),
    (
        "UO.ift_tk",
        include_bytes!("../test_data/ift/roboto/UO.ift_tk"),
    ),
    (
        "US.ift_tk",
        include_bytes!("../test_data/ift/roboto/US.ift_tk"),
    ),
    (
        "V0.ift_tk",
        include_bytes!("../test_data/ift/roboto/V0.ift_tk"),
    ),
    (
        "V4.ift_tk",
        include_bytes!("../test_data/ift/roboto/V4.ift_tk"),
    ),
    (
        "V8.ift_tk",
        include_bytes!("../test_data/ift/roboto/V8.ift_tk"),
    ),
    (
        "VC.ift_tk",
        include_bytes!("../test_data/ift/roboto/VC.ift_tk"),
    ),
    (
        "VG.ift_tk",
        include_bytes!("../test_data/ift/roboto/VG.ift_tk"),
    ),
    (
        "VK.ift_tk",
        include_bytes!("../test_data/ift/roboto/VK.ift_tk"),
    ),
    (
        "VO.ift_tk",
        include_bytes!("../test_data/ift/roboto/VO.ift_tk"),
    ),
    (
        "VS.ift_tk",
        include_bytes!("../test_data/ift/roboto/VS.ift_tk"),
    ),
];
