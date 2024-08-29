use read_fonts::types::Tag;
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
      [
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0, 0, 0, 0, 0,
        0, 0u8
      ],

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      4u8,                 // patch encoding = glyph keyed

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

      // applied entry bitmap (51 bytes) - 300 is applied
      [
        0, 0, 0, 0, 0, 0, 0, 0,           // [0, 64)
        0, 0, 0, 0, 0, 0, 0, 0,           // [64, 128)
        0, 0, 0, 0, 0, 0, 0, 0,           // [128, 192)
        0, 0, 0, 0, 0, 0, 0, 0,           // [192, 256)
        0, 0, 0, 0, 0, 0b00001000, 0, 0,  // [256, 320)
        0, 0, 0, 0, 0, 0, 0, 0,           // [320, 384)
        0, 0, 0u8                         // [384, 400)
      ],

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      4u8,                    // patch encoding = glyph keyed

      /* ### Glyph Map ### */
      {2u16: "glyph_map"}, // first mapped glyph

      // entryIndex[2..6]
      [
        80,  // gid 2
        81,  // gid 3
        300, // gid 4
        299, // gid 5
        80u16   // gid 6
      ],

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
      2u8,                // format

      0u32,               // reserved

      [1, 2, 3, 4u32],    // compat id

      4u8,                // default patch encoding
      ((Uint24::new(3))), // entry count

      {0u32: "entries_offset"},
      0u32,               // entry string data offset

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010000u8: "entries"},              // format = CODEPOINT_BIT_1
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 2
      0b01100000u8,                           // format = IGNORED | CODEPOINT_BIT_2
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 3
      0b00100000u8,                           // format = CODEPOINT_BIT_2
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [5..22]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

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
