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
mod glyph_id;
mod longdatetime;
mod offset;
mod raw;
mod tag;
mod uint24;
mod version;

pub use fixed::{F2Dot14, Fixed};
pub use fword::{FWord, UfWord};
pub use glyph_id::GlyphId;
pub use longdatetime::LongDateTime;
pub use offset::{Offset16, Offset24, Offset32};
pub use raw::{BigEndian, FixedSized, ReadScalar, Scalar};
pub use tag::Tag;
pub use uint24::Uint24;
pub use version::{MajorMinor, Version16Dot16};
