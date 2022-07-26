//! The [loca (Index to Location)][loca] table
//!
//! [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca

use font_types::{BigEndian, Offset32, Tag};
use zerocopy::LayoutVerified;

/// 'loca'
pub const TAG: Tag = Tag::new(b"loca");

/// The [loca] table.
///
/// [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca
pub enum Loca<'a> {
    Short(&'a [BigEndian<u16>]),
    Long(&'a [BigEndian<Offset32>]),
}

impl<'a> Loca<'a> {
    /// Create a new loca table.
    ///
    /// num_glyphs should be read from [maxp], and is_short from [head].
    ///
    /// [maxp]: super::Maxp
    /// [head]: super::head::Head
    pub fn read(data: &'a [u8], num_glyphs: u16, is_long: bool) -> Option<Self> {
        let num_glyphs = num_glyphs as usize;
        if is_long {
            let (data, _) = LayoutVerified::new_slice_unaligned_from_prefix(data, num_glyphs + 1)?;
            Some(Loca::Long(data.into_slice()))
        } else {
            let (data, _) = LayoutVerified::new_slice_unaligned_from_prefix(data, num_glyphs + 1)?;
            Some(Loca::Short(data.into_slice()))
        }
    }

    /// Attempt to return the offset for a given glyph id.
    pub fn get(&self, idx: usize) -> Option<Offset32> {
        match self {
            Loca::Short(data) => {
                let value = data.get(idx)?.get();
                Some(Offset32::new(value as u32 * 2))
            }

            Loca::Long(data) => data.get(idx).copied().map(BigEndian::get),
        }
    }

    /// Iterate all offsets
    pub fn iter(&self) -> impl Iterator<Item = Offset32> + '_ {
        let mut idx = 0;
        std::iter::from_fn(move || {
            let result = self.get(idx);
            idx += 1;
            result
        })
    }
}
