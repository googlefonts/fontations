//! the [VARC (Variable Composite/Component)](https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md) table

use super::variations::PackedDeltas;
pub use super::{
    layout::{Condition, CoverageTable},
    postscript::Index2,
};

include!("../../generated/generated_varc.rs");

trait RawNth<'a> {
    fn nth(self, nth: usize) -> Result<&'a [u8], ReadError>;
}

impl<'a> RawNth<'a> for Option<Result<Index2<'a>, ReadError>> {
    fn nth(self, nth: usize) -> Result<&'a [u8], ReadError> {
        let Some(index) = self else {
            return Err(ReadError::InvalidCollectionIndex(nth as u32));
        };
        let index = index?;
        index.get(nth).map_err(|_| ReadError::OutOfBounds)
    }
}

impl<'a> Varc<'a> {
    /// Friendlier accessor than directly using raw data via [Index2]
    pub fn axis_indices(&self, nth: usize) -> Result<PackedDeltas, ReadError> {
        let raw = self.axis_indices_list().nth(nth)?;
        Ok(PackedDeltas::consume_all(raw.into()))
    }

    /// Friendlier accessor than directly using raw data via [Index2]
    ///
    /// nth would typically be obtained by looking up a [GlyphId] in [Self::coverage].
    pub fn glyph(&self, nth: usize) -> Result<VarcGlyph<'_>, ReadError> {
        let raw = Some(self.var_composite_glyphs()).nth(nth)?;
        Ok(VarcGlyph {
            table: self,
            data: raw.into(),
        })
    }
}

/// I'm just a happy little blob full of components
pub struct VarcGlyph<'a> {
    table: &'a Varc<'a>,
    data: FontData<'a>,
}

impl<'a> VarcGlyph<'a> {
    /// <https://github.com/fonttools/fonttools/blob/5e6b12d12fa08abafbeb7570f47707fbedf69a45/Lib/fontTools/ttLib/tables/otTables.py#L404-L409>
    pub fn components(&self) -> impl Iterator<Item = Result<VarcComponent<'a>, ReadError>> {
        VarcComponentIter {
            table: self.table,
            cursor: self.data.cursor(),
        }
    }
}

struct VarcComponentIter<'a> {
    table: &'a Varc<'a>,
    cursor: Cursor<'a>,
}

impl<'a> Iterator for VarcComponentIter<'a> {
    type Item = Result<VarcComponent<'a>, ReadError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cursor.is_empty() {
            return None;
        }
        Some(VarcComponent::parse(self.table, &mut self.cursor))
    }
}

#[allow(dead_code)] // TEMPORARY
pub struct VarcComponent<'a> {
    flags: VarcFlags,
    gid: GlyphId,
    condition_index: Option<u32>,
    axis_indices_index: Option<u32>,
    axis_values: Option<PackedDeltas<'a>>,
    axis_values_var_index: Option<u32>,
    transform_var_index: Option<u32>,
    dx: i16,
    dy: i16,
    rotation: f32,
    sx: f32,
    sy: f32,
    skewx: f32,
    skewy: f32,
    center_x: i16,
    center_y: i16,
}

impl<'a> VarcComponent<'a> {
    /// Requires access to VARC fields to fully parse.
    ///
    ///  * HarfBuzz [VarComponent::get_path_at](https://github.com/harfbuzz/harfbuzz/blob/0c2f5ecd51d11e32836ee136a1bc765d650a4ec0/src/OT/Var/VARC/VARC.cc#L132)
    // TODO: do we want to be able to parse into an existing glyph to avoid allocation?
    fn parse(table: &Varc, cursor: &mut Cursor<'a>) -> Result<Self, ReadError> {
        let raw_flags = cursor.read_u32_var()?;
        let flags = VarcFlags::from_bits_truncate(raw_flags);
        // Ref https://github.com/harfbuzz/boring-expansion-spec/blob/main/VARC.md#variable-component-record

        // This is a GlyphID16 if GID_IS_24BIT bit of flags is clear, else GlyphID24.
        let gid = if flags.contains(VarcFlags::GID_IS_24BIT) {
            let gid = cursor.read_be::<Uint24>()?.get().to_u32();
            if gid > u16::MAX as u32 {
                return Err(ReadError::BigGlyphIdsNotSupported(gid));
            }
            GlyphId::new(gid as u16)
        } else {
            GlyphId::new(cursor.read_be::<u16>()?.get())
        };

        let condition_index = if flags.contains(VarcFlags::HAVE_CONDITION) {
            Some(cursor.read_u32_var()?)
        } else {
            None
        };

        let (axis_indices_index, axis_values) = if flags.contains(VarcFlags::HAVE_AXES) {
            // <https://github.com/harfbuzz/harfbuzz/blob/0c2f5ecd51d11e32836ee136a1bc765d650a4ec0/src/OT/Var/VARC/VARC.cc#L195-L206>
            let axis_indices_index = cursor.read_u32_var()?;
            let num_axis_values = table.axis_indices(axis_indices_index as usize)?.count();
            // we need to consume num_axis_values entries in packed delta format
            let deltas = if num_axis_values > 0 {
                let Some(data) = cursor.remaining() else {
                    return Err(ReadError::OutOfBounds);
                };
                let deltas = PackedDeltas::new(data, num_axis_values);
                *cursor = deltas.iter().end(); // jump past the packed deltas
                Some(deltas)
            } else {
                None
            };
            (Some(axis_indices_index), deltas)
        } else {
            (None, None)
        };

        let axis_values_var_index = flags
            .contains(VarcFlags::AXIS_VALUES_HAVE_VARIATION)
            .then(|| cursor.read_u32_var())
            .transpose()?;

        let transform_var_index = if flags.contains(VarcFlags::TRANSFORM_HAS_VARIATION) {
            Some(cursor.read_u32_var()?)
        } else {
            None
        };

        let dx = if flags.contains(VarcFlags::HAVE_TRANSLATE_X) {
            cursor.read::<FWord>()?.to_i16()
        } else {
            0
        };
        let dy = if flags.contains(VarcFlags::HAVE_TRANSLATE_Y) {
            cursor.read::<FWord>()?.to_i16()
        } else {
            0
        };

        let rotation = if flags.contains(VarcFlags::HAVE_ROTATION) {
            cursor.read::<F4Dot12>()?.to_f32()
        } else {
            0.0
        };

        let sx = if flags.contains(VarcFlags::HAVE_SCALE_X) {
            cursor.read::<F6Dot10>()?.to_f32()
        } else {
            1.0
        };
        let sy = if flags.contains(VarcFlags::HAVE_SCALE_Y) {
            cursor.read::<F6Dot10>()?.to_f32()
        } else {
            sx
        };

        let skewx = if flags.contains(VarcFlags::HAVE_SKEW_X) {
            cursor.read::<F4Dot12>()?.to_f32()
        } else {
            0.0
        };
        let skewy = if flags.contains(VarcFlags::HAVE_SKEW_Y) {
            cursor.read::<F4Dot12>()?.to_f32()
        } else {
            0.0
        };

        let center_x = if flags.contains(VarcFlags::HAVE_TCENTER_X) {
            cursor.read::<FWord>()?.to_i16()
        } else {
            0
        };
        let center_y = if flags.contains(VarcFlags::HAVE_TCENTER_Y) {
            cursor.read::<FWord>()?.to_i16()
        } else {
            0
        };

        // Optional, process and discard one uint32var per each set bit in RESERVED_MASK.
        let num_reserved = (raw_flags & VarcFlags::RESERVED_MASK.bits).count_ones();
        for _ in 0..num_reserved {
            cursor.read_u32_var()?;
        }
        Ok(VarcComponent {
            flags,
            gid,
            condition_index,
            axis_indices_index,
            axis_values,
            axis_values_var_index,
            transform_var_index,
            dx,
            dy,
            rotation,
            sx,
            sy,
            skewx,
            skewy,
            center_x,
            center_y,
        })
    }
}

#[cfg(test)]
mod tests {
    use types::GlyphId;

    use crate::{FontRef, ReadError, TableProvider};

    use super::{Condition, Varc};

    impl Varc<'_> {
        fn conditions(&self) -> impl Iterator<Item = Condition<'_>> {
            self.condition_list()
                .expect("A condition list is present")
                .expect("We could read the condition list")
                .conditions()
                .iter()
                .enumerate()
                .map(|(i, c)| c.unwrap_or_else(|e| panic!("condition {i} {e}")))
        }

        fn axis_indices_count(&self) -> Result<usize, ReadError> {
            let Some(axis_indices_list) = self.axis_indices_list() else {
                return Ok(0);
            };
            let axis_indices_list = axis_indices_list?;
            Ok(axis_indices_list.count() as usize)
        }
    }

    #[test]
    fn read_cjk_0x6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let table = font.varc().unwrap();
        table.coverage().unwrap(); // should have coverage
    }

    #[test]
    fn identify_all_conditional_types() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();

        // We should have all 5 condition types in order
        assert_eq!(
            (1..=5).collect::<Vec<_>>(),
            table.conditions().map(|c| c.format()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format1_axis_range() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format1AxisRange(condition)) =
            table.conditions().find(|c| c.format() == 1)
        else {
            panic!("No such item");
        };

        assert_eq!(
            (0, 0.5, 1.0),
            (
                condition.axis_index(),
                condition.filter_range_min_value().to_f32(),
                condition.filter_range_max_value().to_f32(),
            )
        );
    }

    #[test]
    fn read_condition_format2_variable_value() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format2VariableValue(condition)) =
            table.conditions().find(|c| c.format() == 2)
        else {
            panic!("No such item");
        };

        assert_eq!((1, 2), (condition.default_value(), condition.var_index(),));
    }

    #[test]
    fn read_condition_format3_and() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format3And(condition)) = table.conditions().find(|c| c.format() == 3)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format4_or() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format4Or(condition)) = table.conditions().find(|c| c.format() == 4)
        else {
            panic!("No such item");
        };

        // Should reference a format 1 and a format 2
        assert_eq!(
            vec![1, 2],
            condition
                .conditions()
                .iter()
                .map(|c| c.unwrap().format())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_condition_format5_negate() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        let Some(Condition::Format5Negate(condition)) =
            table.conditions().find(|c| c.format() == 5)
        else {
            panic!("No such item");
        };

        // Should reference a format 1
        assert_eq!(1, condition.condition().unwrap().format(),);
    }

    #[test]
    fn read_axis_indices_list() {
        let font = FontRef::new(font_test_data::varc::CONDITIONALS).unwrap();
        let table = font.varc().unwrap();
        assert_eq!(table.axis_indices_count().unwrap(), 2);
        assert_eq!(
            vec![2, 3, 4, 5, 6],
            table.axis_indices(1).unwrap().iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_glyph_6868() {
        let font = FontRef::new(font_test_data::varc::CJK_6868).unwrap();
        let gid = font.cmap().unwrap().map_codepoint(0x6868_u32).unwrap();
        let table = font.varc().unwrap();
        let idx = table.coverage().unwrap().get(gid).unwrap();

        let glyph = table.glyph(idx as usize).unwrap();
        assert_eq!(
            vec![GlyphId::new(2), GlyphId::new(5), GlyphId::new(7)],
            glyph
                .components()
                .map(|c| c.unwrap().gid)
                .collect::<Vec<_>>()
        );
    }
}
