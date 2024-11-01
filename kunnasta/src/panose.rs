//! Panose Classification settings
//!
//! See the [PANOSE Classification Metrics Guide](https://monotype.github.io/panose/pan1.html)
use num_enum::IntoPrimitive;

#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum FamilyType {
    Any,
    NoFit,
    LatinText,
    LatinHandWritten,
    LatinDecorative,
    LatinSymbol,
}

#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum Proportion {
    Any,
    NoFit,
    OldStyle,
    Modern,
    EvenWidth,
    Expanded,
    Condensed,
    VeryExpanded,
    VeryCondensed,
    Monospaced,
}

#[derive(IntoPrimitive)]
#[repr(u8)]
pub enum Spacing {
    Any,
    NoFit,
    Proportional,
    Monospaced,
}
