//! 16-bit signed and unsigned font-units

/// 16-bit signed quantity in font design units.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fword(super::int16);

/// 16-bit unsigned quantity in font design units.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ufword(super::int16);

//TODO: we can add addition/etc as needed
