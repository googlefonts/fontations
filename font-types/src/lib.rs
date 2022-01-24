//! Common [scalar data types][data types] used in font files
//!
//! [data types]: https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types

mod fixed;
mod fword;
mod longdatetime;
mod offset;
mod tag;
mod uint24;
mod version16dot16;

/// 8-bit unsigned integer
#[allow(non_camel_case_types)]
pub type uint8 = u8;

/// 8-bit signed integer
#[allow(non_camel_case_types)]
pub type int8 = i8;

/// 16-bit unsigned integer
#[allow(non_camel_case_types)]
pub type uint16 = u16;

/// 16-bit signed integer
#[allow(non_camel_case_types)]
pub type int16 = i16;

/// 32-bit unsigned integer
#[allow(non_camel_case_types)]
pub type uint32 = u32;

/// 32-bit signed integer
#[allow(non_camel_case_types)]
pub type int32 = i32;

pub use fixed::{F2dot14, Fixed};
pub use fword::{Fword, Ufword};
pub use longdatetime::LongDateTime;
pub use offset::{Offset16, Offset24, Offset32};
pub use tag::Tag;
pub use uint24::Uint24;
pub use version16dot16::Version16Dot16;
