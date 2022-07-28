//! Reading OpentType tables

mod array;
mod font_data;
mod layout;
mod read;
mod table_ref;

pub mod parse_prelude {
    pub use crate::array::ComputedArray;
    pub use crate::font_data::{Cursor, FontData};
    pub use crate::read::{ComputeSize, FontRead, FontReadWithArgs, Format, ReadArgs, ReadError};
    pub use crate::table_ref::{ResolveOffset, TableInfo, TableInfoWithArgs, TableRef};

    pub use font_types::{
        BigEndian, F2Dot14, FWord, Fixed, FixedSized, LongDateTime, MajorMinor, Offset, Offset16,
        Offset24, Offset32, OffsetHost, OffsetLen, ReadScalar, Scalar, Tag, UfWord, Uint24,
        Version16Dot16,
    };
    pub use std::ops::Range;
}
