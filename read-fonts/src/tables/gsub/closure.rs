//! Computing the closure over a set of glyphs
//!
//! This means taking a set of glyphs and updating it to include any other glyphs
//! reachable from those glyphs via substitution, recursively.

use std::collections::HashSet;

use font_types::GlyphId;

use crate::{
    tables::layout::{ExtensionLookup, Subtables},
    FontRead, ReadError,
};

use super::{
    AlternateSubstFormat1, Gsub, LigatureSubstFormat1, MultipleSubstFormat1,
    ReverseChainSingleSubstFormat1, SingleSubst, SingleSubstFormat1, SingleSubstFormat2,
    SubstitutionSubtables,
};

/// A trait for tables which participate in closure
pub(crate) trait GlyphClosure {
    /// Update the set of glyphs with any glyphs reachable via substitution.
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError>;
}

impl<'a> Gsub<'a> {
    /// Return the set of glyphs reachable from the input set via any substituion.
    pub fn closure_glyphs(
        &self,
        mut glyphs: HashSet<GlyphId>,
    ) -> Result<HashSet<GlyphId>, ReadError> {
        // we need to do this iteratively, since any glyph found in one pass
        // over the lookups could also be the target of substitutions.

        // we always call this once, and then keep calling if it produces
        // additional glyphs
        let mut prev_glyph_count = glyphs.len();
        self.closure_glyphs_once(&mut glyphs)?;
        let mut new_glyph_count = glyphs.len();

        while prev_glyph_count != new_glyph_count {
            prev_glyph_count = new_glyph_count;
            self.closure_glyphs_once(&mut glyphs)?;
            new_glyph_count = glyphs.len();
        }

        Ok(glyphs)
    }

    fn closure_glyphs_once(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        let lookup_list = self.lookup_list()?;
        for lookup in lookup_list.lookups().iter() {
            let subtables = lookup?.subtables()?;
            subtables.add_reachable_glyphs(glyphs)?;
        }
        Ok(())
    }
}

impl<'a> GlyphClosure for SubstitutionSubtables<'a> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        match self {
            SubstitutionSubtables::Single(tables) => tables.add_reachable_glyphs(glyphs),
            SubstitutionSubtables::Multiple(tables) => tables.add_reachable_glyphs(glyphs),
            SubstitutionSubtables::Alternate(tables) => tables.add_reachable_glyphs(glyphs),
            SubstitutionSubtables::Ligature(tables) => tables.add_reachable_glyphs(glyphs),
            SubstitutionSubtables::Reverse(tables) => tables.add_reachable_glyphs(glyphs),
            _ => Ok(()),
        }
    }
}

impl<'a, T: FontRead<'a> + GlyphClosure + 'a, Ext: ExtensionLookup<'a, T> + 'a> GlyphClosure
    for Subtables<'a, T, Ext>
{
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        self.iter()
            .try_for_each(|t| t?.add_reachable_glyphs(glyphs))
    }
}

impl<'a> GlyphClosure for SingleSubst<'a> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        for (target, replacement) in self.iter_subs()? {
            if glyphs.contains(&target) {
                glyphs.insert(replacement);
            }
        }
        Ok(())
    }
}

impl<'a> SingleSubst<'a> {
    fn iter_subs(&self) -> Result<impl Iterator<Item = (GlyphId, GlyphId)> + '_, ReadError> {
        let (left, right) = match self {
            SingleSubst::Format1(t) => (Some(t.iter_subs()?), None),
            SingleSubst::Format2(t) => (None, Some(t.iter_subs()?)),
        };
        Ok(left
            .into_iter()
            .flatten()
            .chain(right.into_iter().flatten()))
    }
}

impl<'a> SingleSubstFormat1<'a> {
    fn iter_subs(&self) -> Result<impl Iterator<Item = (GlyphId, GlyphId)> + '_, ReadError> {
        let delta = self.delta_glyph_id();
        let coverage = self.coverage()?;
        Ok(coverage.iter().filter_map(move |gid| {
            let raw = (gid.to_u16() as i32).checked_add(delta as i32);
            let raw = raw.and_then(|raw| u16::try_from(raw).ok())?;
            Some((gid, GlyphId::new(raw)))
        }))
    }
}

impl<'a> SingleSubstFormat2<'a> {
    fn iter_subs(&self) -> Result<impl Iterator<Item = (GlyphId, GlyphId)> + '_, ReadError> {
        let coverage = self.coverage()?;
        let subs = self.substitute_glyph_ids();
        Ok(coverage.iter().zip(subs.iter().map(|id| id.get())))
    }
}

impl<'a> GlyphClosure for MultipleSubstFormat1<'a> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        let coverage = self.coverage()?;
        let sequences = self.sequences();
        for (gid, replacements) in coverage.iter().zip(sequences.iter()) {
            let replacements = replacements?;
            if glyphs.contains(&gid) {
                glyphs.extend(
                    replacements
                        .substitute_glyph_ids()
                        .iter()
                        .map(|gid| gid.get()),
                );
            }
        }
        Ok(())
    }
}

impl<'a> GlyphClosure for AlternateSubstFormat1<'a> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        let coverage = self.coverage()?;
        let alts = self.alternate_sets();
        for (gid, alts) in coverage.iter().zip(alts.iter()) {
            let alts = alts?;
            if glyphs.contains(&gid) {
                glyphs.extend(alts.alternate_glyph_ids().iter().map(|gid| gid.get()));
            }
        }
        Ok(())
    }
}

impl<'a> GlyphClosure for LigatureSubstFormat1<'a> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        let coverage = self.coverage()?;
        let ligs = self.ligature_sets();
        for (gid, lig_set) in coverage.iter().zip(ligs.iter()) {
            let lig_set = lig_set?;
            if glyphs.contains(&gid) {
                for lig in lig_set.ligatures().iter() {
                    let lig = lig?;
                    if lig
                        .component_glyph_ids()
                        .iter()
                        .all(|gid| glyphs.contains(&gid.get()))
                    {
                        glyphs.insert(lig.ligature_glyph());
                    }
                }
            }
        }
        Ok(())
    }
}

impl GlyphClosure for ReverseChainSingleSubstFormat1<'_> {
    fn add_reachable_glyphs(&self, glyphs: &mut HashSet<GlyphId>) -> Result<(), ReadError> {
        for coverage in self
            .backtrack_coverages()
            .iter()
            .chain(self.lookahead_coverages().iter())
        {
            if !coverage?.iter().any(|gid| glyphs.contains(&gid)) {
                return Ok(());
            }
        }

        for (gid, sub) in self.coverage()?.iter().zip(self.substitute_glyph_ids()) {
            if glyphs.contains(&gid) {
                glyphs.insert(sub.get());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::{FontRef, TableProvider};

    use super::*;
    use font_test_data::closure as test_data;

    struct GlyphMap {
        to_gid: HashMap<&'static str, GlyphId>,
        from_gid: HashMap<GlyphId, &'static str>,
    }

    impl GlyphMap {
        fn new(raw_order: &'static str) -> GlyphMap {
            let to_gid: HashMap<_, _> = raw_order
                .split('\n')
                .map(|line| line.trim())
                .filter(|line| !(line.starts_with('#') || line.is_empty()))
                .enumerate()
                .map(|(gid, name)| (name, GlyphId::new(gid.try_into().unwrap())))
                .collect();
            let from_gid = to_gid.iter().map(|(name, gid)| (*gid, *name)).collect();
            GlyphMap { from_gid, to_gid }
        }

        fn get_gid(&self, name: &str) -> Option<GlyphId> {
            self.to_gid.get(name).copied()
        }

        fn get_name(&self, gid: GlyphId) -> Option<&str> {
            self.from_gid.get(&gid).copied()
        }
    }

    fn get_gsub(test_data: &'static [u8]) -> Gsub<'_> {
        let font = FontRef::new(test_data).unwrap();
        font.gsub().unwrap()
    }

    fn compute_closure(gsub: &Gsub, glyph_map: &GlyphMap, input: &[&str]) -> HashSet<GlyphId> {
        let input_glyphs = input
            .iter()
            .map(|name| glyph_map.get_gid(name).unwrap())
            .collect();
        gsub.closure_glyphs(input_glyphs).unwrap()
    }

    /// assert a set of glyph ids matches a slice of names
    macro_rules! assert_closure_result {
        ($glyph_map:expr, $result:expr, $expected:expr) => {
            let result = $result
                .iter()
                .map(|gid| $glyph_map.get_name(*gid).unwrap())
                .collect::<HashSet<_>>();
            let expected = $expected.iter().copied().collect::<HashSet<_>>();
            if expected != result {
                let in_output = result.difference(&expected).collect::<Vec<_>>();
                let in_expected = expected.difference(&result).collect::<Vec<_>>();
                let mut msg = format!("Closure output does not match\n");
                if !in_expected.is_empty() {
                    msg.push_str(format!("missing {in_expected:?}\n").as_str());
                }
                if !in_output.is_empty() {
                    msg.push_str(format!("unexpected {in_output:?}").as_str());
                }
                panic!("{msg}")
            }
        };
    }

    #[test]
    fn smoke_test() {
        // tests various lookup types.
        // test input is font-test-data/test_data/fea/simple_closure.fea
        let gsub = get_gsub(test_data::SIMPLE);
        let glyph_map = GlyphMap::new(test_data::SIMPLE_GLYPHS);
        let result = compute_closure(&gsub, &glyph_map, &["a"]);

        assert_closure_result!(
            glyph_map,
            result,
            &["a", "A", "b", "c", "d", "a_a", "a.1", "a.2", "a.3"]
        );
    }

    #[test]
    fn recursive() {
        // a scenario in which one substitution adds glyphs that trigger additional
        // substitutions.
        //
        // test input is font-test-data/test_data/fea/recursive_closure.fea
        let gsub = get_gsub(test_data::RECURSIVE);
        let glyph_map = GlyphMap::new(test_data::RECURSIVE_GLYPHS);
        let result = compute_closure(&gsub, &glyph_map, &["a"]);
        assert_closure_result!(glyph_map, result, &["a", "b", "c", "d"]);
    }
}
