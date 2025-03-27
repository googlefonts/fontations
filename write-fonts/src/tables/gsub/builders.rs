//! GSUB lookup builders

use std::{collections::BTreeMap, convert::TryFrom};

use types::GlyphId16;

use crate::tables::{layout::builders::Builder, variations::ivs_builder::VariationStoreBuilder};

/// A builder for [`SingleSubst`](super::SingleSubst) subtables.
#[derive(Clone, Debug, Default)]
pub struct SingleSubBuilder {
    items: BTreeMap<GlyphId16, GlyphId16>,
}

impl SingleSubBuilder {
    /// Add this replacement to the builder.
    ///
    /// If there is an existing substitution for the provided target, it will
    /// be overwritten.
    pub fn insert(&mut self, target: GlyphId16, replacement: GlyphId16) {
        self.items.insert(target, replacement);
    }

    /// Returns `true` if all the pairs of items in the two iterators can be
    /// added to this lookup.
    ///
    /// The iterators are expected to be equal length.
    pub fn can_add(&self, target: GlyphId16, replacement: GlyphId16) -> bool {
        // only false if target exists with a different replacement
        !matches!(self.items.get(&target), Some(x) if *x != replacement)
    }

    /// Returns `true` if there are no substitutions in this builder.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate all the substitution pairs in this builder.
    ///
    /// used when compiling the `aalt` feature.
    pub fn iter_pairs(&self) -> impl Iterator<Item = (GlyphId16, GlyphId16)> + '_ {
        self.items.iter().map(|(target, alt)| (*target, *alt))
    }

    /// Convert this `SingleSubBuilder` into a `MultipleSubBuilder`.
    ///
    /// This is used by the fea compiler in some cases to reduce the number
    /// of generated lookups.
    pub fn promote_to_multi_sub(self) -> MultipleSubBuilder {
        MultipleSubBuilder {
            items: self
                .items
                .into_iter()
                .map(|(key, gid)| (key, vec![gid]))
                .collect(),
        }
    }
}

impl Builder for SingleSubBuilder {
    type Output = Vec<super::SingleSubst>;

    fn build(self, _: &mut VariationStoreBuilder) -> Self::Output {
        if self.items.is_empty() {
            return Default::default();
        }
        // if all pairs are equidistant and within the i16 range, find the
        // common delta
        let delta = self
            .items
            .iter()
            .map(|(k, v)| v.to_u16() as i32 - k.to_u16() as i32)
            .reduce(|acc, val| if acc == val { acc } else { i32::MAX })
            .and_then(|delta| i16::try_from(delta).ok());

        let coverage = self.items.keys().copied().collect();
        if let Some(delta) = delta {
            vec![super::SingleSubst::format_1(coverage, delta)]
        } else {
            let replacements = self.items.values().copied().collect();
            vec![super::SingleSubst::format_2(coverage, replacements)]
        }
    }
}

/// A builder for [`MultipleSubstFormat1`](super::MultipleSubstFormat1) subtables.
#[derive(Clone, Debug, Default)]
pub struct MultipleSubBuilder {
    items: BTreeMap<GlyphId16, Vec<GlyphId16>>,
}

impl Builder for MultipleSubBuilder {
    type Output = Vec<super::MultipleSubstFormat1>;

    fn build(self, _: &mut VariationStoreBuilder) -> Self::Output {
        let coverage = self.items.keys().copied().collect();
        let seq_tables = self.items.into_values().map(super::Sequence::new).collect();
        vec![super::MultipleSubstFormat1::new(coverage, seq_tables)]
    }
}

impl MultipleSubBuilder {
    /// Add a new substitution to this builder.
    ///
    /// If the target already exists with a different replacement, it will be
    /// overwritten.
    pub fn insert(&mut self, target: GlyphId16, replacement: Vec<GlyphId16>) {
        self.items.insert(target, replacement);
    }

    /// Returns `true` if no other replacement already exists for this target.
    pub fn can_add(&self, target: GlyphId16, replacement: &[GlyphId16]) -> bool {
        match self.items.get(&target) {
            None => true,
            Some(thing) => thing == replacement,
        }
    }
}

/// A builder for [`AlternateSubstFormat1`](super::AlternateSubstFormat1) subtables
#[derive(Clone, Debug, Default)]
pub struct AlternateSubBuilder {
    items: BTreeMap<GlyphId16, Vec<GlyphId16>>,
}

impl AlternateSubBuilder {
    /// Add a new alternate sub rule to this lookup.
    pub fn insert(&mut self, target: GlyphId16, replacement: Vec<GlyphId16>) {
        self.items.insert(target, replacement);
    }

    /// Returns `true` if this builder contains no rules.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Iterate all alternates in this lookup.
    ///
    /// used when compiling aalt
    pub fn iter_pairs(&self) -> impl Iterator<Item = (GlyphId16, GlyphId16)> + '_ {
        self.items
            .iter()
            .flat_map(|(target, alt)| alt.iter().map(|alt| (*target, *alt)))
    }
}

impl Builder for AlternateSubBuilder {
    type Output = Vec<super::AlternateSubstFormat1>;

    fn build(self, _: &mut VariationStoreBuilder) -> Self::Output {
        let coverage = self.items.keys().copied().collect();
        let seq_tables = self
            .items
            .into_values()
            .map(super::AlternateSet::new)
            .collect();
        vec![super::AlternateSubstFormat1::new(coverage, seq_tables)]
    }
}

/// A builder for [`LigatureSubstFormat1`](super::LigatureSubstFormat1) subtables.
#[derive(Clone, Debug, Default)]
pub struct LigatureSubBuilder {
    items: BTreeMap<GlyphId16, Vec<(Vec<GlyphId16>, GlyphId16)>>,
}

impl LigatureSubBuilder {
    /// Add a new ligature substitution rule to the builder.
    pub fn insert(&mut self, target: Vec<GlyphId16>, replacement: GlyphId16) {
        let (first, rest) = target.split_first().unwrap();
        let entry = self.items.entry(*first).or_default();
        // skip duplicates
        if !entry
            .iter()
            .any(|existing| (existing.0 == rest && existing.1 == replacement))
        {
            entry.push((rest.to_owned(), replacement))
        }
    }

    /// Check if this target sequence already has a replacement in this lookup.
    pub fn can_add(&self, target: &[GlyphId16], replacement: GlyphId16) -> bool {
        let Some((first, rest)) = target.split_first() else {
            return false;
        };
        match self.items.get(first) {
            Some(ligs) => !ligs
                .iter()
                .any(|(seq, target)| seq == rest && *target != replacement),
            None => true,
        }
    }
}

impl Builder for LigatureSubBuilder {
    type Output = Vec<super::LigatureSubstFormat1>;

    fn build(self, _: &mut VariationStoreBuilder) -> Self::Output {
        let coverage = self.items.keys().copied().collect();
        let lig_sets = self
            .items
            .into_values()
            .map(|mut ligs| {
                // we want to sort longer items first, but otherwise preserve
                // the order provided by the user.
                ligs.sort_by_key(|(lig, _)| std::cmp::Reverse(lig.len()));
                super::LigatureSet::new(
                    ligs.into_iter()
                        .map(|(components, lig_glyph)| super::Ligature::new(lig_glyph, components))
                        .collect(),
                )
            })
            .collect();

        vec![super::LigatureSubstFormat1::new(coverage, lig_sets)]
    }
}
