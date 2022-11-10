//! The [HVAR (Horizontal Metrics Variation)](https://docs.microsoft.com/en-us/typography/opentype/spec/hvar) table

use crate::variation::{DeltaSetIndexMap, ItemVariationStore};
use font_types::Tag;

/// 'HVAR'
pub const TAG: Tag = Tag::new(b"HVAR");

include!("../../generated/generated_hvar.rs");
