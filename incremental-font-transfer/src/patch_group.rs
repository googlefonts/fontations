//! API for selecting and applying a group of IFT patches.
//!
//! This provides methods for selecting a maximal group of patches that are compatible with each other and
//! additionally methods for applying that group of patches.

use read_fonts::{tables::ift::CompatibilityId, FontRef, ReadError, TableProvider};
use std::collections::{BTreeMap, HashMap};

use crate::{
    font_patch::{IncrementalFontPatchBase, PatchingError},
    patchmap::{
        intersecting_patches, IftTableTag, IntersectionInfo, PatchFormat, PatchUri,
        SubsetDefinition, UriTemplateError,
    },
};

/// A group of patches derived from a single IFT font.
///
/// This is a group which can be applied simultaneously to that font. Patches are
/// initially missing data which must be fetched and supplied to patch application
/// method.
pub struct PatchGroup<'a> {
    font: FontRef<'a>,
    patches: Option<CompatibleGroup>,
}

impl PatchGroup<'_> {
    /// Intersect the available and unapplied patches in ift_font against subset_definition
    ///
    /// Returns a group of patches which would be applied next.
    pub fn select_next_patches<'b>(
        ift_font: FontRef<'b>,
        subset_definition: &SubsetDefinition,
    ) -> Result<PatchGroup<'b>, ReadError> {
        let candidates = intersecting_patches(&ift_font, subset_definition)?;
        if candidates.is_empty() {
            return Ok(PatchGroup {
                font: ift_font,
                patches: None,
            });
        }

        let ift_compat_id = ift_font.ift().ok().map(|t| t.compatibility_id());
        let iftx_compat_id = ift_font.iftx().ok().map(|t| t.compatibility_id());
        if ift_compat_id == iftx_compat_id {
            // The spec disallows two tables with same compat ids.
            // See: https://w3c.github.io/IFT/Overview.html#extend-font-subset
            return Err(ReadError::ValidationError);
        }

        let compat_group =
            Self::select_next_patches_from_candidates(candidates, ift_compat_id, iftx_compat_id)?;

        Ok(PatchGroup {
            font: ift_font,
            patches: Some(compat_group),
        })
    }

    /// Returns an iterator over URIs in this group.
    pub fn uris(&self) -> impl Iterator<Item = &str> {
        self.invalidating_patch_iter()
            .chain(self.non_invalidating_patch_iter())
            .map(|info| info.uri.as_str())
    }

    /// Returns true if there is at least one uri associated with this group.
    pub fn has_uris(&self) -> bool {
        let Some(patches) = &self.patches else {
            return false;
        };
        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(_)) => true,
            CompatibleGroup::Mixed { ift, iftx } => ift.has_uris() || iftx.has_uris(),
        }
    }

    fn next_invalidating_patch(&self) -> Option<&PatchInfo> {
        self.invalidating_patch_iter().next()
    }

    fn invalidating_patch_iter(&self) -> impl Iterator<Item = &PatchInfo> {
        let full = match &self.patches {
            Some(CompatibleGroup::Full(info)) => Some(&info.0),
            _ => None,
        };

        let partial_1 = match &self.patches {
            Some(CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(v),
                iftx: _,
            }) => Some(&v.0),
            _ => None,
        };

        let partial_2 = match &self.patches {
            Some(CompatibleGroup::Mixed {
                ift: _,
                iftx: ScopedGroup::PartialInvalidation(v),
            }) => Some(&v.0),
            _ => None,
        };

        full.into_iter().chain(partial_1).chain(partial_2)
    }

    fn non_invalidating_patch_iter(&self) -> impl Iterator<Item = &PatchInfo> {
        let ift = match &self.patches {
            Some(CompatibleGroup::Mixed { ift, iftx: _ }) => Some(ift),
            _ => None,
        };
        let iftx = match &self.patches {
            Some(CompatibleGroup::Mixed { ift: _, iftx }) => Some(iftx),
            _ => None,
        };

        let it1 = ift
            .into_iter()
            .flat_map(|scope| scope.no_invalidation_iter());
        let it2 = iftx
            .into_iter()
            .flat_map(|scope| scope.no_invalidation_iter());

        it1.chain(it2)
    }

    fn select_next_patches_from_candidates(
        candidates: Vec<PatchUri>,
        ift_compat_id: Option<CompatibilityId>,
        iftx_compat_id: Option<CompatibilityId>,
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

        // Step 1: sort the candidates into separate lists based on invalidation characteristics.
        let GroupingByInvalidation {
            full_invalidation,
            partial_invalidation_ift,
            partial_invalidation_iftx,
            mut no_invalidation_ift,
            mut no_invalidation_iftx,
        } = GroupingByInvalidation::group_patches(candidates, ift_compat_id, iftx_compat_id)
            .map_err(|_| ReadError::MalformedData("Malformed URI templates."))?;

        // Step 2 - now make patch selections in priority order: first full invalidation, second partial, lastly none.
        if let Some(patch) = Self::select_invalidating_candidate(full_invalidation) {
            // TODO(garretrieger): use a heuristic to select the best patch
            return Ok(CompatibleGroup::Full(patch.into()));
        }

        let mut ift_selected_uri: Option<String> = None;
        let ift_scope =
            Self::select_invalidating_candidate(partial_invalidation_ift).map(|patch| {
                ift_selected_uri = Some(patch.patch_info.uri.clone());
                ScopedGroup::PartialInvalidation(patch.into())
            });

        let mut iftx_selected_uri: Option<String> = None;
        let iftx_scope = Self::select_invalidating_candidate(
            partial_invalidation_iftx.into_iter().filter(|patch| {
                // TODO(garretrieger): use a heuristic to select the best patch
                let Some(selected) = &ift_selected_uri else {
                    return true;
                };
                selected != &patch.patch_info.uri
            }),
        )
        .map(|patch| {
            iftx_selected_uri = Some(patch.patch_info.uri.clone());
            ScopedGroup::PartialInvalidation(patch.into())
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

    /// Select an entry from a list of candidate invalidating entries according to the specs selection criteria.
    ///
    /// Context: <https://w3c.github.io/IFT/Overview.html#invalidating-patch-selection>
    fn select_invalidating_candidate<T>(candidates: T) -> Option<CandidatePatch>
    where
        T: IntoIterator<Item = CandidatePatch>,
    {
        // Note:
        // - As mentioned in the spec we can find at least one entry matching that criteria by finding an entry with the
        //   largest intersection (since that can't be a strict subset of others).
        // - Intersection size is tracked in intersection info.
        // - Ties are broken by entry order, which is also tracked in intersection info.
        // - So it's sufficient to just find a candidate patch with the largest intersection info, relying on it's
        //   Ord implementation.
        candidates
            .into_iter()
            .max_by_key(|candidate| candidate.intersection_info.clone())
    }

    /// Attempt to apply the next patch (or patches if non-invalidating) listed in this group.
    ///
    /// Returns the bytes of the updated font.
    pub fn apply_next_patches(
        self,
        patch_data: &mut HashMap<String, UriStatus>,
    ) -> Result<Vec<u8>, PatchingError> {
        if let Some(patch) = self.next_invalidating_patch() {
            let entry = patch_data
                .get_mut(&patch.uri)
                .ok_or(PatchingError::MissingPatches)?;

            match entry {
                UriStatus::Pending(patch_data) => {
                    let r = self.font.apply_table_keyed_patch(patch, patch_data)?;
                    *entry = UriStatus::Applied;
                    return Ok(r);
                }
                UriStatus::Applied => {} // previously applied uris are ignored according to the spec.
            }
        }

        // No invalidating patches left, so apply any non invalidating ones in one pass.
        // First check if we have all of the needed data.
        let new_font = {
            let mut accumulated_info: Vec<(&PatchInfo, &[u8])> = vec![];
            for info in self.non_invalidating_patch_iter() {
                let data = patch_data
                    .get(&info.uri)
                    .ok_or(PatchingError::MissingPatches)?;

                match data {
                    UriStatus::Pending(data) => accumulated_info.push((info, data)),
                    UriStatus::Applied => {} // previously applied uris are ignored according to the spec.
                }
            }

            if accumulated_info.is_empty() {
                return Err(PatchingError::EmptyPatchList);
            }

            self.font
                .apply_glyph_keyed_patches(accumulated_info.into_iter())?
        };

        for info in self.non_invalidating_patch_iter() {
            if let Some(status) = patch_data.get_mut(&info.uri) {
                *status = UriStatus::Applied;
            };
        }

        Ok(new_font)
    }
}

#[derive(Default)]
struct GroupingByInvalidation {
    full_invalidation: Vec<CandidatePatch>,
    partial_invalidation_ift: Vec<CandidatePatch>,
    partial_invalidation_iftx: Vec<CandidatePatch>,
    // TODO(garretrieger): do we need sorted order, use HashMap instead?
    no_invalidation_ift: BTreeMap<String, NoInvalidationPatch>,
    no_invalidation_iftx: BTreeMap<String, NoInvalidationPatch>,
}

impl GroupingByInvalidation {
    fn group_patches(
        candidates: Vec<PatchUri>,
        ift_compat_id: Option<CompatibilityId>,
        iftx_compat_id: Option<CompatibilityId>,
    ) -> Result<GroupingByInvalidation, UriTemplateError> {
        let mut result: GroupingByInvalidation = Default::default();

        for uri in candidates.into_iter() {
            // TODO(garretrieger): for efficiency can we delay uri template resolution until we have actually selected patches?
            // TODO(garretrieger): for btree construction don't recompute the resolved uri, cache inside the patch uri object?
            match uri.encoding() {
                PatchFormat::TableKeyed {
                    fully_invalidating: true,
                } => result.full_invalidation.push(uri.try_into()?),
                PatchFormat::TableKeyed {
                    fully_invalidating: false,
                } => {
                    if Some(uri.expected_compatibility_id()) == ift_compat_id.as_ref() {
                        result.partial_invalidation_ift.push(uri.try_into()?)
                    } else if Some(uri.expected_compatibility_id()) == iftx_compat_id.as_ref() {
                        result.partial_invalidation_iftx.push(uri.try_into()?)
                    }
                }
                PatchFormat::GlyphKeyed => {
                    if Some(uri.expected_compatibility_id()) == ift_compat_id.as_ref() {
                        result
                            .no_invalidation_ift
                            .insert(uri.uri_string()?, NoInvalidationPatch(uri.try_into()?));
                    } else if Some(uri.expected_compatibility_id()) == iftx_compat_id.as_ref() {
                        result
                            .no_invalidation_iftx
                            .insert(uri.uri_string()?, NoInvalidationPatch(uri.try_into()?));
                    }
                }
            }
        }

        Ok(result)
    }
}

/// Tracks whether a URI has already been applied to a font or not.
#[derive(PartialEq, Eq, Debug)]
pub enum UriStatus {
    Applied,
    Pending(Vec<u8>),
}

/// Tracks information related to a patch necessary to apply that patch.
#[derive(PartialEq, Eq, Debug)]
pub struct PatchInfo {
    uri: String,
    source_table: IftTableTag,
    application_flag_bit_index: usize,
}

impl PatchInfo {
    pub(crate) fn tag(&self) -> &IftTableTag {
        &self.source_table
    }

    pub(crate) fn application_flag_bit_index(&self) -> usize {
        self.application_flag_bit_index
    }
}

impl TryFrom<PatchUri> for PatchInfo {
    type Error = UriTemplateError;

    fn try_from(value: PatchUri) -> Result<Self, Self::Error> {
        Ok(PatchInfo {
            uri: value.uri_string()?,
            application_flag_bit_index: value.application_flag_bit_index(),
            source_table: value.source_table(),
        })
    }
}

/// Type to track a patch being considered for selection.
struct CandidatePatch {
    intersection_info: IntersectionInfo,
    patch_info: PatchInfo,
}

impl TryFrom<PatchUri> for CandidatePatch {
    type Error = UriTemplateError;

    fn try_from(value: PatchUri) -> Result<Self, Self::Error> {
        Ok(Self {
            intersection_info: value.intersection_info(),
            patch_info: value.try_into()?,
        })
    }
}

/// Type for a single non invalidating patch.
#[derive(PartialEq, Eq, Debug)]
struct NoInvalidationPatch(PatchInfo);

/// Type for a single partially invalidating patch.
#[derive(PartialEq, Eq, Debug)]
struct PartialInvalidationPatch(PatchInfo);

impl From<CandidatePatch> for PartialInvalidationPatch {
    fn from(value: CandidatePatch) -> Self {
        Self(value.patch_info)
    }
}

/// Type for a single fully invalidating patch.
#[derive(PartialEq, Eq, Debug)]
struct FullInvalidationPatch(PatchInfo);

impl From<CandidatePatch> for FullInvalidationPatch {
    fn from(value: CandidatePatch) -> Self {
        Self(value.patch_info)
    }
}

/// Represents a group of patches which are valid (compatible) to be applied together to
/// an IFT font.
#[derive(PartialEq, Eq, Debug)]
enum CompatibleGroup {
    Full(FullInvalidationPatch),
    Mixed { ift: ScopedGroup, iftx: ScopedGroup },
}

/// A set of zero or more compatible patches that are derived from the same scope
/// ("IFT " vs "IFTX")
#[derive(PartialEq, Eq, Debug)]
enum ScopedGroup {
    PartialInvalidation(PartialInvalidationPatch),
    NoInvalidation(BTreeMap<String, NoInvalidationPatch>),
}

impl ScopedGroup {
    fn has_uris(&self) -> bool {
        match self {
            ScopedGroup::PartialInvalidation(PartialInvalidationPatch(_)) => true,
            ScopedGroup::NoInvalidation(uri_map) => !uri_map.is_empty(),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::{
        glyph_keyed::tests::assemble_glyph_keyed_patch,
        testdata::test_font_for_patching_with_loca_mod,
    };
    use font_test_data::{
        bebuffer::BeBuffer,
        ift::{
            glyf_u16_glyph_patches, glyph_keyed_patch_header, table_keyed_format2,
            table_keyed_patch,
        },
    };

    use font_types::{Int24, Tag};

    use read_fonts::{
        tables::ift::{IFTX_TAG, IFT_TAG},
        FontRef,
    };

    use write_fonts::FontBuilder;

    const TABLE_1_FINAL_STATE: &[u8] = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
    const TABLE_2_FINAL_STATE: &[u8] = "foobarbaz foobarbaz foobarbaz\n".as_bytes();

    fn base_font(ift: Option<BeBuffer>, iftx: Option<BeBuffer>) -> Vec<u8> {
        let mut font_builder = FontBuilder::new();

        if let Some(buffer) = &ift {
            font_builder.add_raw(IFT_TAG, buffer.as_slice());
        }
        if let Some(buffer) = &iftx {
            font_builder.add_raw(IFTX_TAG, buffer.as_slice());
        }

        font_builder.add_raw(Tag::new(b"tab1"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab2"), "foobar\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab4"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab5"), "foobar\n".as_bytes());
        font_builder.build()
    }

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
            IftTableTag::Ift(cid_1()),
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: true,
            },
            Default::default(),
        )
    }

    fn p2_partial_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            IftTableTag::Ift(cid_1()),
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: false,
            },
            Default::default(),
        )
    }

    fn p2_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            IftTableTag::Iftx(cid_2()),
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: false,
            },
            Default::default(),
        )
    }

    fn p2_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            IftTableTag::Iftx(cid_2()),
            42,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
    }

    fn p3_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            IftTableTag::Iftx(cid_2()),
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: false,
            },
            Default::default(),
        )
    }

    fn p3_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            IftTableTag::Ift(cid_1()),
            42,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
    }

    fn p4_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            IftTableTag::Ift(cid_1()),
            42,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
    }

    fn p4_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            IftTableTag::Iftx(cid_2()),
            42,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
    }

    fn p5_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            5,
            IftTableTag::Iftx(cid_2()),
            42,
            PatchFormat::GlyphKeyed,
            Default::default(),
        )
    }

    fn full(index: u32, codepoints: u64) -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            index,
            IftTableTag::Ift(cid_1()),
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: true,
            },
            IntersectionInfo::new(codepoints, 0, 0),
        )
    }

    fn partial(index: u32, compat_id: CompatibilityId, codepoints: u64) -> PatchUri {
        let tag = if compat_id == cid_1() {
            IftTableTag::Ift(compat_id)
        } else {
            IftTableTag::Iftx(compat_id)
        };
        PatchUri::from_index(
            "//foo.bar/{id}",
            index,
            tag,
            42,
            PatchFormat::TableKeyed {
                fully_invalidating: false,
            },
            IntersectionInfo::new(codepoints, 0, 0),
        )
    }

    fn patch_info_ift(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            application_flag_bit_index: 42,
            source_table: IftTableTag::Ift(cid_1()),
        }
    }

    fn patch_info_iftx(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            application_flag_bit_index: 42,
            source_table: IftTableTag::Iftx(cid_2()),
        }
    }

    #[test]
    fn full_invalidation() {
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p1_full()],
            Some(cid_1()),
            Some(cid_2()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Full(FullInvalidationPatch(patch_info_ift("//foo.bar/04")))
        );

        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                p1_full(),
                p2_partial_c1(),
                p3_partial_c2(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            Some(cid_1()),
            Some(cid_2()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Full(FullInvalidationPatch(patch_info_ift("//foo.bar/04"),))
        );
    }

    #[test]
    fn full_invalidation_selection_order() {
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![full(3, 9), full(1, 7), full(2, 24)],
            Some(cid_1()),
            Some(cid_2()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Full(FullInvalidationPatch(patch_info_ift("//foo.bar/08")))
        );
    }

    #[test]
    fn partial_invalidation_selection_order() {
        // Only IFT
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                partial(3, cid_1(), 9),
                partial(1, cid_1(), 23),
                partial(2, cid_1(), 24),
            ],
            Some(cid_1()),
            Some(cid_2()),
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

        // Only IFTX
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                partial(4, cid_2(), 1),
                partial(5, cid_2(), 22),
                partial(6, cid_2(), 2),
            ],
            Some(cid_1()),
            Some(cid_2()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(BTreeMap::default()),

                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0K"
                ),)),
            }
        );

        // Both
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                partial(3, cid_1(), 9),
                partial(1, cid_1(), 23),
                partial(2, cid_1(), 24),
                partial(4, cid_2(), 1),
                partial(5, cid_2(), 22),
                partial(6, cid_2(), 2),
            ],
            Some(cid_1()),
            Some(cid_2()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_iftx(
                    "//foo.bar/0K"
                ),)),
            }
        );
    }

    #[test]
    fn mixed() {
        // (partial, no inval)
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p4_no_c1(), p5_no_c2()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p5_no_c2()],
            Some(cid_1()),
            Some(cid_2()),
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
    fn missing_compat_ids() {
        // (None, None)
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            None,
            None,
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(Default::default()),
                iftx: ScopedGroup::NoInvalidation(Default::default()),
            }
        );

        // (Some, None)
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            Some(cid_1()),
            None,
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::NoInvalidation(Default::default()),
            }
        );

        // (None, Some)
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            None,
            Some(cid_1()),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation(Default::default()),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info_ift(
                    "//foo.bar/08"
                ),)),
            }
        );
    }

    #[test]
    fn dedups_uris() {
        // Duplicates inside a scope
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p4_no_c1(), p4_no_c1()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p4_no_c1(), p4_no_c2(), p5_no_c2()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p2_partial_c2(), p3_partial_c2()],
            Some(cid_1()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p2_no_c2(), p5_no_c2()],
            Some(cid_1()),
            Some(cid_2()),
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

        let group = PatchGroup::select_next_patches_from_candidates(
            vec![p3_partial_c2(), p3_no_c1(), p4_no_c1()],
            Some(cid_1()),
            Some(cid_2()),
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

    fn create_group_for(uris: Vec<PatchUri>) -> PatchGroup<'static> {
        let data = FontRef::new(font_test_data::CMAP12_FONT1).unwrap();
        let group =
            PatchGroup::select_next_patches_from_candidates(uris, Some(cid_1()), Some(cid_2()))
                .unwrap();

        PatchGroup {
            font: data,
            patches: Some(group),
        }
    }

    fn empty_group() -> PatchGroup<'static> {
        let data = FontRef::new(font_test_data::CMAP12_FONT1).unwrap();
        PatchGroup {
            font: data,
            patches: None,
        }
    }

    #[test]
    fn uris() {
        let g = create_group_for(vec![]);
        assert_eq!(g.uris().collect::<Vec<&str>>(), Vec::<&str>::default());
        assert!(!g.has_uris());

        let g = empty_group();
        assert_eq!(g.uris().collect::<Vec<&str>>(), Vec::<&str>::default());
        assert!(!g.has_uris());

        let g = create_group_for(vec![p1_full()]);
        assert_eq!(g.uris().collect::<Vec<&str>>(), vec!["//foo.bar/04"],);
        assert!(g.has_uris());

        let g = create_group_for(vec![p2_partial_c1(), p3_partial_c2()]);
        assert_eq!(
            g.uris().collect::<Vec<&str>>(),
            vec!["//foo.bar/08", "//foo.bar/0C"]
        );
        assert!(g.has_uris());

        let g = create_group_for(vec![p2_partial_c1()]);
        assert_eq!(g.uris().collect::<Vec<&str>>(), vec!["//foo.bar/08",],);
        assert!(g.has_uris());

        let g = create_group_for(vec![p3_partial_c2()]);
        assert_eq!(g.uris().collect::<Vec<&str>>(), vec!["//foo.bar/0C"],);
        assert!(g.has_uris());

        let g = create_group_for(vec![p2_partial_c1(), p4_no_c2(), p5_no_c2()]);
        assert_eq!(
            g.uris().collect::<Vec<&str>>(),
            vec!["//foo.bar/08", "//foo.bar/0G", "//foo.bar/0K"],
        );
        assert!(g.has_uris());

        let g = create_group_for(vec![p3_partial_c2(), p4_no_c1()]);
        assert_eq!(
            g.uris().collect::<Vec<&str>>(),
            vec!["//foo.bar/0C", "//foo.bar/0G"],
        );

        let g = create_group_for(vec![p4_no_c1(), p5_no_c2()]);
        assert_eq!(
            g.uris().collect::<Vec<&str>>(),
            vec!["//foo.bar/0G", "//foo.bar/0K"],
        );
        assert!(g.has_uris());
    }

    #[test]
    fn select_next_patches_no_intersection() {
        let font = base_font(Some(table_keyed_format2()), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([55].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        assert!(!g.has_uris());
        assert_eq!(g.uris().collect::<Vec<&str>>(), Vec::<&str>::default());

        assert_eq!(
            g.apply_next_patches(&mut Default::default()),
            Err(PatchingError::EmptyPatchList)
        );
    }

    #[test]
    fn apply_patches_full_invalidation() {
        let font = base_font(Some(table_keyed_format2()), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        assert!(g.has_uris());
        let mut patch_data = HashMap::from([
            (
                "foo/04".to_string(),
                UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
            ),
            (
                "foo/bar".to_string(),
                UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
            ),
        ]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );

        assert_eq!(
            patch_data,
            HashMap::from([
                ("foo/04".to_string(), UriStatus::Applied,),
                (
                    "foo/bar".to_string(),
                    UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
                ),
            ])
        )
    }

    #[test]
    fn apply_patches_one_partial_invalidation() {
        let mut buffer = table_keyed_format2();
        buffer.write_at("encoding", 2u8);

        // IFT
        let font = base_font(Some(buffer.clone()), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let mut patch_data = HashMap::from([(
            "foo/04".to_string(),
            UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
        )]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );

        assert_eq!(
            patch_data,
            HashMap::from([("foo/04".to_string(), UriStatus::Applied,),])
        );

        // IFTX
        let font = base_font(None, Some(buffer.clone()));
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let mut patch_data = HashMap::from([(
            "foo/04".to_string(),
            UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
        )]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );

        assert_eq!(
            patch_data,
            HashMap::from([("foo/04".to_string(), UriStatus::Applied,),])
        );
    }

    #[test]
    fn apply_patches_two_partial_invalidation() {
        let mut ift_buffer = table_keyed_format2();
        ift_buffer.write_at("encoding", 2u8);

        let mut iftx_buffer = table_keyed_format2();
        iftx_buffer.write_at("compat_id[0]", 2u32);
        iftx_buffer.write_at("encoding", 2u8);
        iftx_buffer.write_at("id_delta", Int24::new(1));

        let font = base_font(Some(ift_buffer), Some(iftx_buffer));
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font.clone(), &s).unwrap();

        let mut patch_2 = table_keyed_patch();
        patch_2.write_at("compat_id", 2u32);
        patch_2.write_at("patch[0]", Tag::new(b"tab4"));
        patch_2.write_at("patch[1]", Tag::new(b"tab5"));

        let mut patch_data = HashMap::from([
            (
                "foo/04".to_string(),
                UriStatus::Pending(table_keyed_patch().as_slice().to_vec()),
            ),
            (
                "foo/08".to_string(),
                UriStatus::Pending(patch_2.as_slice().to_vec()),
            ),
        ]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );

        // only the first patch gets applied so tab4/tab5 are unchanged.
        assert_eq!(
            new_font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
            font.table_data(Tag::new(b"tab4")).unwrap().as_bytes(),
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab5")).unwrap().as_bytes(),
            font.table_data(Tag::new(b"tab5")).unwrap().as_bytes(),
        );
    }

    #[test]
    fn apply_patches_mixed() {
        let mut ift_builder = table_keyed_format2();
        ift_builder.write_at("encoding", 2u8);

        let mut iftx_builder = table_keyed_format2();
        iftx_builder.write_at("encoding", 3u8);
        iftx_builder.write_at("compat_id[0]", 6u32);
        iftx_builder.write_at("compat_id[1]", 7u32);
        iftx_builder.write_at("compat_id[2]", 8u32);
        iftx_builder.write_at("compat_id[3]", 9u32);
        iftx_builder.write_at("id_delta", Int24::new(1));

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (IFT_TAG, ift_builder.as_slice()),
                (IFTX_TAG, iftx_builder.as_slice()),
                (Tag::new(b"tab1"), "abcdef\n".as_bytes()),
            ]),
        );
        let font = FontRef::new(font.as_slice()).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font.clone(), &s).unwrap();

        let patch_ift = table_keyed_patch();
        let patch_iftx =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());

        let mut patch_data = HashMap::from([
            (
                "foo/04".to_string(),
                UriStatus::Pending(patch_ift.as_slice().to_vec()),
            ),
            (
                "foo/08".to_string(),
                UriStatus::Pending(patch_iftx.as_slice().to_vec()),
            ),
        ]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );

        // only the partial invalidation patch gets applied, so glyf is unchanged.
        assert_eq!(
            new_font.table_data(Tag::new(b"glyf")).unwrap().as_bytes(),
            font.table_data(Tag::new(b"glyf")).unwrap().as_bytes(),
        );
    }

    #[test]
    fn apply_patches_all_no_invalidation() {
        let mut ift_builder = table_keyed_format2();
        ift_builder.write_at("encoding", 3u8);
        ift_builder.write_at("compat_id[0]", 6u32);
        ift_builder.write_at("compat_id[1]", 7u32);
        ift_builder.write_at("compat_id[2]", 8u32);
        ift_builder.write_at("compat_id[3]", 9u32);

        let mut iftx_builder = table_keyed_format2();
        iftx_builder.write_at("encoding", 3u8);
        iftx_builder.write_at("compat_id[0]", 7u32);
        iftx_builder.write_at("compat_id[1]", 7u32);
        iftx_builder.write_at("compat_id[2]", 8u32);
        iftx_builder.write_at("compat_id[3]", 9u32);
        iftx_builder.write_at("id_delta", Int24::new(1));

        let font = test_font_for_patching_with_loca_mod(
            true,
            |_| {},
            HashMap::from([
                (IFT_TAG, ift_builder.as_slice()),
                (IFTX_TAG, iftx_builder.as_slice()),
            ]),
        );

        let font = FontRef::new(font.as_slice()).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let patch1 =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());

        let mut patch2 = glyf_u16_glyph_patches();
        patch2.write_at("gid_13", 14u16);
        let mut header = glyph_keyed_patch_header();
        header.write_at("compatibility_id", 7u32);
        let patch2 = assemble_glyph_keyed_patch(header, patch2);

        let mut patch_data = HashMap::from([
            (
                "foo/04".to_string(),
                UriStatus::Pending(patch1.as_slice().to_vec()),
            ),
            (
                "foo/08".to_string(),
                UriStatus::Pending(patch2.as_slice().to_vec()),
            ),
        ]);

        let new_font = g.apply_next_patches(&mut patch_data).unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        let new_glyf: &[u8] = new_font.table_data(Tag::new(b"glyf")).unwrap().as_bytes();
        assert_eq!(
            &[
                1, 2, 3, 4, 5, 0, // gid 0
                6, 7, 8, 0, // gid 1
                b'a', b'b', b'c', 0, // gid2
                b'd', b'e', b'f', b'g', // gid 7
                b'h', b'i', b'j', b'k', b'l', 0, // gid 8 + 9
                b'm', b'n', // gid 13
                b'm', b'n', // gid 14
            ],
            new_glyf
        );

        assert_eq!(
            patch_data,
            HashMap::from([
                ("foo/04".to_string(), UriStatus::Applied,),
                ("foo/08".to_string(), UriStatus::Applied,),
            ])
        );

        // there should be no more applicable patches left now.
        let g = PatchGroup::select_next_patches(new_font, &s).unwrap();
        assert!(!g.has_uris());
    }

    #[test]
    fn tables_have_same_compat_id() {
        let ift_buffer = table_keyed_format2();
        let iftx_buffer = table_keyed_format2();

        let font = base_font(Some(ift_buffer), Some(iftx_buffer));
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font.clone(), &s);

        assert!(g.is_err(), "did not fail as expected.");
        if let Err(err) = g {
            assert_eq!(ReadError::ValidationError, err);
        }
    }

    #[test]
    fn invalid_uri_templates() {
        let mut buffer = table_keyed_format2();
        buffer.write_at("uri_template_var_end", b'~');

        let font = base_font(Some(buffer), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());

        let Err(err) = PatchGroup::select_next_patches(font, &s) else {
            panic!("Should have failed")
        };
        assert_eq!(err, ReadError::MalformedData("Malformed URI templates."));
    }
}
