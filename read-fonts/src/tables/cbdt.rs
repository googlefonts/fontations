//! The [CBDT (Color Bitmap Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/cbdt) table

use super::bitmap::{BitmapData, BitmapLocation};

include!("../../generated/generated_cbdt.rs");

impl<'a> Cbdt<'a> {
    pub fn data(&self, location: &BitmapLocation) -> Result<BitmapData<'a>, ReadError> {
        super::bitmap::bitmap_data(self.offset_data(), location, true)
    }
}
