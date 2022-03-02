use crate::{Fixed, Int16, LongDateTime, Uint16, Uint32};

toy_table_macro::tables! {
    Head {
     major_version: Uint16,
     minor_version: Uint16,
     font_revision: Fixed,
     checksum_adjustment: Uint32,
     magic_number: Uint32,
     flags: Uint16,
     units_per_em: Uint16,
     created: LongDateTime,
     modified: LongDateTime,
     x_min: Int16,
     y_min: Int16,
     x_max: Int16,
     y_max: Int16,
     mac_style: Uint16,
     lowest_rec_ppem: Uint16,
     font_direction_hint: Int16,
     index_to_loc_format: Int16,
     glyph_data_format: Int16,
    }
}
