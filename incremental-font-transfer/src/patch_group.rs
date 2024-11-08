use read_fonts::{tables::ift::CompatibilityId, FontRef, ReadError, TableProvider};
use std::collections::{btree_map::Entry, BTreeMap, HashSet};

use crate::{
    font_patch::{IncrementalFontPatchBase, PatchingError},
    patchmap::{intersecting_patches, IftTableTag, PatchEncoding, PatchUri, SubsetDefinition},
};

/// A group of patches derived from a single IFT font which can be applied simulatenously
/// to that font. Patches are initially missing data which must be fetched and supplied to
/// the group before it can be applied to the font.
pub struct PatchGroup<'a> {
    font: FontRef<'a>,
    patches: Option<CompatibleGroup>,
}

/// A group of patches and associated patch data which is ready to apply to the base font.
pub struct AppliablePatchGroup<'a> {
    font: FontRef<'a>,
    patches: CompatibleGroup,
}

pub enum AddDataResult<'a> {
    NeedsMoreData(PatchGroup<'a>),
    Ready(AppliablePatchGroup<'a>),
}

impl<'a> PatchGroup<'a> {
    /// Intersect the available and unapplied patches in ift_font against subset_definition and return a group
    /// of patches which would be applied next.
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

        let compat_group = Self::select_next_patches_from_candidates(
            candidates,
            ift_font.ift().ok().map(|t| t.compatibility_id()),
            ift_font.iftx().ok().map(|t| t.compatibility_id()),
        )?;

        Ok(PatchGroup {
            font: ift_font,
            patches: Some(compat_group),
        })
    }

    /// Returns the list of URIs in this group which do not yet have patch data supplied for them.
    pub fn pending_uris(&self) -> HashSet<&str> {
        let Some(patches) = &self.patches else {
            return Default::default();
        };

        match patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => {
                if info.data.is_some() {
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
    pub fn add_patch_data(mut self, uri: &'a str, data: Vec<u8>) -> AddDataResult {
        let Some(patches) = &mut self.patches else {
            return AddDataResult::NeedsMoreData(self);
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
            return AddDataResult::NeedsMoreData(self);
        }

        AddDataResult::Ready(AppliablePatchGroup {
            font: self.font,
            patches: self.patches.unwrap(),
        })
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
                    if Some(uri.expected_compatibility_id()) == ift_compat_id.as_ref() {
                        partial_invalidation_ift.push(PartialInvalidationPatch(uri.into()))
                    } else if Some(uri.expected_compatibility_id()) == iftx_compat_id.as_ref() {
                        partial_invalidation_iftx.push(PartialInvalidationPatch(uri.into()))
                    }
                }
                PatchEncoding::GlyphKeyed => {
                    if Some(uri.expected_compatibility_id()) == ift_compat_id.as_ref() {
                        no_invalidation_ift
                            .insert(uri.uri_string(), NoInvalidationPatch(uri.into()));
                    } else if Some(uri.expected_compatibility_id()) == iftx_compat_id.as_ref() {
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
            .find(|patch| {
                // TODO(garretrieger): use a heuristic to select the best patch
                let Some(selected) = &ift_selected_uri else {
                    return true;
                };
                selected != &patch.0.uri
            })
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

impl AppliablePatchGroup<'_> {
    pub fn apply_patches(self) -> Result<Vec<u8>, PatchingError> {
        match &self.patches {
            CompatibleGroup::Full(FullInvalidationPatch(info)) => {
                self.font.apply_table_keyed_patch(info)
            }
            CompatibleGroup::Mixed { ift, iftx } => {
                // Apply partial invalidation patches first
                let base = self.apply_partial_invalidation_patch(ift, None)?;
                let base = self.apply_partial_invalidation_patch(iftx, base)?;

                // Then apply no invalidation patches
                let mut combined = ift
                    .no_invalidation_iter()
                    .chain(iftx.no_invalidation_iter())
                    .peekable();

                if combined.peek().is_some() {
                    match base {
                        Some(base) => base.as_slice().apply_glyph_keyed_patches(combined),
                        None => self.font.apply_glyph_keyed_patches(combined),
                    }
                } else {
                    base.ok_or(PatchingError::EmptyPatchList)
                }
            }
        }
    }

    fn apply_partial_invalidation_patch(
        &self,
        scoped_group: &ScopedGroup,
        new_base: Option<Vec<u8>>,
    ) -> Result<Option<Vec<u8>>, PatchingError> {
        match (scoped_group, new_base) {
            (ScopedGroup::PartialInvalidation(patch), Some(new_base)) => {
                Ok(Some(new_base.as_slice().apply_table_keyed_patch(&patch.0)?))
            }
            (ScopedGroup::PartialInvalidation(patch), None) => {
                Ok(Some(self.font.apply_table_keyed_patch(&patch.0)?))
            }
            (_, new_base) => Ok(new_base),
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
        self.data.as_deref()
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
struct NoInvalidationPatch(PatchInfo);

/// Type for a single partially invalidating patch.
#[derive(PartialEq, Eq, Debug)]
struct PartialInvalidationPatch(PatchInfo);

/// Type for a single fully invalidating patch.
#[derive(PartialEq, Eq, Debug)]
struct FullInvalidationPatch(PatchInfo);

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
                        uris.insert(key);
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::glyph_keyed::tests::assemble_glyph_keyed_patch;
    use font_test_data::ift::{
        glyf_u16_glyph_patches, glyph_keyed_patch_header, table_keyed_format2, table_keyed_patch,
        test_font_for_patching_with_loca_mod,
    };

    use font_types::{Int24, Tag};

    use read_fonts::{test_helpers::BeBuffer, FontRef};

    use write_fonts::FontBuilder;

    const TABLE_1_FINAL_STATE: &[u8] = "hijkabcdeflmnohijkabcdeflmno\n".as_bytes();
    const TABLE_2_FINAL_STATE: &[u8] = "foobarbaz foobarbaz foobarbaz\n".as_bytes();

    fn base_font(ift: Option<BeBuffer>, iftx: Option<BeBuffer>) -> Vec<u8> {
        let mut font_builder = FontBuilder::new();

        if let Some(buffer) = &ift {
            font_builder.add_raw(Tag::new(b"IFT "), buffer.as_slice());
        }
        if let Some(buffer) = &iftx {
            font_builder.add_raw(Tag::new(b"IFTX"), buffer.as_slice());
        }

        font_builder.add_raw(Tag::new(b"tab1"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab2"), "foobar\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab4"), "abcdef\n".as_bytes());
        font_builder.add_raw(Tag::new(b"tab5"), "foobar\n".as_bytes());
        font_builder.build()
    }

    impl<'a> AddDataResult<'a> {
        fn unwrap_ready(self) -> AppliablePatchGroup<'a> {
            match self {
                AddDataResult::Ready(val) => val,
                AddDataResult::NeedsMoreData(_) => panic!("Expected to be ready."),
            }
        }

        fn unwrap_needs_more(self) -> PatchGroup<'a> {
            match self {
                AddDataResult::NeedsMoreData(val) => val,
                AddDataResult::Ready(_) => panic!("Expected to be needs more data."),
            }
        }
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
            &IftTableTag::Ift(cid_1()),
            PatchEncoding::TableKeyed {
                fully_invalidating: true,
            },
        )
    }

    fn p2_partial_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::Ift(cid_1()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p2_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::Iftx(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p2_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::Iftx(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p2_partial_c2_ift() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &IftTableTag::Ift(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p3_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            &IftTableTag::Iftx(cid_2()),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p3_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            &IftTableTag::Ift(cid_1()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p4_no_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            &IftTableTag::Ift(cid_1()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p4_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            4,
            &IftTableTag::Iftx(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn p5_no_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            5,
            &IftTableTag::Iftx(cid_2()),
            PatchEncoding::GlyphKeyed,
        )
    }

    fn patch_info_ift(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            source_table: IftTableTag::Ift(cid_1()),
        }
    }

    fn patch_info_ift_c2(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            source_table: IftTableTag::Ift(cid_2()),
        }
    }

    fn patch_info_iftx(uri: &str) -> PatchInfo {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
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
    fn tables_have_same_compat_id() {
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                p2_partial_c1(),
                p2_partial_c2_ift(),
                p3_partial_c2(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            Some(cid_2()),
            Some(cid_2()),
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
        let group = PatchGroup::select_next_patches_from_candidates(
            vec![
                p2_partial_c1(),
                p3_partial_c2(),
                p2_partial_c2_ift(),
                p4_no_c1(),
                p5_no_c2(),
            ],
            Some(cid_2()),
            Some(cid_2()),
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
        let g = create_group_for(vec![p1_full()]);
        assert!(g.has_pending_uris());
        let _ = g.add_patch_data("//foo.bar/04", vec![1]).unwrap_ready();

        // Mixed
        let g = create_group_for(vec![p2_partial_c1(), p3_partial_c2()]);
        assert!(g.has_pending_uris());
        let g = g
            .add_patch_data("//foo.bar/0C", vec![1])
            .unwrap_needs_more();
        assert_eq!(g.pending_uris(), ["//foo.bar/08"].into_iter().collect());
        assert!(g.has_pending_uris());

        let g = create_group_for(vec![p4_no_c2(), p5_no_c2(), p2_partial_c1()]);
        assert!(g.has_pending_uris());
        let g = g
            .add_patch_data("//foo.bar/0K", vec![1])
            .unwrap_needs_more();
        assert_eq!(
            g.pending_uris(),
            ["//foo.bar/08", "//foo.bar/0G"].into_iter().collect()
        );
        assert!(g.has_pending_uris());

        let g = g
            .add_patch_data("//foo.bar/08", vec![1])
            .unwrap_needs_more();
        let _ = g.add_patch_data("//foo.bar/0G", vec![1]).unwrap_ready();
    }

    #[test]
    fn add_patch_data_empty_group() {
        let g = empty_group();
        assert!(!g.has_pending_uris());
        assert_eq!(g.pending_uris(), [].into_iter().collect());
        let g = g
            .add_patch_data("//foo.bar/04", vec![1])
            .unwrap_needs_more();
        assert!(!g.has_pending_uris());
        assert_eq!(g.pending_uris(), [].into_iter().collect());
    }

    #[test]
    fn add_patch_data_ignores_unknown() {
        let g = create_group_for(vec![p1_full()]);
        assert!(g.has_pending_uris());
        let g = g
            .add_patch_data("//foo.bar/foo", vec![1])
            .unwrap_needs_more();
        assert_eq!(g.pending_uris(), ["//foo.bar/04"].into_iter().collect());
        assert!(g.has_pending_uris());

        let g = create_group_for(vec![p2_partial_c1()]);
        assert!(g.has_pending_uris());
        let g = g
            .add_patch_data("//foo.bar/foo", vec![1])
            .unwrap_needs_more();
        assert_eq!(g.pending_uris(), ["//foo.bar/08"].into_iter().collect());
        assert!(g.has_pending_uris());

        let g = create_group_for(vec![p4_no_c2()]);
        assert!(g.has_pending_uris());
        let g = g
            .add_patch_data("//foo.bar/foo", vec![1])
            .unwrap_needs_more();
        assert_eq!(g.pending_uris(), ["//foo.bar/0G"].into_iter().collect());
        assert!(g.has_pending_uris());
    }

    #[test]
    fn select_next_patches_no_intersection() {
        let font = base_font(Some(table_keyed_format2()), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([55].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        assert!(!g.has_pending_uris());
        assert_eq!(g.pending_uris(), [].into_iter().collect());

        let g = g
            .add_patch_data("foo/04", table_keyed_patch().as_slice().to_vec())
            .unwrap_needs_more();

        assert!(!g.has_pending_uris());
        assert_eq!(g.pending_uris(), [].into_iter().collect());
    }

    #[test]
    fn apply_patches_full_invalidation() {
        let font = base_font(Some(table_keyed_format2()), None);
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        assert!(g.has_pending_uris());

        let g = g
            .add_patch_data("foo/04", table_keyed_patch().as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );
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

        let g = g
            .add_patch_data("foo/04", table_keyed_patch().as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
        );

        // IFTX
        let font = base_font(None, Some(buffer.clone()));
        let font = FontRef::new(&font).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let g = g
            .add_patch_data("foo/04", table_keyed_patch().as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
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
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let mut patch_2 = table_keyed_patch();
        patch_2.write_at("compat_id", 2u32);
        patch_2.write_at("patch[0]", Tag::new(b"tab4"));
        patch_2.write_at("patch[1]", Tag::new(b"tab5"));

        let g = g
            .add_patch_data("foo/04", table_keyed_patch().as_slice().to_vec())
            .unwrap_needs_more()
            .add_patch_data("foo/08", patch_2.as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
        let new_font = FontRef::new(&new_font).unwrap();

        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab2")).unwrap().as_bytes(),
            TABLE_2_FINAL_STATE,
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
            |_| {},
            HashMap::from([
                (Tag::new(b"IFT "), ift_builder.as_slice()),
                (Tag::new(b"IFTX"), iftx_builder.as_slice()),
                (Tag::new(b"tab1"), "abcdef\n".as_bytes()),
            ]),
        );
        let font = FontRef::new(font.as_slice()).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let patch_ift = table_keyed_patch();
        let patch_iftx =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());

        let g = g
            .add_patch_data("foo/04", patch_ift.as_slice().to_vec())
            .unwrap_needs_more()
            .add_patch_data("foo/08", patch_iftx.as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
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
            ],
            new_glyf
        );
        assert_eq!(
            new_font.table_data(Tag::new(b"tab1")).unwrap().as_bytes(),
            TABLE_1_FINAL_STATE,
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
        iftx_builder.write_at("compat_id[0]", 6u32);
        iftx_builder.write_at("compat_id[1]", 7u32);
        iftx_builder.write_at("compat_id[2]", 8u32);
        iftx_builder.write_at("compat_id[3]", 9u32);
        iftx_builder.write_at("id_delta", Int24::new(1));

        let font = test_font_for_patching_with_loca_mod(
            |_| {},
            HashMap::from([
                (Tag::new(b"IFT "), ift_builder.as_slice()),
                (Tag::new(b"IFTX"), iftx_builder.as_slice()),
            ]),
        );

        let font = FontRef::new(font.as_slice()).unwrap();

        let s = SubsetDefinition::codepoints([5].into_iter().collect());
        let g = PatchGroup::select_next_patches(font, &s).unwrap();

        let patch1 =
            assemble_glyph_keyed_patch(glyph_keyed_patch_header(), glyf_u16_glyph_patches());

        let mut patch2 = glyf_u16_glyph_patches();
        patch2.write_at("gid_13", 14u16);
        let patch2 = assemble_glyph_keyed_patch(glyph_keyed_patch_header(), patch2);

        let g = g
            .add_patch_data("foo/04", patch1.as_slice().to_vec())
            .unwrap_needs_more()
            .add_patch_data("foo/08", patch2.as_slice().to_vec())
            .unwrap_ready();

        let new_font = g.apply_patches().unwrap();
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
    }
}
