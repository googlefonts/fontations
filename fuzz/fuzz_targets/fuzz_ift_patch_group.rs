#![no_main]
//! Fuzzes the incremental_font_transfer patch_group.rs API

use std::collections::{HashMap, HashSet};

use font_types::Fixed;
use incremental_font_transfer::{
    patch_group::{PatchGroup, UrlStatus},
    patchmap::{DesignSpace, FeatureSet, PatchUrl, SubsetDefinition},
};
use libfuzzer_sys::{arbitrary, fuzz_target};
use read_fonts::{
    collections::{IntSet, RangeSet},
    types::Tag,
};
use shared_brotli_patch_decoder::NoopBrotliDecoder;
use skrifa::FontRef;
use write_fonts::FontBuilder;

#[derive(Debug, arbitrary::Arbitrary)]
struct FuzzInput {
    // Build font up from tagged data blobs to bypass some of the complexity in finding a valid font file
    // none of the IFT code should have issues with fonts malformed at the top level as parsing is left
    // up to read-fonts and skrifa.
    font_tables: HashMap<u32, Vec<u8>>,

    // Parts of the target subset definition.
    codepoints: HashSet<u32>,
    features: Option<HashSet<u32>>,
    design_space: HashMap<u32, Vec<(i32, i32)>>,

    // Patches
    patches: HashMap<String, Vec<u8>>,
    applied_patches: HashSet<String>,
}

impl FuzzInput {
    fn to_font(&self) -> Vec<u8> {
        let mut font_builder = FontBuilder::new();

        self.font_tables
            .iter()
            .map(|(tag, data)| (Tag::from_u32(*tag), data))
            .for_each(|(tag, data)| {
                font_builder.add_raw(tag, data);
            });

        font_builder.build()
    }

    fn to_subset_definition(&self) -> SubsetDefinition {
        let codepoints: IntSet<u32> = self.codepoints.iter().copied().collect();

        let feature_set = if let Some(tags) = &self.features {
            FeatureSet::Set(tags.iter().copied().map(Tag::from_u32).collect())
        } else {
            FeatureSet::All
        };

        let design_space: HashMap<Tag, RangeSet<Fixed>> = self
            .design_space
            .iter()
            .map(|(tag, v)| {
                let v: RangeSet<Fixed> = v
                    .iter()
                    .map(|(start, end)| Fixed::from_i32(*start)..=Fixed::from_i32(*end))
                    .collect();
                (Tag::from_u32(*tag), v)
            })
            .collect();

        SubsetDefinition::new(codepoints, feature_set, DesignSpace::Ranges(design_space))
    }
}

/// Used to ensure read only functions don't get optimized away.
fn black_box<T>(dummy: T) -> T {
    unsafe { std::ptr::read_volatile(&dummy) }
}

fuzz_target!(|input: FuzzInput| {
    let font_data = input.to_font();
    let Ok(font) = FontRef::new(&font_data) else {
        return;
    };

    let subset_definition = input.to_subset_definition();

    // TODO(garretrieger): generate a random patch data map.
    let Ok(group) = PatchGroup::select_next_patches(font, &Default::default(), &subset_definition)
    else {
        return;
    };

    // Exercise urls() api on group
    black_box(group.has_urls());
    for url in group.urls() {
        black_box(url);
    }

    // Exercise patch application.
    let mut url_map: HashMap<PatchUrl, UrlStatus> = input
        .patches
        .into_iter()
        .map(|(url, data)| (PatchUrl(url), UrlStatus::Pending(data)))
        .collect();
    for url in input.applied_patches {
        url_map.insert(PatchUrl(url), UrlStatus::Applied);
    }

    // When running under a fuzzer disable brotli decoding and instead just pass through the input data.
    // This allows the fuzzer to more effectively explore code gated behind brotli decoding.
    //
    // TODO(garretrieger): In addition to the noop decoder, also have one that can return all of the possible errors.
    let _ = black_box(group.apply_next_patches_with_decoder(&mut url_map, &NoopBrotliDecoder));
});
