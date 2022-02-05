use crate::*;
use zerocopy::{AsBytes, FromBytes, LayoutVerified, Unaligned, BE, I16, I32, I64, U16, U32};

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

#[derive(Clone, FromBytes, AsBytes, Unaligned)]
#[repr(C)]
pub struct HeadZero {
    pub major_version: U16<BE>,
    pub minor_version: U16<BE>,
    pub font_revision: I32<BE>,
    pub checksum_adjustment: U32<BE>,
    pub magic_number: U32<BE>,
    pub flags: U16<BE>,
    pub units_per_em: U16<BE>,
    pub created: I64<BE>,
    pub modified: I64<BE>,
    pub x_min: I16<BE>,
    pub y_min: I16<BE>,
    pub x_max: I16<BE>,
    pub y_max: I16<BE>,
    pub mac_style: U16<BE>,
    pub lowest_rec_ppem: U16<BE>,
    pub font_direction_hint: I16<BE>,
    pub index_to_loc_format: I16<BE>,
    pub glyph_data_format: I16<BE>,
}

impl<'a> FontRead<'a> for &'a HeadZero {
    fn read(data: blob::Blob<'a>) -> Option<Self> {
        let layout = LayoutVerified::<_, HeadZero>::new_unaligned(data.as_bytes())?;
        Some(layout.into_ref())
    }
}
