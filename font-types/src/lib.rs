//! Common [scalar data types][data types] used in font files
//!
//! [data types]: https://docs.microsoft.com/en-us/typography/opentype/spec/otff#data-types

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(any(feature = "std", test))]
#[macro_use]
extern crate std;

#[cfg(all(not(feature = "std"), not(test)))]
#[macro_use]
extern crate core as std;

mod fixed;
mod fword;
mod integers;
mod longdatetime;
mod offset;
mod raw;
mod tag;
mod uint24;
mod version16dot16;

pub use fixed::{F2Dot14, Fixed, RawF2Dot14, RawFixed};
pub use fword::{FWord, RawFWord, RawUfWord, UfWord};
pub use longdatetime::{LongDateTime, RawLongDateTime};
pub use offset::{Offset16, Offset24, Offset32, RawOffset16, RawOffset24, RawOffset32};
pub use raw::RawType;
pub use tag::Tag;
pub use uint24::{RawU24, Uint24};
pub use version16dot16::{RawVersion16Dot16, Version16Dot16};
