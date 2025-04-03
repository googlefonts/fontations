//! support closure for GPOS

use super::Gpos;
use crate::{collections::IntSet, ReadError, Tag};

impl Gpos<'_> {
    /// Return a set of all feature indices underneath the specified scripts, languages and features
    ///
    /// if no script is provided, all scripts will be queried
    /// if no language is provided, all languages will be queried
    /// if no feature is provided, all features will be queried
    pub fn collect_features(
        &self,
        scripts: Option<&IntSet<Tag>>,
        languages: Option<&IntSet<Tag>>,
        features: Option<&IntSet<Tag>>,
    ) -> Result<IntSet<u16>, ReadError> {
        let feature_list = self.feature_list()?;
        let script_list = self.script_list()?;
        let head_ptr = self.offset_data().as_bytes().as_ptr() as usize;
        script_list.collect_features(head_ptr, &feature_list, scripts, languages, features)
    }
}
