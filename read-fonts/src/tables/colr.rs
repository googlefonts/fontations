//! The [COLR](https://docs.microsoft.com/en-us/typography/opentype/spec/colr) table

use crate::variations::{DeltaSetIndexMap, ItemVariationStore};
use font_types::Tag;

/// 'COLR'
pub const TAG: Tag = Tag::new(b"COLR");

include!("../../generated/generated_colr.rs");
