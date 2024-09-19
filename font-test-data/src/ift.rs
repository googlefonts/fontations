//! Test data for the IFT table
//!
//! Used for incremental font transfer. Specification:
//! <https://w3c.github.io/IFT/Overview.html>

use read_fonts::types::Tag;
use read_fonts::{be_buffer, be_buffer_add, test_helpers::BeBuffer, types::Int24, types::Uint24};

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
      (Uint24::new(4)),   // entry count
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
      [0b00001101, 0b00000011, 0b00110001u8],  // codepoints = [5..22]

      // Entry id = 4
      0b00110000u8,                           // format = CODEPOINT_BIT_1 | CODEPOINT_BIT_2
      (Uint24::new(80_000)),                  // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [80_005..80_022]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn features_and_design_space_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8, // format

      0u32, // reserved

      [1, 2, 3, 4u32], // compat id

      4u8, // default patch encoding
      (Uint24::new(3)), // entry count
      {0u32: "entries_offset"},
      0u32, // entry id string data offset

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      /* ### Entries Array ### */
      // Entry id = 1
      {0b00010001u8: "entries"},          // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      2u8,                                // feature count = 2
      (Tag::new(b"liga")),                // feature[0] = liga
      (Tag::new(b"smcp")),                // feature[1] = smcp

      1u16,                               // design space count
      (Tag::new(b"wdth")),                // tag = wdth
      {0x8000u32: "wdth start"},          // start = 0.5
      0x10000u32,                         // end = 1.0

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entries Array
      // Entry id = 2
      0b00010001u8,                       // format = CODEPOINT_BIT_1 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count
      (Tag::new(b"rlig")),                // feature[0]

      0u16,                               // design space count

      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 3
      0b000100001u8,                      // format = CODEPOINT_BIT_2 | FEATURES_AND_DESIGN_SPACE

      1u8,                                // feature count = 1
      (Tag::new(b"smcp")),                // feature[0] = smcp

      3u16,                               // design space count
      (Tag::new(b"wght")),                // tag = wght
      0x00C8_0000u32,                     // start = 200
      0x02BC_0000u32,                     // end = 700

      (Tag::new(b"wdth")),                // tag = wdth
      0x0u32,                             // start = 0
      0x8000,                             // end = 0

      (Tag::new(b"wdth")),                // tag = wdth
      0x0002_0000,                        // start = 2.0
      0x0002_8000,                        // end = 2.5

      5u16,                               // bias = 5
      0b00001101, 0b00000011, 0b00110001  // codepoints = [5..22]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

pub fn copy_indices_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      4u8,                      // default patch encoding = glyph keyed
      (Uint24::new(9)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      0u32,                     // entry id string data offset

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      // Entries Array

      // Entry id = 1
      {0b00100000u8: "entries"},              // format
      5u16,                                   // bias = 5
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 2
      0b00100000u8,                           // format = CODEPOINT_BIT_2
      50u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [50..67]

      // Entry id = 3
      0b00000001u8,                           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count = 1
      (Tag::new(b"rlig")),                    // feature[0] = rlig

      1u16,                                   // design space count = 1
      (Tag::new(b"wght")),                    // tag = wght
      0x00C8_0000u32,                         // start = 200
      0x02BC_0000u32,                         // end = 700

      // Entry id = 4
      0b00000001u8,                           // format = FEATURES_AND_DESIGN_SPACE

      1u8,                                    // feature count
      (Tag::new(b"liga")),                    // feature[0] = liga

      1u16,                                   // design space count
      (Tag::new(b"wght")),                    // tag = wght
      0x0032_0000,                            // start = 50
      0x0064_0000,                            // end = 100

      // Entry id = 5
      0b00000010u8,                           // format = COPY_INDICES
      1u8,                                    // copy count
      (Uint24::new(0)),                       // copy

      // Entry id = 6
      0b00000010u8,                           // format = COPY_INDICES
      1u8,                                    // copy count
      (Uint24::new(2)),                       // copy

      // Entry id = 7
      0b00000010u8,                           // format = COPY_INDICES
      4u8,                                    // copy count
      (Uint24::new(3)),                       // copy[0]
      (Uint24::new(2)),                       // copy[1]
      (Uint24::new(1)),                       // copy[2]
      (Uint24::new(0)),                       // copy[3]

      // Entry id = 8
      0b00000010u8,                           // format = COPY_INDICES
      2u8,                                    // copy count
      (Uint24::new(4)),                       // copy[0]
      (Uint24::new(5)),                       // copy[1]

      // Entry id = 9
      0b00100010u8,                           // format = CODEPOINT_BIT_2 | COPY_INDICES
      1u8,                                    // copy count
      (Uint24::new(0)),                       // copy[0]
      100u16,                                 // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [100..117]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn custom_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                               // format

      0u32,                              // reserved

      [1, 2, 3, 4u32],                   // compat id

      4u8,                               // default patch encoding = glyph keyed
      {(Uint24::new(4)): "entry_count"}, // entry count
      {0u32: "entries_offset"},          // entries offset
      0u32,                              // entry id string data offset

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      // Entries Array
      // Entry id = 0
      {0b00010100u8: "entries"},              // format = CODEPOINT_BIT_1 | ID_DELTA
      (Int24::new(-1)),                       // id delta
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [0..17]

      // Entry id = 6
      0b00100100u8,                           // format = CODEPOINT_BIT_2 | ID_DELTA
      {(Int24::new(5)): "id delta"},            // id delta
      5u16,                                   // bias
      [0b00001101, 0b00000011, 0b00110001u8], // codepoints = [5..22]

      // Entry id = 14
      0b01000100u8,                           // format = ID_DELTA | IGNORED
      {(Int24::new(7)): "id delta - ignored entry"}, // id delta

      // Entry id = 15
      0b00101000u8,                           // format = CODEPOINT_BIT_2 | PATCH_ENCODING
      {4u8: "entry[4] encoding"},             // patch encoding = Glyph Keyed
      10u16,                                  // bias
      [0b00001101, 0b00000011, 0b00110001u8]  // codepoints = [10..27]
    };

    let offset = buffer.offset_for("entries") as u32;
    buffer.write_at("entries_offset", offset);

    buffer
}

// Format specification: https://w3c.github.io/IFT/Overview.html#patch-map-format-2
pub fn string_ids_format2() -> BeBuffer {
    let mut buffer = be_buffer! {
      2u8,                      // format

      0u32,                     // reserved

      [1, 2, 3, 4u32],          // compat id

      4u8,                      // default patch encoding = glyph keyed
      (Uint24::new(6)),         // entry count
      {0u32: "entries_offset"}, // entries offset
      {0u32: "string_data_offset"},                     // entry id string data offset

      8u16, // uriTemplateLength
      [b'A', b'B', b'C', b'D', b'E', b'F', 0xc9, 0xa4],  // uriTemplate[8]

      /* ### Entry Data ### */

      // Entry id = ""
      {0b00000000u8: "entries"},              // format = {}

      // Entry id = abc
      0b00000100u8,                           // format = ID_DELTA
      3u16,                                   // id length

      // Entry id = defg
      0b00000100u8,                           // format = ID_DELTA
      4u16,                                   // id length

      // Entry id = defg
      0b00000000u8,                           // format = {}

      // Entry id = hij
      0b00000100u8,                           // format = ID_DELTA
      {3u16: "entry[4] id length"},           // id length

      // Entry id = ""
      0b00000100u8,                           // format = ID_DELTA
      0u16,                                   // id length

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
