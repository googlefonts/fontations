//! Closure support for the ISO OFF extended GSUB formats.

use super::*;
use crate::tables::gsub::{
    AlternateSubstFormat2, MultipleSubstFormat2, SingleSubstFormat3, SingleSubstFormat4,
};

impl GlyphClosure for SingleSubstFormat3<'_> {
    fn closure_glyphs(
        &self,
        ctx: &mut ClosureCtx,
        _lookup_list: &SubstitutionLookupList,
        _lookup_index: u16,
    ) -> Result<(), ReadError> {
        if self.coverage_offset().is_null() {
            return Ok(());
        }
        let coverage = self.coverage()?;
        let mask = 0xffffff;
        // Preserve the same degenerate-font guardrails as the 16-bit format.
        if coverage.population() >= mask as usize {
            return Ok(());
        }
        let intersection = coverage.intersect_set(ctx.parent_active_glyphs());
        let Some(first) = intersection.first() else {
            return Ok(());
        };
        let delta = self.delta_glyph_id().to_i32();
        let min_before = first.to_u32();
        let max_before = intersection.last().unwrap().to_u32();
        let min_after = min_before.wrapping_add_signed(delta) & mask;
        let max_after = max_before.wrapping_add_signed(delta) & mask;
        if intersection.len() == (max_before - min_before + 1) as u64
            && ((min_before <= min_after && min_after <= max_before)
                || (min_before <= max_after && max_after <= max_before))
        {
            return Ok(());
        }
        ctx.output.extend(
            intersection
                .iter()
                .map(|gid| GlyphId::new(gid.to_u32().wrapping_add_signed(delta) & mask)),
        );
        Ok(())
    }
}

impl GlyphClosure for SingleSubstFormat4<'_> {
    fn closure_glyphs(
        &self,
        ctx: &mut ClosureCtx,
        _lookup_list: &SubstitutionLookupList,
        _lookup_index: u16,
    ) -> Result<(), ReadError> {
        if self.coverage_offset().is_null() || self.glyph_count().to_u32() == 0 {
            return Ok(());
        }
        let coverage = self.coverage()?;
        let glyph_set = ctx.active_glyphs_stack.last().unwrap_or(&*ctx.glyphs);
        let substitutes = self.substitute_glyph_ids();
        if self.glyph_count().to_u32() as u64 > glyph_set.len() * coverage.cost() as u64 {
            ctx.output.extend(
                glyph_set
                    .iter()
                    .filter_map(|gid| coverage.get(gid))
                    .filter_map(|index| substitutes.get(index as usize))
                    .map(|gid| GlyphId::from(gid.get())),
            );
        } else {
            ctx.output.extend(
                coverage
                    .iter()
                    .zip(substitutes)
                    .filter(|(gid, _)| glyph_set.contains(*gid))
                    .map(|(_, gid)| GlyphId::from(gid.get())),
            );
        }
        Ok(())
    }
}

macro_rules! intersects_coverage {
    ($($table:ident),* $(,)?) => {$(
        impl Intersect for $table<'_> {
            fn intersects(&self, glyphs: &IntSet<GlyphId>) -> Result<bool, ReadError> {
                if self.coverage_offset().is_null() {
                    return Ok(false);
                }
                Ok(self.coverage()?.intersects(glyphs))
            }
        }
    )*};
}

intersects_coverage!(
    SingleSubstFormat3,
    SingleSubstFormat4,
    MultipleSubstFormat2,
    AlternateSubstFormat2,
);

macro_rules! closure_array_sets {
    ($table:ident, $count:ident, $sets:ident, $glyphs:ident) => {
        impl GlyphClosure for $table<'_> {
            fn closure_glyphs(
                &self,
                ctx: &mut ClosureCtx,
                _lookup_list: &SubstitutionLookupList,
                _lookup_index: u16,
            ) -> Result<(), ReadError> {
                if self.coverage_offset().is_null() || self.$count().to_u32() == 0 {
                    return Ok(());
                }
                let coverage = self.coverage()?;
                let glyph_set = ctx.active_glyphs_stack.last().unwrap_or(&*ctx.glyphs);
                let sets = self.$sets();
                if self.$count().to_u32() as u64 > glyph_set.len() * coverage.cost() as u64 {
                    ctx.output.extend(
                        glyph_set
                            .iter()
                            .filter_map(|gid| coverage.get(gid))
                            .filter_map(|index| sets.get(index as usize).ok())
                            .flat_map(|set| {
                                set.$glyphs().iter().map(|gid| GlyphId::from(gid.get()))
                            }),
                    );
                } else {
                    ctx.output.extend(
                        coverage
                            .iter()
                            .zip(sets.iter_as_nullable())
                            .filter_map(|(gid, set)| {
                                if glyph_set.contains(gid) {
                                    set.transpose().ok().flatten()
                                } else {
                                    None
                                }
                            })
                            .flat_map(|set| {
                                set.$glyphs().iter().map(|gid| GlyphId::from(gid.get()))
                            }),
                    );
                }
                Ok(())
            }
        }
    };
}

closure_array_sets!(
    MultipleSubstFormat2,
    sequence_count,
    sequences,
    substitute_glyph_ids
);
closure_array_sets!(
    AlternateSubstFormat2,
    alternate_set_count,
    alternate_sets,
    alternate_glyph_ids
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FontData;
    use font_test_data::bebuffer::BeBuffer;
    use font_types::{GlyphId24, Int24, Uint24};

    fn gsub_with_lookup(kind: u16, subtable: &[u8]) -> Vec<u8> {
        BeBuffer::new()
            .push(1u16)
            .push(2u16)
            .push(0u16)
            .push(0u16)
            .push(0u16)
            .push(0u32)
            .push(0u32)
            .push(0u32)
            .push(26u32)
            .push(1u16)
            .push(6u32)
            .push(kind)
            .push(0u16)
            .push(1u16)
            .push(8u16)
            .extend(subtable.iter().copied())
            .to_vec()
    }

    fn close_lookup(kind: u16, subtable: &[u8], inputs: &[u32]) -> Vec<u32> {
        let bytes = gsub_with_lookup(kind, subtable);
        let gsub = Gsub::read(FontData::new(&bytes)).unwrap();
        let mut glyphs: IntSet<_> = inputs.iter().copied().map(GlyphId::new).collect();
        let mut lookups: IntSet<_> = [0].into_iter().collect();
        gsub.closure_lookups(&glyphs, &mut lookups).unwrap();
        assert_eq!(lookups.iter().collect::<Vec<_>>(), [0]);
        gsub.closure_glyphs(&lookups, &mut glyphs).unwrap();
        glyphs.iter().map(GlyphId::to_u32).collect()
    }

    fn close(subtable: &[u8], inputs: &[u32]) -> Vec<u32> {
        close_lookup(1, subtable, inputs)
    }

    #[test]
    fn single_subst3_closure_wraps_signed_24bit_deltas() {
        for (input, delta, output) in [
            (65535, 1, 65536),
            (65536, -1, 65535),
            (0, -1, 0xffffff),
            (0xffffff, 1, 0),
            (0x800000, -0x800000, 0),
            (0, 0x7fffff, 0x7fffff),
        ] {
            let subtable = BeBuffer::new()
                .push(3u16)
                .push(9u32)
                .push(Int24::new(delta))
                .push(3u16)
                .push(Uint24::new(1))
                .push(GlyphId24::new(input));
            let mut expected = vec![input, output];
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(close(&subtable, &[input]), expected);
        }
    }

    #[test]
    fn single_subst4_closure_repeats_until_stable() {
        let subtable = BeBuffer::new()
            .push(4u16)
            .push(18u32)
            .push(Uint24::new(3))
            .extend([65536, 0, 0xffffff].map(GlyphId24::new))
            .push(3u16)
            .push(Uint24::new(3))
            .extend([65535, 65536, 70000].map(GlyphId24::new));
        assert_eq!(
            close(&subtable, &[65535, 70000]),
            [0, 65535, 65536, 70000, 0xffffff]
        );
    }

    #[test]
    fn single_subst4_closure_keeps_full_width_coverage_indices() {
        let count = 65537u32;
        let subtable = BeBuffer::new()
            .push(4u16)
            .push(9 + count * 3)
            .push(Uint24::new(count))
            .extend((0..count).map(|gid| GlyphId24::new(if gid == 65536 { 70000 } else { 0 })))
            .push(4u16)
            .push(Uint24::new(1))
            .extend([0, 65536, 0].map(Uint24::new));
        assert_eq!(close(&subtable, &[65536]), [65536, 70000]);
    }

    #[test]
    fn single_subst_wide_null_and_truncated_tables() {
        for subtable in [
            BeBuffer::new()
                .push(3u16)
                .push(0u32)
                .push(Int24::new(1))
                .to_vec(),
            BeBuffer::new()
                .push(4u16)
                .push(0u32)
                .push(Uint24::new(0))
                .to_vec(),
        ] {
            let bytes = gsub_with_lookup(1, &subtable);
            let gsub = Gsub::read(FontData::new(&bytes)).unwrap();
            let mut glyphs: IntSet<_> = [GlyphId::new(65536)].into_iter().collect();
            let mut lookups: IntSet<_> = [0].into_iter().collect();
            gsub.closure_lookups(&glyphs, &mut lookups).unwrap();
            assert!(lookups.is_empty());
            gsub.closure_glyphs(&IntSet::all(), &mut glyphs).unwrap();
            assert_eq!(glyphs.iter().collect::<Vec<_>>(), [GlyphId::new(65536)]);
        }
        let truncated = BeBuffer::new()
            .push(4u16)
            .push(12u32)
            .push(Uint24::new(2))
            .push(GlyphId24::new(70000));
        let bytes = gsub_with_lookup(1, &truncated);
        let gsub = Gsub::read(FontData::new(&bytes)).unwrap();
        let mut glyphs = [GlyphId::new(65536)].into_iter().collect();
        assert!(gsub.closure_glyphs(&IntSet::all(), &mut glyphs).is_err());
    }

    #[test]
    fn wide_multiple_and_alternate_closure_is_transitive() {
        let subtable = BeBuffer::new()
            .push(2u16)
            .push(15u32)
            .push(Uint24::new(2))
            .extend([26, 34].map(Uint24::new))
            .push(3u16)
            .push(Uint24::new(2))
            .extend([65536, 70000].map(GlyphId24::new))
            .push(2u16)
            .extend([70000, 65535].map(GlyphId24::new))
            .push(1u16)
            .push(GlyphId24::new(0xffffff));
        for kind in [2, 3] {
            assert_eq!(
                close_lookup(kind, &subtable, &[65536]),
                [65535, 65536, 70000, 0xffffff]
            );
            assert_eq!(
                close_lookup(kind, &subtable, &[65536, 70000]),
                [65535, 65536, 70000, 0xffffff]
            );
        }
    }

    #[test]
    fn wide_multiple_and_alternate_closure_keeps_large_indices_and_offsets() {
        let count = 65537u32;
        let coverage_offset = 9 + count * 3;
        let sequence_offset = coverage_offset + 14;
        let subtable = BeBuffer::new()
            .push(2u16)
            .push(coverage_offset)
            .push(Uint24::new(count))
            .extend(
                (0..count)
                    .map(|index| Uint24::new(if index == 65536 { sequence_offset } else { 0 })),
            )
            .push(4u16)
            .push(Uint24::new(1))
            .extend([0, 65536, 0].map(Uint24::new))
            .push(1u16)
            .push(GlyphId24::new(70000));
        for kind in [2, 3] {
            assert_eq!(close_lookup(kind, &subtable, &[65536]), [65536, 70000]);
        }
    }

    #[test]
    fn wide_multiple_and_alternate_closure_skips_null_and_invalid_sets() {
        for kind in [2, 3] {
            for offset in [0, 0xffffff] {
                let subtable = BeBuffer::new()
                    .push(2u16)
                    .push(12u32)
                    .push(Uint24::new(1))
                    .push(Uint24::new(offset))
                    .push(3u16)
                    .push(Uint24::new(1))
                    .push(GlyphId24::new(65536));
                assert_eq!(close_lookup(kind, &subtable, &[65536]), [65536]);
            }
        }
    }
}
