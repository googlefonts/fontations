use read_fonts::{tables::ift::CompatibilityId, FontRef, ReadError, TableProvider};
use std::{collections::BTreeMap, marker::PhantomData};

use crate::patchmap::{intersecting_patches, PatchEncoding, PatchUri, SubsetDefinition};

/// A group of patches derived from a single IFT font which can be applied simulatenously
/// to that font.
pub struct PatchApplicationGroup<'a> {
    font: FontRef<'a>,
    patches: CompatibleGroup,
}

impl PatchApplicationGroup<'_> {
    pub fn select_next_patches<'a>(
        font: FontRef<'a>,
        subset_definition: &SubsetDefinition,
    ) -> Result<PatchApplicationGroup<'a>, ReadError> {
        let candidates = intersecting_patches(&font, subset_definition)?;
        let compat_group = Self::select_next_patches_from_candidates(
            candidates,
            font.ift()?.compatibility_id(),
            font.iftx()?.compatibility_id(),
        )?;

        Ok(PatchApplicationGroup {
            font,
            patches: compat_group,
        })
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

        // TODO: On construction we need to ensure that there are no PatchInfo's with duplicate URIs.

        let mut full_invalidation: Vec<FullInvalidationPatch> = vec![];
        let mut partial_invalidation_ift: Vec<PartialInvalidationPatch<AffectsIft>> = vec![];
        let mut partial_invalidation_iftx: Vec<PartialInvalidationPatch<AffectsIftx>> = vec![];
        let mut no_invalidation_ift: BTreeMap<String, NoInvalidationPatch<AffectsIft>> =
            Default::default();
        let mut no_invalidation_iftx: BTreeMap<String, NoInvalidationPatch<AffectsIftx>> =
            Default::default();

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
                        partial_invalidation_ift
                            .push(PartialInvalidationPatch::<AffectsIft>(uri.into()))
                    } else if *uri.expected_compatibility_id() == iftx_compat_id {
                        partial_invalidation_iftx
                            .push(PartialInvalidationPatch::<AffectsIftx>(uri.into()))
                    }
                }
                PatchEncoding::GlyphKeyed => {
                    if *uri.expected_compatibility_id() == ift_compat_id {
                        no_invalidation_ift.insert(
                            uri.uri_string(),
                            NoInvalidationPatch::<AffectsIft>(uri.into()),
                        );
                    } else if *uri.expected_compatibility_id() == iftx_compat_id {
                        no_invalidation_iftx.insert(
                            uri.uri_string(),
                            NoInvalidationPatch::<AffectsIftx>(uri.into()),
                        );
                    }
                }
            }
        }

        // Step 2 - now make patch selections in priority order: first full invalidation, second partial, lastly none.
        if let Some(patch) = full_invalidation.into_iter().next() {
            // TODO(garretrieger): use a heuristic to select the best patch
            return Ok(CompatibleGroup::Full(patch));
        }

        let ift_scope = partial_invalidation_ift
            .into_iter()
            // TODO(garretrieger): use a heuristic to select the best patch
            .next()
            .map(|patch| ScopedGroup::<AffectsIft>::PartialInvalidation(patch));

        let iftx_scope = partial_invalidation_iftx
            .into_iter()
            // TODO(garretrieger): use a heuristic to select the best patch
            .next()
            .map(|patch| ScopedGroup::<AffectsIftx>::PartialInvalidation(patch));

        match (ift_scope, iftx_scope) {
            (Some(scope1), Some(scope2)) => Ok(CompatibleGroup::Mixed {
                ift: scope1,
                iftx: scope2,
            }),
            (Some(scope1), None) => Ok(CompatibleGroup::Mixed {
                ift: scope1,
                iftx: ScopedGroup::NoInvalidation::<AffectsIftx>(no_invalidation_iftx),
            }),
            (None, Some(scope2)) => Ok(CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation::<AffectsIft>(no_invalidation_ift),
                iftx: scope2,
            }),
            (None, None) => Ok(CompatibleGroup::Mixed {
                ift: ScopedGroup::NoInvalidation::<AffectsIft>(no_invalidation_ift),
                iftx: ScopedGroup::NoInvalidation::<AffectsIftx>(no_invalidation_iftx),
            }),
        }
    }

    fn add_patch_data(&mut self, uri: String, data: Vec<u8>) {
        todo!()
    }

    fn pending_uris(&self) -> Vec<&str> {
        todo!()
    }

    // How do we ensure all patch data is present? should we set this up so data is non optional.
    fn apply_to(&self, font: FontRef) {
        todo!()
    }
}

/// Marks which mappinng table is affected (via invalidation) by application of a patch.
pub(crate) trait PatchScope {}

/// This patch affects only the "IFT " table.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct AffectsIft;

/// This patch affects only the "IFTX" table.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct AffectsIftx;

/// This patch affects both the "IFT " and "IFTX" table.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct AffectsBoth;

impl PatchScope for AffectsIft {}
impl PatchScope for AffectsIftx {}
impl PatchScope for AffectsBoth {}

/// Tracks information related to a patch necessary to apply that patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct PatchInfo<T>
where
    T: PatchScope,
{
    uri: String,
    data: Option<Vec<u8>>,
    _phantom: PhantomData<T>,
    // TODO: details for how to mark the patch applied in the mapping table (ie. bit index to flip).
    // TODO: Signals for heuristic patch selection:
}

impl<T> From<PatchUri> for PatchInfo<T>
where
    T: PatchScope,
{
    fn from(value: PatchUri) -> Self {
        PatchInfo {
            uri: value.uri_string(),
            data: None,
            _phantom: Default::default(),
        }
    }
}

/// Type for a single non invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct NoInvalidationPatch<T>(PatchInfo<T>)
where
    T: PatchScope;

/// Type for a single partially invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct PartialInvalidationPatch<T>(PatchInfo<T>)
where
    T: PatchScope;

/// Type for a single fully invalidating patch.
#[derive(PartialEq, Eq, Debug)]
pub(crate) struct FullInvalidationPatch(PatchInfo<AffectsBoth>);

/// Represents a group of patches which are valid (compatible) to be applied together to
/// an IFT font.
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum CompatibleGroup {
    Full(FullInvalidationPatch),
    Mixed {
        ift: ScopedGroup<AffectsIft>,
        iftx: ScopedGroup<AffectsIftx>,
    },
}

/// A set of zero or more compatible patches that are derived from the same scope
/// ("IFT " vs "IFTX")
#[derive(PartialEq, Eq, Debug)]
pub(crate) enum ScopedGroup<T>
where
    T: PatchScope,
{
    PartialInvalidation(PartialInvalidationPatch<T>),
    NoInvalidation(BTreeMap<String, NoInvalidationPatch<T>>),
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
            &cid_1(),
            PatchEncoding::TableKeyed {
                fully_invalidating: true,
            },
        )
    }

    fn p2_partial_c1() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            2,
            &cid_1(),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p3_partial_c2() -> PatchUri {
        PatchUri::from_index(
            "//foo.bar/{id}",
            3,
            &cid_2(),
            PatchEncoding::TableKeyed {
                fully_invalidating: false,
            },
        )
    }

    fn p4_no_c1() -> PatchUri {
        PatchUri::from_index("//foo.bar/{id}", 4, &cid_1(), PatchEncoding::GlyphKeyed)
    }

    fn p5_no_c2() -> PatchUri {
        PatchUri::from_index("//foo.bar/{id}", 5, &cid_2(), PatchEncoding::GlyphKeyed)
    }

    fn patch_info<T>(uri: &str) -> PatchInfo<T>
    where
        T: PatchScope,
    {
        PatchInfo {
            uri: uri.to_string(),
            data: None,
            _phantom: Default::default(),
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
            CompatibleGroup::Full(FullInvalidationPatch(patch_info("//foo.bar/04")))
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
            CompatibleGroup::Full(FullInvalidationPatch(patch_info("//foo.bar/04"),))
        );
    }

    #[test]
    fn mixed() {
        // (partial, no)
        let group = PatchApplicationGroup::select_next_patches_from_candidates(
            vec![p2_partial_c1(), p4_no_c1(), p5_no_c2()],
            cid_1(),
            cid_2(),
        )
        .unwrap();

        assert_eq!(
            group,
            CompatibleGroup::Mixed {
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info(
                    "//foo.bar/08"
                ),)),
                iftx: ScopedGroup::NoInvalidation(BTreeMap::from([(
                    "//foo.bar/0K".to_string(),
                    NoInvalidationPatch(patch_info("//foo.bar/0K"))
                )]))
            }
        );

        // (no, partial)
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
                    NoInvalidationPatch(patch_info("//foo.bar/0G"))
                )])),
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info(
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
                ift: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info(
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
                iftx: ScopedGroup::PartialInvalidation(PartialInvalidationPatch(patch_info(
                    "//foo.bar/0C"
                ),)),
            }
        );
    }
}
