//! The [VARC (Variable Composites/Components)](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) table

pub use super::layout::{Condition, CoverageTable};
pub use super::postscript::Index2;

include!("../../generated/generated_varc.rs");
