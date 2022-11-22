//! The [COLR](https://docs.microsoft.com/en-us/typography/opentype/spec/colr) table

use super::variations::{DeltaSetIndexMap, ItemVariationStore};
use types::Tag;

/// 'COLR'
pub const TAG: Tag = Tag::new(b"COLR");

include!("../../generated/generated_colr.rs");
