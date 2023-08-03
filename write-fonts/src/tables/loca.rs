//! The [loca (Index to Location)][loca] table
//!
//! [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca

use read_fonts::TopLevelTable;
use types::Tag;

use crate::{
    validate::{Validate, ValidationCtx},
    FontWrite,
};

/// The [loca] table.
///
/// [loca]: https://docs.microsoft.com/en-us/typography/opentype/spec/loca
pub struct Loca {
    // we just store u32, and then convert to u16 if needed in the `FontWrite` impl
    pub(crate) offsets: Vec<u32>,
    loca_format: LocaFormat,
}

/// Whether or not the 'loca' table uses short or long offsets.
///
/// This flag is stored in the 'head' table's [indexToLocFormat][locformat] field.
/// See the ['loca' spec][spec] for more information.
///
/// [locformat]: super::head::Head::index_to_loc_format
/// [spec]: https://learn.microsoft.com/en-us/typography/opentype/spec/loca
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum LocaFormat {
    Short = 0,
    Long = 1,
}

impl TopLevelTable for Loca {
    const TAG: Tag = Tag::new(b"loca");
}

impl Loca {
    /// Create a new loca table from 32-bit offsets.
    ///
    /// The loca format will be calculated based on the raw values.
    ///
    /// You generally do not construct this directly; it is constructed alongside
    /// the corresponding 'glyf' table using the
    /// [GlyfLocaBuilder](super::glyf::GlyfLocaBuilder).
    pub fn new(offsets: Vec<u32>) -> Self {
        let loca_format = LocaFormat::new(&offsets);

        Loca {
            offsets,
            loca_format,
        }
    }

    pub fn format(&self) -> LocaFormat {
        self.loca_format
    }
}

impl LocaFormat {
    fn new(loca: &[u32]) -> LocaFormat {
        // https://github.com/fonttools/fonttools/blob/1c283756a5e39d69459eea80ed12792adc4922dd/Lib/fontTools/ttLib/tables/_l_o_c_a.py#L37
        const MAX_SHORT_LOCA_VALUE: u32 = 0x20000;
        if loca.last().copied().unwrap_or_default() < MAX_SHORT_LOCA_VALUE
            && loca.iter().all(|offset| offset % 2 == 0)
        {
            LocaFormat::Short
        } else {
            LocaFormat::Long
        }
    }
}

impl FontWrite for Loca {
    fn write_into(&self, writer: &mut crate::TableWriter) {
        match self.loca_format {
            LocaFormat::Long => self.offsets.write_into(writer),
            LocaFormat::Short => self
                .offsets
                .iter()
                .for_each(|off| ((off >> 1) as u16).write_into(writer)),
        }
    }
}

impl Validate for Loca {
    fn validate_impl(&self, _ctx: &mut ValidationCtx) {}
}
