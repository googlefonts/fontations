use crate::*;
//use font_types_macro::

#[derive(Debug, Clone, FontThing)]
pub struct Head {
    pub major_version: uint16,
    pub minor_version: uint16,
    pub font_revision: Fixed,
    pub checksum_adjustment: uint32,
    pub magic_number: uint32,
    pub flags: uint16,
    pub units_per_em: uint16,
    pub created: LongDateTime,
    pub modified: LongDateTime,
    pub x_min: int16,
    pub y_min: int16,
    pub x_max: int16,
    pub y_max: int16,
    pub mac_style: uint16,
    pub lowest_rec_ppem: uint16,
    pub font_direction_hint: int16,
    pub index_to_loc_format: int16,
    pub glyph_data_format: int16,
}
