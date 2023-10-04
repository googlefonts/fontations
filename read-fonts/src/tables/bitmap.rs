//! Common bitmap (EBLC/EBDT/CBLC/CBDT) types.

include!("../../generated/generated_bitmap.rs");

impl BitmapSize {
    pub fn subtable<'a>(
        &self,
        offset_data: FontData<'a>,
        index: u32,
    ) -> Result<BitmapSizeSubtable<'a>, ReadError> {
        let base_offset = self.index_subtable_array_offset() as usize;
        const SUBTABLE_HEADER_SIZE: usize = 8;
        let header_offset = base_offset + index as usize * SUBTABLE_HEADER_SIZE;
        let header_data = offset_data
            .slice(header_offset..)
            .ok_or(ReadError::OutOfBounds)?;
        let header = IndexSubtableArray::read(header_data)?;
        let subtable_offset = base_offset + header.additional_offset_to_index_subtable() as usize;
        let subtable_data = offset_data
            .slice(subtable_offset..)
            .ok_or(ReadError::OutOfBounds)?;
        let subtable = IndexSubtable::read(subtable_data)?;
        Ok(BitmapSizeSubtable {
            first_glyph_index: header.first_glyph_index(),
            last_glyph_index: header.last_glyph_index(),
            kind: subtable,
        })
    }

    pub fn location(
        &self,
        offset_data: FontData,
        glyph_id: GlyphId,
    ) -> Result<BitmapLocation, ReadError> {
        if !(self.start_glyph_index()..=self.end_glyph_index()).contains(&glyph_id) {
            return Err(ReadError::OutOfBounds);
        }
        let mut location = BitmapLocation::default();
        for ix in 0..self.number_of_index_subtables() {
            let subtable = self.subtable(offset_data, ix)?;
            if !(subtable.first_glyph_index..=subtable.last_glyph_index).contains(&glyph_id) {
                continue;
            }
            // glyph index relative to the first glyph in the subtable
            let glyph_ix =
                glyph_id.to_u16() as usize - subtable.first_glyph_index.to_u16() as usize;
            match &subtable.kind {
                IndexSubtable::Format1(st) => {
                    location.format = st.image_format();
                    location.data_offset = st.image_data_offset() as usize
                        + st.sbit_offsets()
                            .get(glyph_ix)
                            .ok_or(ReadError::OutOfBounds)?
                            .get() as usize;
                }
                IndexSubtable::Format2(st) => {
                    location.format = st.image_format();
                    let data_size = st.image_size() as usize;
                    location.data_size = Some(data_size);
                    location.data_offset = st.image_data_offset() as usize + glyph_ix * data_size;
                    location.metrics = Some(st.big_metrics()[0].clone());
                }
                IndexSubtable::Format3(st) => {
                    location.format = st.image_format();
                    location.data_offset = st.image_data_offset() as usize
                        + st.sbit_offsets()
                            .get(glyph_ix)
                            .ok_or(ReadError::OutOfBounds)?
                            .get() as usize;
                }
                IndexSubtable::Format4(st) => {
                    location.format = st.image_format();
                    let array = st.glyph_array();
                    let array_ix = match array.binary_search_by(|x| x.glyph_id().cmp(&glyph_id)) {
                        Ok(ix) => ix,
                        _ => {
                            return Err(ReadError::InvalidCollectionIndex(glyph_id.to_u16() as u32))
                        }
                    };
                    let offset1 = array[array_ix].sbit_offset() as usize;
                    let offset2 = array
                        .get(array_ix + 1)
                        .ok_or(ReadError::OutOfBounds)?
                        .sbit_offset() as usize;
                    location.data_offset = offset1;
                    location.data_size = Some(offset2 - offset1);
                }
                IndexSubtable::Format5(st) => {
                    location.format = st.image_format();
                    let array = st.glyph_array();
                    if array.binary_search_by(|x| x.get().cmp(&glyph_id)).is_err() {
                        return Err(ReadError::InvalidCollectionIndex(glyph_id.to_u16() as u32));
                    }
                    let data_size = st.image_size() as usize;
                    location.data_size = Some(data_size);
                    location.data_offset = st.image_data_offset() as usize + glyph_ix * data_size;
                    location.metrics = Some(st.big_metrics()[0].clone());
                }
            }
            return Ok(location);
        }
        Err(ReadError::OutOfBounds)
    }
}

pub struct BitmapSizeSubtable<'a> {
    pub first_glyph_index: GlyphId,
    pub last_glyph_index: GlyphId,
    pub kind: IndexSubtable<'a>,
}

#[derive(Clone, Default)]
pub struct BitmapLocation {
    /// Format of EBDT/CBDT image data.
    pub format: u16,
    /// Offset in EBDT/CBDT table.
    pub data_offset: usize,
    pub data_size: Option<usize>,
    pub metrics: Option<BigGlyphMetrics>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum BitmapFormat {
    Mask,
    PackedMask,
    Color,
    Png,
}

#[derive(Clone)]
pub enum BitmapMetrics {
    Small(SmallGlyphMetrics),
    Big(BigGlyphMetrics),
}

#[derive(Clone)]
pub struct BitmapData<'a> {
    pub format: BitmapFormat,
    pub metrics: Option<BitmapMetrics>,
    pub data: &'a [u8],
}

pub(crate) fn bitmap_data<'a>(
    offset_data: FontData<'a>,
    location: &BitmapLocation,
    is_color: bool,
) -> Result<BitmapData<'a>, ReadError> {
    let mut image_data = offset_data
        .slice(location.data_offset..)
        .ok_or(ReadError::OutOfBounds)?
        .cursor();
    match location.format {
        17 if is_color => {
            let metrics = image_data.read_array::<SmallGlyphMetrics>(1)?[0].clone();
            let data_len = image_data.read::<u32>()? as usize;
            let data = image_data.read_array::<u8>(data_len)?;
            return Ok(BitmapData {
                format: BitmapFormat::Png,
                metrics: Some(BitmapMetrics::Small(metrics)),
                data,
            });
        }
        18 if is_color => {
            let metrics = image_data.read_array::<BigGlyphMetrics>(1)?[0].clone();
            let data_len = image_data.read::<u32>()? as usize;
            let data = image_data.read_array::<u8>(data_len)?;
            return Ok(BitmapData {
                format: BitmapFormat::Png,
                metrics: Some(BitmapMetrics::Big(metrics)),
                data,
            });
        }
        19 if is_color => {
            let data_len = image_data.read::<u32>()? as usize;
            let data = image_data.read_array::<u8>(data_len)?;
            return Ok(BitmapData {
                format: BitmapFormat::Png,
                metrics: None,
                data,
            });
        }
        _ => {}
    }
    Err(ReadError::MalformedData("bad image format"))
}

#[cfg(feature = "traversal")]
impl SbitLineMetrics {
    pub(crate) fn traversal_type<'a>(&self, data: FontData<'a>) -> FieldType<'a> {
        FieldType::Record(self.clone().traverse(data))
    }
}
