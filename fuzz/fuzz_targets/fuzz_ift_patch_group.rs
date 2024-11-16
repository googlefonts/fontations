#![no_main]
//! Fuzzes the incremental_font_transfer patch_group.rs API

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    ops::RangeInclusive,
};

use incremental_font_transfer::{patch_group::PatchGroup, patchmap::SubsetDefinition};
use libfuzzer_sys::{arbitrary, fuzz_target};
use read_fonts::{collections::IntSet, types::Tag};
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
    features: HashSet<u32>,
    design_space: HashMap<u32, Vec<(f64, f64)>>,
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
        let feature_tags: BTreeSet<Tag> =
            self.features.iter().copied().map(Tag::from_u32).collect();

        let design_space: HashMap<Tag, Vec<RangeInclusive<f64>>> = self
            .design_space
            .iter()
            .map(|(tag, v)| {
                let v: Vec<RangeInclusive<f64>> =
                    v.iter().map(|(start, end)| *start..=*end).collect();
                (Tag::from_u32(*tag), v)
            })
            .collect();
        SubsetDefinition::new(codepoints, feature_tags, design_space)
    }
}

fuzz_target!(|input: FuzzInput| {
    let font_data = input.to_font();
    let Ok(font) = FontRef::new(&font_data) else {
        return;
    };

    let subset_definition = input.to_subset_definition();

    let _ = PatchGroup::select_next_patches(font, &subset_definition);

    // TODO(garretrieger): also apply patches. for patches we will need to bypass brotli compression.
    // TODO(garretrieger): on patch application we should never see an incompatible patch error.
});
