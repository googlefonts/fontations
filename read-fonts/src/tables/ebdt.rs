//! The [EBDT (Embedded Bitmap Data)](https://docs.microsoft.com/en-us/typography/opentype/spec/ebdt) table

use super::bitmap::{BitmapData, BitmapLocation};

include!("../../generated/generated_ebdt.rs");

impl<'a> Ebdt<'a> {
    pub fn data(&self, location: &BitmapLocation) -> Result<BitmapData<'a>, ReadError> {
        super::bitmap::bitmap_data(self.offset_data(), location, false)
    }
}
