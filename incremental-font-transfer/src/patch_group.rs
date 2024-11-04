use font_types::Tag;
use read_fonts::{FontRef, ReadError};
use std::{collections::BTreeMap, marker::PhantomData};

use crate::patchmap::{intersecting_patches, PatchEncoding, PatchUri, SubsetDefinition};

/// A group of patches derived from a single IFT font which can be applied simulatenously
/// to that font.
pub(crate) struct PatchApplicationGroup<'a> {
    font: FontRef<'a>,
    patches: CompatibleGroup,
}

impl PatchApplicationGroup<'_> {
    pub fn select_next_patches<'a>(
        font: FontRef<'a>,
        subset_definition: &SubsetDefinition,
    ) -> Result<PatchApplicationGroup<'a>, ReadError> {
        let candidates = intersecting_patches(&font, subset_definition)?;
        let compat_group = Self::select_next_patches_from_candidates(candidates)?;

        Ok(PatchApplicationGroup {
            font,
            patches: compat_group,
        })
    }

    fn select_next_patches_from_candidates(
        candidates: Vec<PatchUri>,
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
            match uri.encoding() {
                PatchEncoding::TableKeyed {
                    fully_invalidating: true,
                } => full_invalidation.push(FullInvalidationPatch(uri.into())),
                PatchEncoding::TableKeyed {
                    fully_invalidating: false,
                } => {
                    if uri.source_table() == Tag::new(b"IFT ") {
                        partial_invalidation_ift.push(PartialInvalidationPatch::<AffectsIft>(
                            uri.into(),
                            Default::default(),
                        ))
                    } else if uri.source_table() == Tag::new(b"IFTX") {
                        partial_invalidation_iftx.push(PartialInvalidationPatch::<AffectsIftx>(
                            uri.into(),
                            Default::default(),
                        ))
                    }
                }
                PatchEncoding::GlyphKeyed => {
                    if uri.source_table() == Tag::new(b"IFT ") {
                        no_invalidation_ift.insert(
                            "TODO".to_string(), // TODO(garretrieger): key should be the fully subbed URI string.
                            NoInvalidationPatch::<AffectsIft>(uri.into(), Default::default()),
                        );
                    } else if uri.source_table() == Tag::new(b"IFTX") {
                        no_invalidation_iftx.insert(
                            "TODO".to_string(), // TODO(garretrieger): key should be the fully subbed URI string.
                            NoInvalidationPatch::<AffectsIftx>(uri.into(), Default::default()),
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
trait PatchScope {}

/// This patch affects only the "IFT " table.
struct AffectsIft;

/// This patch affects only the "IFTX" table.
struct AffectsIftx;

/// This patch affects both the "IFT " and "IFTX" table.
struct AffectsBoth;

impl PatchScope for AffectsIft {}
impl PatchScope for AffectsIftx {}
impl PatchScope for AffectsBoth {}

/// Tracks information related to a patch necessary to apply that patch.
struct PatchInfo<T>
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
        todo!()
    }
}

/// Type for a single non invalidating patch.
struct NoInvalidationPatch<T>(PatchInfo<T>, PhantomData<T>)
where
    T: PatchScope;

/// Type for a single partially invalidating patch.
struct PartialInvalidationPatch<T>(PatchInfo<T>, PhantomData<T>)
where
    T: PatchScope;

/// Type for a single fully invalidating patch.
struct FullInvalidationPatch(PatchInfo<AffectsBoth>);

/// Represents a group of patches which are valid (compatible) to be applied together to
/// an IFT font.
enum CompatibleGroup {
    Full(FullInvalidationPatch),
    Mixed {
        ift: ScopedGroup<AffectsIft>,
        iftx: ScopedGroup<AffectsIftx>,
    },
}

/// A set of zero or more compatible patches that are derived from the same scope
/// ("IFT " vs "IFTX")
enum ScopedGroup<T>
where
    T: PatchScope,
{
    None,
    PartialInvalidation(PartialInvalidationPatch<T>),
    NoInvalidation(BTreeMap<String, NoInvalidationPatch<T>>),
}
