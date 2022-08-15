//! The [loca (Index to Location)][loca] table
//!
//! [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca

use font_types::{BigEndian, Offset32, Tag};

use crate::{parse_prelude::ReadArgs, FontReadWithArgs};

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

impl ReadArgs for Loca<'_> {
    type Args = bool;
}

impl<'a> FontReadWithArgs<'a> for Loca<'a> {
    fn read_with_args(
        data: crate::FontData<'a>,
        args: &Self::Args,
    ) -> Result<Self, crate::ReadError> {
        let is_long = *args;
        if is_long {
            data.read_array(0..data.len()).map(Loca::Long)
        } else {
            data.read_array(0..data.len()).map(Loca::Short)
        }
    }
}
