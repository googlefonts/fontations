//! Reading OpentType tables

mod font_data;
mod layout;
mod table_ref;

pub mod parse_prelude {
    pub use crate::font_data::{Cursor, FontData, ReadError};
    pub use crate::table_ref::{FontRead, Format, ResolveOffset, TableInfo, TableRef};

    pub use font_types::{
        BigEndian, F2Dot14, FWord, Fixed, FixedSized, LongDateTime, MajorMinor, Offset, Offset16,
        Offset24, Offset32, OffsetHost, OffsetLen, ReadScalar, Scalar, Tag, UfWord, Uint24,
        Version16Dot16,
    };
    pub use std::ops::Range;
}
