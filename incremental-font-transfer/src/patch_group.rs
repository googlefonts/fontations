use read_fonts::{tables::ift::CompatibilityId, FontRef, ReadError, TableProvider};
use std::collections::{btree_map::Entry, BTreeMap, HashSet};

use crate::{
    font_patch::{IncrementalFontPatchBase, PatchingError},
    patchmap::{intersecting_patches, IftTableTag, PatchEncoding, PatchUri, SubsetDefinition},
};

// TODO XXXXX can we remove uses of font.ift() and font.iftx()

/// A group of patches derived from a single IFT font which can be applied simulatenously
/// to that font.
pub struct PatchApplicationGroup<'a> {
    font: FontRef<'a>,
    patches: Option<CompatibleGroup>,
}

impl PatchApplicationGroup<'_> {
    /// Intersect the available and unapplied patches in ift_font against subset_definition and return a group
    /// of patches which would be applied next.
    pub fn select_next_patches<'a>(
        ift_font: FontRef<'a>,
        subset_definition: &SubsetDefinition,
    ) -> Result<PatchApplicationGroup<'a>, ReadError> {
        // TODO(garretrieger): what happens when there are no intersecting patches? add tests
        let candidates = intersecting_patches(&ift_font, subset_definition)?;
        let compat_group = Self::select_next_patches_from_candidates(
            candidates,
            ift_font.ift()?.compatibility_id(),
            ift_font.iftx()?.compatibility_id(),
        )?;

        Ok(PatchApplicationGroup {
            font: ift_font,
            patches: Some(compat_group),
        })
    }

    /// Returns the list of URIs in this group which do not yet have patch data supplied for them.
    pub fn pending_uris(&self) -> HashSet<&str> {
        let Some(patches) = &self.patches else {
            return Default::default();
        };

        // TODO(garretrieger): filter out uri's which have data associated with them.
        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => {
                if let Some(_) = info.data {
                    return HashSet::default();
                };
                HashSet::from([info.uri.as_str()])
            }
            CompatibleGroup::Mixed { ift, iftx } => {
                let mut uris: HashSet<&str> = Default::default();
                ift.collect_pending_uris(&mut uris);
                iftx.collect_pending_uris(&mut uris);
                uris
            }
        }
    }

    pub fn has_pending_uris(&self) -> bool {
        let Some(patches) = &self.patches else {
            return false;
        };
        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => info.data.is_none(),
            CompatibleGroup::Mixed { ift, iftx } => {
                ift.has_pending_uris() || iftx.has_pending_uris()
            }
        }
    }

    /// Supply patch data for a uri. Once all patches have been supplied this will trigger patch application and
    /// the optional return will contain the new font.
    pub fn add_patch_data(
        &mut self,
        uri: &str,
        data: Vec<u8>,
    ) -> Option<Result<Vec<u8>, PatchingError>> {
        let Some(patches) = &mut self.patches else {
            return None;
        };

        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => {
                if info.uri == uri {
                    info.data = Some(data);
                }
            }
            CompatibleGroup::Mixed { ift, iftx } => {
                if let Some(data) = ift.add_patch_data(uri, data) {
                    iftx.add_patch_data(uri, data);
                }
            }
        };

        if self.has_pending_uris() {
            return None;
        }

        let r = self.apply_patches();
        self.patches = None;
        Some(r)
    }

    pub(crate) fn apply_patches(&self) -> Result<Vec<u8>, PatchingError> {
        let Some(patches) = &self.patches else {
            return Err(PatchingError::InternalError);
        };
        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => {
                self.font.apply_table_keyed_patch(info)
            }
            CompatibleGroup::Mixed { ift, iftx } => {
                // Apply partial invalidation patches first
                let base = self.apply_partial_invalidation_patch(ift, None)?;
                let base = self.apply_partial_invalidation_patch(
                    iftx,
                    base.as_ref().map(|base| base.as_slice()),
                )?;

                // Then apply no invalidation patches
                let ift_patches = ift.no_invalidation_iter();
                let iftx_patches = iftx.no_invalidation_iter();

                let font = base
                    .as_ref()
                    .map(|base| FontRef::new(base.as_slice()))
                    .transpose()
                    .map_err(PatchingError::FontParsingFailed)?;
                let font = font.as_ref().unwrap_or(&self.font);

                font.apply_glyph_keyed_patches(ift_patches.chain(iftx_patches))
            }
        }
    }

    pub(crate) fn apply_partial_invalidation_patch(
        &self,
        scoped_group: &ScopedGroup,
        new_base: Option<&[u8]>,
    ) -> Result<Option<Vec<u8>>, PatchingError> {
        let font = new_base
            .as_ref()
            .map(|base| FontRef::new(base))
            .transpose()
            .map_err(PatchingError::FontParsingFailed)?;
        let font = font.as_ref().unwrap_or(&self.font);
        match scoped_group {
            ScopedGroup::PartialInvalidation(patch) => {
                Ok(Some(font.apply_table_keyed_patch(&patch.0)?))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn select_next_patches_from_candidates(
        candidates: Vec<PatchUri>,
        ift_compat_id: CompatibilityId,
        iftx_compat_id: CompatibilityId,
    ) -> Result<CompatibleGroup, ReadError> {
        // Some notes about this implementation:
        // - From candidates we need to form the largest possible group of patches which follow the selection criteria
        //   from: https://w3c.github.io/IFT/Overview.html#extend-font-subset and won't invalidate each other.
        //
        // - Validation constraints are encoded into the structure of CompatibleGroup so the task here is to fill up
        //   a compatible group appropriately.
        //
        // - When multiple valid choices exist the specification allows the implementation to take one of it's choosing.
        //   Here we use a heuristic that tries to select the patch which has the most value to the extension request.
        //
        // - During selection we need to ensure that there are no PatchInfo's with duplicate URIs. The spec doesn't
        //   require erroring on this case, and it's resolved by:
        //   - In the spec algo patches are selected and applied one at a time.
        //   - Further it specifically disallows re-applying the same URI later.
        //   - So therefore we de-dup by retaining the particular instance which has the highest selection
        //     priority.

        let mut full_invalidation: Vec<FullInvalidationPatch> = vec![];
        let mut partial_invalidation_ift: Vec<PartialInvalidationPatch> = vec![];
        let mut partial_invalidation_iftx: Vec<PartialInvalidationPatch> = vec![];
        // TODO(garretrieger): do we need sorted order, use HashMap instead?
        let mut no_invalidation_ift: BTreeMap<String, NoInvalidationPatch> = Default::default();
        let mut no_invalidation_iftx: BTreeMap<String, NoInvalidationPatch> = Default::default();

        // Step 1: sort the candidates into separate lists based on invalidation characteristics.
        for uri in candidates.into_iter() {
            // TODO(garretrieger): for efficiency can we delay uri template resolution until we have actually selected patches?
            // TODO(garretrieger): for btree construction don't recompute the resolved uri, cache inside the patch uri object?
            match uri.encoding() {
                PatchEncoding::TableKeyed {
                    fully_invalidating: true,
                } => full_invalidation.push(FullInvalidationPatch(uri.into())),
                PatchEncoding::TableKeyed {
                    fully_invalidating: false,
                } => {
                    if *uri.expected_compatibility_id() == ift_compat_id {
                        partial_invalidation_ift.push(PartialInvalidationPatch(uri.into()))
                    } else if *uri.expected_compatibility_id() == iftx_compat_id {
                        partial_invalidation_iftx.push(PartialInvalidationPatch(uri.into()))
                    }
                }
                PatchEncoding::GlyphKeyed => {
                    if *uri.expected_compatibility_id() == ift_compat_id {
                        no_invalidation_ift
                            .insert(uri.uri_string(), NoInvalidationPatch(uri.into()));
                    } else if *uri.expected_compatibility_id() == iftx_compat_id {
                        no_invalidation_iftx
                            .insert(uri.uri_string(), NoInvalidationPatch(uri.into()));
                    }
                }
            }
        }

        // Step 2 - now make patch selections in priority order: first full invalidation, second partial, lastly none.
        if let Some(patch) = full_invalidation.into_iter().next() {
            // TODO(garretrieger): use a heuristic to select the best patch
            return Ok(CompatibleGroup::Full(patch));
        }

        let mut ift_selected_uri: Option<String> = None;
        let ift_scope = partial_invalidation_ift
            .into_iter()
            // TODO(garretrieger): use a heuristic to select the best patch
            .next()
            .map(|patch| {
                ift_selected_uri = Some(patch.0.uri.clone());
                ScopedGroup::PartialInvalidation(patch)
            });

        let mut iftx_selected_uri: Option<String> = None;
        let iftx_scope = partial_invalidation_iftx
            .into_iter()
            .filter(|patch| {
                let Some(selected) = &ift_selected_uri else {
                    return true;
                };
                selected != &patch.0.uri
            })
            // TODO(garretrieger): use a heuristic to select the best patch
            .next()
            .map(|patch| {
                iftx_selected_uri = Some(patch.0.uri.clone());
                ScopedGroup::PartialInvalidation(patch)
            });

        // URI's which have been selected for use above should not show up in other selections.
        if let (Some(uri), None) = (&ift_selected_uri, &iftx_selected_uri) {
            no_invalidation_iftx.remove(uri);
        }
        if let (None, Some(uri)) = (ift_selected_uri, iftx_selected_uri) {
            no_invalidation_ift.remove(&uri);
        }

        match (ift_scope, iftx_scope) {
            (Some(scope1), Some(scope2)) => Ok(CompatibleGroup::Mixed {
                ift: scope1,
                iftx: scope2,
            }),
            (Some(scope1), None) => Ok(CompatibleGroup::Mixed {
                ift: scope1,
                iftx: ScopedGroup::NoInvalidation(no_invalidation_iftx),
            }),
            (None, Some(scope2)) => Ok(CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(no_invalidation_ift),
                iftx: scope2,
            }),
            (None, None) => {
                // The two groups can't contain any duplicate URIs so remove all URIs in ift from iftx.
                for uri in no_invalidation_ift.keys() {
                    no_invalidation_iftx.remove(uri);
                }
                Ok(CompatibleGroup::Mixed {
                    ift: ScopedGroup::NoInvalidation(no_invalidation_ift),
                    iftx: ScopedGroup::NoInvalidation(no_invalidation_iftx),
                })
            }
        }
    }
}

/// Tracks information related to a patch necessary to apply that patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct PatchInfo {
    uri: String,
    data: Option<Vec<u8>>,
    source_table: IftTableTag,
    // TODO: details for how to mark the patch applied in the mapping table (ie. bit index to flip).
    // TODO: Signals for heuristic patch selection:
}

impl PatchInfo {
    pub(crate) fn data(&self) -> Option<&[u8]> {
        self.data.as_ref().map(|v| v.as_slice())
    }

    pub(crate) fn tag(&self) -> &IftTableTag {
        &self.source_table
    }
}

impl From<PatchUri> for PatchInfo {
    fn from(value: PatchUri) -> Self {
        PatchInfo {
            uri: value.uri_string(),
            data: None,
            source_table: value.source_table(),
        }
    }
}

/// Type for a single non invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct NoInvalidationPatch(PatchInfo);

/// Type for a single partially invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct PartialInvalidationPatch(PatchInfo);

/// Type for a single fully invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct FullInvalidationPatch(PatchInfo);

/// Represents a group of patches which are valid (compatible) to be applied together to
/// an IFT font.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum CompatibleGroup {
    Full(FullInvalidationPatch),
    Mixed { ift: ScopedGroup, iftx: ScopedGroup },
}

/// A set of zero or more compatible patches that are derived from the same scope
/// ("IFT " vs "IFTX")
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ScopedGroup {
    PartialInvalidation(PartialInvalidationPatch),
    NoInvalidation(BTreeMap<String, NoInvalidationPatch>),
}

impl ScopedGroup {
    fn collect_pending_uris<'a>(&'a self, uris: &mut HashSet<&'a str>) {
        match self {
            ScopedGroup::PartialInvalidation(PartialInvalidationPatch(info)) => {
                if info.data.is_none() {
                    uris.insert(&info.uri);
                }
            }
            ScopedGroup::NoInvalidation(uri_map) => {
                for (key, value) in uri_map.iter() {
                    if value.0.data.is_none() {
                        uris.insert(&key);
                    }
                }
            }
        }
    }

    fn has_pending_uris(&self) -> bool {
        match self {
            ScopedGroup::PartialInvalidation(PartialInvalidationPatch(info)) => info.data.is_none(),
            ScopedGroup::NoInvalidation(uri_map) => {
                for (_, value) in uri_map.iter() {
                    if value.0.data.is_none() {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn add_patch_data(&mut self, uri: &str, data: Vec<u8>) -> Option<Vec<u8>> {
        match self {
            ScopedGroup::PartialInvalidation(PartialInvalidationPatch(info)) => {
                if info.uri == uri {
                    info.data = Some(data);
                    None
                } else {
                    Some(data)
                }
            }
            ScopedGroup::NoInvalidation(uri_map) => match &mut uri_map.entry(uri.to_string()) {
                Entry::Occupied(e) => {
                    e.get_mut().0.data = Some(data);
                    None
                }
                Entry::Vacant(_) => Some(data),
            },
        }
    }

    fn no_invalidation_iter(&self) -> impl Iterator<Item = &PatchInfo> {
        match self {
            ScopedGroup::PartialInvalidation(_) => NoInvalidationPatchesIter { it: None },
            ScopedGroup::NoInvalidation(map) => NoInvalidationPatchesIter {
                it: Some(map.values()),
            },
        }
    }
}

struct NoInvalidationPatchesIter<'a, T>
where
    T: Iterator<Item = &'a NoInvalidationPatch>,
{
    it: Option<T>,
}

impl<'a, T> Iterator for NoInvalidationPatchesIter<'a, T>
where
    T: Iterator<Item = &'a NoInvalidationPatch>,
{
    type Item = &'a PatchInfo;

    fn next(&mut self) -> Option<Self::Item> {
        let it = self.it.as_mut()?;
        Some(&it.next()?.0)
    }
}

// TODO Tests
// - tests where both tables have same compat id.
// - tests where duplicate uri's are present.
#[cfg(test)]
mod tests {
    use super::*;

    fn cid_1() -> CompatibilityId {
        CompatibilityId::from_u32s([0, 0, 0, 1])
    }

    fn cid_2() -> CompatibilityId {
        CompatibilityId::from_u32s([0, 0, 0, 2])
    }

    fn p1_full() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            1,
            &IftTableTag::IFT(cid_1()),
            PatchEncoding::TableKeyed {
                fully_invalidating: true,
            },
        )
    }

    fn p2_partial_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::IFT(cid_1()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p2_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::IFTX(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p2_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::IFTX(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p2_partial_c2_ift() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::IFT(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p3_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            &IftTableTag::IFTX(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p3_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            &IftTableTag::IFT(cid_1()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p4_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            &IftTableTag::IFT(cid_1()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p4_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            &IftTableTag::IFTX(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p5_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            5,
            &IftTableTag::IFTX(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn patch_info_ift(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            source_table: IftTableTag::IFT(cid_1()),
        }
    }

    fn patch_info_ift_c2(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            source_table: IftTableTag::IFT(cid_2()),
        }
    }

    fn patch_info_iftx(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            source_table: IftTableTag::IFTX(cid_2()),
        }
    }

    #[test]
    fn full_invalidation() {
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p1_full()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Full(FullInvalidationPatch(patch_info_ift("//foo.bar/04")))
        );

        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![
                p1_full(),
                p2_partial_c1(),
                p3_partial_c2(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Full(FullInvalidationPatch(patch_info_ift("//foo.bar/04"),))
        );
    }

    #[test]
    fn mixed() {
        // (partial, no inval)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0K".to_string(),
                    NoInvalidationPatch(patch_info_iftx("//foo.bar/0K"))
                )]))
            }
        );

        // (no inval, partial)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p4_no_c1(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0G".to_string(),
                    NoInvalidationPatch(patch_info_ift("//foo.bar/0G"))
                )])),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0C"
                ),))
            }
        );

        // (partial, empty)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::default()),
            }
        );

        // (empty, partial)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::default()),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0C"
                ),)),
            }
        );
    }

    #[test]
    fn tables_have_same_compat_id() {
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![
                p2_partial_c1(),
                p2_partial_c2_ift(),
                p3_partial_c2(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            cid_2(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift_c2(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::new()),
            }
        );

        // Check that input order determines the winner.
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![
                p2_partial_c1(),
                p3_partial_c2(),
                p2_partial_c2_ift(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            cid_2(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0C"
                ),)),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::new()),
            }
        );
    }

    #[test]
    fn dedups_uris() {
        // Duplicates inside a scope
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p4_no_c1(), p4_no_c1()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0G".to_string(),
                    NoInvalidationPatch(patch_info_ift("//foo.bar/0G"))
                )])),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::new()),
            }
        );

        // Duplicates across scopes (no invalidation + no invalidation)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p4_no_c1(), p4_no_c2(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0G".to_string(),
                    NoInvalidationPatch(patch_info_ift("//foo.bar/0G"))
                )])),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0K".to_string(),
                    NoInvalidationPatch(patch_info_iftx("//foo.bar/0K"))
                )])),
            }
        );

        // Duplicates across scopes (partial + partial)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p2_partial_c2(), p3_partial_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ))),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0C"
                ))),
            }
        );

        // Duplicates across scopes (partial + no invalidation)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p2_no_c2(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ))),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0K".to_string(),
                    NoInvalidationPatch(patch_info_iftx("//foo.bar/0K"))
                )])),
            }
        );

        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p3_no_c1(), p4_no_c1()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0G".to_string(),
                    NoInvalidationPatch(patch_info_ift("//foo.bar/0G"))
                )])),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0C"
                ))),
            }
        );
    }

    fn create_group_for(uris: Vec<PatchUri>) -> PatchApplicationGroup<'static> {
        let data = FontRef::new(font_test_data::CMAP12_FONT1).unwrap();
        let group =
            PatchApplicationGroup::select_next_patches_from_candidates(uris, cid_1(), cid_2())
                .unwrap();

        PatchApplicationGroup {
            font: data,
            patches: Some(group),
        }
    }

    #[test]
    fn pending_uris() {
        assert_eq!(
            create_group_for(vec![]).pending_uris(),
            [].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p1_full()]).pending_uris(),
            ["//foo.bar/04"].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p2_partial_c1(), p3_partial_c2()]).pending_uris(),
            ["//foo.bar/08", "//foo.bar/0C"].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p2_partial_c1()]).pending_uris(),
            ["//foo.bar/08",].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p3_partial_c2()]).pending_uris(),
            ["//foo.bar/0C"].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p2_partial_c1(), p4_no_c2(), p5_no_c2()]).pending_uris(),
            ["//foo.bar/08", "//foo.bar/0G", "//foo.bar/0K"]
                .into_iter()
                .collect()
        );

        assert_eq!(
            create_group_for(vec![p3_partial_c2(), p4_no_c1()]).pending_uris(),
            ["//foo.bar/0C", "//foo.bar/0G"].into_iter().collect()
        );

        assert_eq!(
            create_group_for(vec![p4_no_c1(), p5_no_c2()]).pending_uris(),
            ["//foo.bar/0G", "//foo.bar/0K"].into_iter().collect()
        );
    }

    #[test]
    fn add_patch_data() {
        // Full
        let mut g = create_group_for(vec![p1_full()]);
        assert!(g.has_pending_uris());
        assert_eq!(
            g.add_patch_data("//foo.bar/04", vec![1]),
            // TODO: patch application isn't implemented yet, update this once it is.
            Some(Err(PatchingError::InternalError))
        );
        assert_eq!(g.pending_uris(), [].into_iter().collect());
        assert!(!g.has_pending_uris());

        // Mixed
        let mut g = create_group_for(vec![p2_partial_c1(), p3_partial_c2()]);
        assert!(g.has_pending_uris());
        assert_eq!(g.add_patch_data("//foo.bar/0C", vec![1]), None);
        assert_eq!(g.pending_uris(), ["//foo.bar/08"].into_iter().collect());
        assert!(g.has_pending_uris());

        let mut g = create_group_for(vec![p4_no_c2(), p5_no_c2(), p2_partial_c1()]);
        assert!(g.has_pending_uris());
        assert_eq!(g.add_patch_data("//foo.bar/0K", vec![1]), None);
        assert_eq!(
            g.pending_uris(),
            ["//foo.bar/08", "//foo.bar/0G"].into_iter().collect()
        );
        assert!(g.has_pending_uris());

        assert_eq!(g.add_patch_data("//foo.bar/08", vec![1]), None);
        assert_eq!(
            g.add_patch_data("//foo.bar/0G", vec![1]),
            // TODO: patch application isn't implemented yet, update this once it is.
            Some(Err(PatchingError::InternalError))
        );
        assert_eq!(g.pending_uris(), [].into_iter().collect());
        assert!(!g.has_pending_uris());
    }

    #[test]
    fn add_patch_data_ignores_unknown() {
        let mut g = create_group_for(vec![p1_full()]);
        assert!(g.has_pending_uris());
        assert_eq!(g.add_patch_data("//foo.bar/foo", vec![1]), None);
        assert_eq!(g.pending_uris(), ["//foo.bar/04"].into_iter().collect());
        assert!(g.has_pending_uris());

        let mut g = create_group_for(vec![p2_partial_c1()]);
        assert!(g.has_pending_uris());
        assert_eq!(g.add_patch_data("//foo.bar/foo", vec![1]), None);
        assert_eq!(g.pending_uris(), ["//foo.bar/08"].into_iter().collect());
        assert!(g.has_pending_uris());

        let mut g = create_group_for(vec![p4_no_c2()]);
        assert!(g.has_pending_uris());
        assert_eq!(g.add_patch_data("//foo.bar/foo", vec![1]), None);
        assert_eq!(g.pending_uris(), ["//foo.bar/0G"].into_iter().collect());
        assert!(g.has_pending_uris());
    }
}
